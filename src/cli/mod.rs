//! CLI schema and top-level dispatch.
use crate::domain::label::LabelSelection;
use crate::domain::project::ProjectStateType;
use crate::store::{IssueListSelector, RepositoryScope, StateFilter, StateType, Store};
use anyhow::{Context, Result};
use clap::builder::PossibleValuesParser;
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use label::LabelTarget;
use std::path::PathBuf;
mod help_values;
mod issue;
mod label;
mod output;
mod project;
mod project_state;
mod pull_request;
mod repository;
mod state;
mod text_input;
mod wiki;

use text_input::TextInput;

// Closed value sets are advertised in `--help` from the same constant the
// parser rejects against, so a set can never gain a value the help omits.
// Sets that live in configuration are filled in at help time by `help_values`.

/// Parse the command line, naming configured values in help when asked for it.
pub async fn parse() -> Cli {
    let mut command = Cli::command();
    if help_values::wants_help(std::env::args_os()) {
        if let Some(values) = help_values::load().await {
            command = help_values::augment(command, &values);
        }
        command = help_values::quote_possible_values(command);
    }
    let matches = command.get_matches();
    match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    }
}

fn issue_state_types() -> PossibleValuesParser {
    PossibleValuesParser::new(StateType::VALUES.map(StateType::as_str))
}

fn project_state_types() -> PossibleValuesParser {
    PossibleValuesParser::new(ProjectStateType::VALUES.map(ProjectStateType::as_str))
}

fn label_selections() -> PossibleValuesParser {
    PossibleValuesParser::new(LabelSelection::VALUES.map(LabelSelection::as_str))
}

fn pull_request_state_filters() -> PossibleValuesParser {
    PossibleValuesParser::new(StateFilter::VALUES.map(StateFilter::as_str))
}

#[derive(Parser)]
#[command(
    name = "octa",
    version,
    about = "GitHub-style Issue / pull request / Wiki collaboration, fully local"
)]
pub struct Cli {
    #[command(flatten)]
    scope: ScopeArgs,
    #[command(subcommand)]
    command: TopCommand,
}

impl Cli {
    fn validate(&mut self) -> Result<()> {
        let TopCommand::Issue {
            command: IssueCommand::Comment { args, .. },
        } = &mut self.command
        else {
            return Ok(());
        };

        args.validate()
    }
}

#[derive(Args)]
struct ScopeArgs {
    /// Select an already-known repository by name; `octa repository list` names them.
    #[arg(long, alias = "repo", global = true)]
    repository: Option<String>,
    /// Aggregate read-only commands across all repositories.
    #[arg(
        long,
        alias = "all-repos",
        global = true,
        conflicts_with = "repository"
    )]
    all_repositories: bool,
}

impl ScopeArgs {
    fn to_scope(&self) -> RepositoryScope {
        if self.all_repositories {
            RepositoryScope::All
        } else if let Some(name) = &self.repository {
            RepositoryScope::Named(name.clone())
        } else {
            RepositoryScope::Current
        }
    }
}

#[derive(Subcommand)]
enum TopCommand {
    /// Execute a read-only GraphQL query.
    Query {
        /// Read the GraphQL document from this file instead of stdin.
        #[arg(long)]
        file: Option<PathBuf>,
        /// Variables as a JSON object.
        #[arg(long)]
        variables: Option<String>,
        /// Print the versioned public schema instead of executing a document.
        #[arg(long, conflicts_with_all = ["file", "variables"])]
        schema: bool,
    },
    /// Manage issues.
    Issue {
        #[command(subcommand)]
        command: IssueCommand,
    },
    /// Manage pull requests.
    #[command(alias = "pr")]
    PullRequest {
        #[command(subcommand)]
        command: PullRequestCommand,
    },
    /// Manage wiki pages.
    Wiki {
        #[command(subcommand)]
        command: WikiCommand,
    },
    /// Configure Issue and Project states, labels, and label groups.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Manage finite, repository-scoped projects.
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    /// Manage Project milestones.
    Milestone {
        #[command(subcommand)]
        command: MilestoneCommand,
    },
    /// Inspect the repositories octa has recorded.
    ///
    /// Registration is implicit: the first octa command run inside a Git
    /// repository records it, keyed by that repository's Git common directory.
    /// Every worktree of one repository therefore resolves to the same entry,
    /// and a repository appears here whether or not it holds any Issues.
    #[command(alias = "repo")]
    Repository {
        #[command(subcommand)]
        command: RepositoryCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum RepositoryCommand {
    /// List the repositories octa has recorded.
    List {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ConfigCommand {
    /// Configure Issues: their states, labels, and label groups.
    Issue {
        #[command(subcommand)]
        command: IssueConfigCommand,
    },
    /// Configure Projects: their states, labels, and label groups.
    Project {
        #[command(subcommand)]
        command: ProjectConfigCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum IssueConfigCommand {
    /// Manage the states Issues can be in.
    State {
        #[command(subcommand)]
        command: StateCommand,
    },
    /// Manage the labels Issues can carry.
    Label {
        #[command(subcommand)]
        command: LabelCommand,
    },
    /// Manage the label groups Issue labels belong to.
    LabelGroup {
        #[command(subcommand)]
        command: LabelGroupCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum ProjectConfigCommand {
    /// Manage the states Projects can be in.
    State {
        #[command(subcommand)]
        command: ProjectStateCommand,
    },
    /// Manage the labels Projects can carry.
    Label {
        #[command(subcommand)]
        command: LabelCommand,
    },
    /// Manage the label groups Project labels belong to.
    LabelGroup {
        #[command(subcommand)]
        command: LabelGroupCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum IssueCommand {
    /// Browse issues in a read-only terminal interface.
    Tui,
    /// Open a new issue.
    Open {
        #[command(flatten)]
        args: IssueOpenArgs,
    },
    /// Open a new issue. Alias of `open`.
    Create {
        #[command(flatten)]
        args: IssueOpenArgs,
    },
    /// List issues.
    List {
        #[command(flatten)]
        state_filter: IssueListStateArgs,
        /// Filter by label.
        #[arg(long)]
        label: Option<String>,
        /// Filter by Project id or name. List them with `octa project list`.
        #[arg(long)]
        project: Option<String>,
        /// Filter by milestone id or name within --project. List a Project's
        /// milestones with `octa project show <project>`.
        #[arg(long, requires = "project")]
        milestone: Option<String>,
        /// Filter to issues related to this issue number.
        #[arg(long)]
        related_to: Option<i64>,
        #[arg(long)]
        unblocked: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show an issue and its discussion.
    Show {
        number: i64,
        #[arg(long)]
        json: bool,
    },
    /// Add or delete an issue comment.
    #[command(
        after_help = "Examples:\n  octa issue comment 42 --body \"Looks good\"\n  octa issue comment 42 --body-file comment.md\n  octa issue comment 42 --body-file -\n  octa issue comment 42 --delete 7 --lease <LEASE>"
    )]
    Comment {
        number: i64,
        #[command(flatten)]
        args: IssueCommentArgs,
    },
    /// Move an issue to the `in progress` default state.
    ///
    /// `start` takes no `--as`: picking work up says nothing yet about where it
    /// will land.
    Start {
        number: i64,
        #[arg(long)]
        lease: Option<String>,
    },
    /// Close an issue.
    Close {
        number: i64,
        /// A `closed` state other than the default, such as a not-planned one.
        #[arg(long = "as", value_name = "STATE")]
        as_state: Option<String>,
        #[arg(long)]
        lease: Option<String>,
    },
    /// Reopen a closed issue.
    Reopen {
        number: i64,
        /// An `open` state other than the default.
        #[arg(long = "as", value_name = "STATE")]
        as_state: Option<String>,
        #[arg(long)]
        lease: Option<String>,
    },
    /// Set scalar Issue properties.
    Set {
        number: i64,
        /// Move the issue to any available state.
        #[arg(long = "as", value_name = "STATE")]
        as_state: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[command(flatten)]
        body: BodyInputArgs,
        /// Project id or name. List them with `octa project list`.
        #[arg(long)]
        project: Option<String>,
        /// Milestone id or name within the issue's Project. List a Project's
        /// milestones with `octa project show <project>`.
        #[arg(long)]
        milestone: Option<String>,
        #[arg(long)]
        parent: Option<i64>,
        #[arg(long)]
        lease: Option<String>,
    },
    /// Unset optional scalar Issue properties.
    Unset {
        number: i64,
        #[arg(long)]
        project: bool,
        #[arg(long)]
        milestone: bool,
        #[arg(long)]
        parent: bool,
        #[arg(long)]
        lease: Option<String>,
    },
    /// Add relationships or collection members.
    Add {
        number: i64,
        /// Attach a label.
        #[arg(long)]
        label: Option<String>,
        /// Add an Issue that blocks this Issue.
        #[arg(long)]
        blocker: Option<i64>,
        /// Add an Issue blocked by this Issue.
        #[arg(long)]
        blocks: Option<i64>,
        #[arg(long)]
        related: Option<i64>,
        #[arg(long, alias = "pr")]
        pull_request: Option<i64>,
        #[arg(long)]
        lease: Option<String>,
    },
    /// Remove relationships or collection members.
    Remove {
        number: i64,
        /// Detach a label.
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        blocker: Option<i64>,
        #[arg(long)]
        blocks: Option<i64>,
        #[arg(long)]
        related: Option<i64>,
        #[arg(long, alias = "pr")]
        pull_request: Option<i64>,
        #[arg(long)]
        lease: Option<String>,
    },
    /// Lock an issue.
    Lock { number: i64 },
    /// Unlock an issue.
    Unlock {
        number: i64,
        #[arg(long, required_unless_present = "force", conflicts_with = "force")]
        lease: Option<String>,
        #[arg(long, conflicts_with = "lease")]
        force: bool,
    },
}

#[derive(Args)]
pub(crate) struct IssueCommentArgs {
    #[command(flatten)]
    body: BodyInputArgs,
    /// Delete a comment by its ID.
    #[arg(
        long,
        value_name = "COMMENT_ID",
        requires = "lease",
        conflicts_with_all = ["body", "body_file"]
    )]
    delete: Option<i64>,
    /// Issue lease required when deleting a comment.
    #[arg(long, requires = "delete")]
    lease: Option<String>,
    #[arg(skip)]
    action: Option<IssueCommentAction>,
}

impl IssueCommentArgs {
    fn validate(&mut self) -> Result<()> {
        let action = if self.delete.is_some() {
            self.validate_delete()?
        } else {
            self.validate_add()?
        };
        self.action = Some(action);
        Ok(())
    }

    fn validate_add(&mut self) -> Result<IssueCommentAction> {
        if self.lease.is_some() {
            anyhow::bail!("--lease can be used only with --delete");
        }
        let body = std::mem::take(&mut self.body)
            .resolve()?
            .context("comment body is required")?;
        Ok(IssueCommentAction::Add { body })
    }

    fn validate_delete(&mut self) -> Result<IssueCommentAction> {
        if self.body.is_present() {
            anyhow::bail!("--body/--body-file cannot be used with --delete");
        }
        let comment = self
            .delete
            .take()
            .context("comment ID is required with --delete")?;
        let lease = self
            .lease
            .take()
            .context("--lease is required with --delete")?;
        Ok(IssueCommentAction::Delete { comment, lease })
    }

    pub(crate) fn into_action(self) -> Result<IssueCommentAction> {
        self.action
            .ok_or_else(|| anyhow::anyhow!("issue comment options were not validated"))
    }
}

pub(crate) enum IssueCommentAction {
    Add { body: String },
    Delete { comment: i64, lease: String },
}

#[derive(Args)]
pub(crate) struct IssueOpenArgs {
    #[arg(long)]
    title: String,
    #[command(flatten)]
    body: BodyInputArgs,
    /// An `open` state other than the type's default.
    #[arg(long = "as", value_name = "STATE")]
    as_state: Option<String>,
    /// Project id or name. List them with `octa project list`.
    #[arg(long)]
    project: Option<String>,
    /// Milestone id or name; requires an explicit --project. List a Project's
    /// milestones with `octa project show <project>`.
    #[arg(long, requires = "project")]
    milestone: Option<String>,
    /// Parent issue number. The parent's Project is inherited when omitted.
    #[arg(long)]
    parent: Option<i64>,
    #[arg(long)]
    json: bool,
}

#[derive(Args, Default)]
#[group(multiple = false)]
pub(crate) struct BodyInputArgs {
    /// Use this text as the body.
    #[arg(long)]
    body: Option<String>,
    /// Read the body from PATH; use `-` for stdin.
    #[arg(long, value_name = "PATH")]
    body_file: Option<PathBuf>,
}

impl BodyInputArgs {
    fn is_present(&self) -> bool {
        self.body.is_some() || self.body_file.is_some()
    }

    fn resolve(self) -> Result<Option<String>> {
        TextInput::from_options(self.body, self.body_file)?
            .map(|input| input.read("Issue text"))
            .transpose()
    }
}
/// The `issue list` state selectors.
///
/// The three are mutually exclusive. A state name already determines its type,
/// so combining `--state` with `--state-type` could only be redundant or empty.
#[derive(Args)]
#[group(multiple = false)]
pub(crate) struct IssueListStateArgs {
    /// State names, comma-separated. Matches any of them.
    #[arg(long, value_name = "NAMES", value_delimiter = ',')]
    state: Vec<String>,
    /// State types, comma-separated.
    #[arg(
        long,
        value_name = "TYPES",
        value_delimiter = ',',
        value_parser = issue_state_types(),
    )]
    state_type: Vec<String>,
    /// List issues in every state, including closed ones.
    #[arg(long)]
    all: bool,
}

impl IssueListStateArgs {
    fn into_selector(self) -> Result<IssueListSelector> {
        if self.all {
            Ok(IssueListSelector::All)
        } else if !self.state.is_empty() {
            Ok(IssueListSelector::States(self.state))
        } else if !self.state_type.is_empty() {
            let types = self
                .state_type
                .iter()
                .map(|value| StateType::parse(value))
                .collect::<Result<Vec<_>>>()?;
            Ok(IssueListSelector::Types(types))
        } else {
            // Omitting a selector hides closed work. Perfect orthogonality with
            // --all would cost more than it buys in a listing read every day.
            Ok(IssueListSelector::default())
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum ProjectCommand {
    /// Create a Project.
    Create {
        #[arg(long)]
        name: String,
        /// Open the Project in an open-type state other than the default.
        #[arg(long = "as", value_name = "STATE")]
        as_state: Option<String>,
        #[arg(long, default_value = "")]
        summary: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        json: bool,
    },
    /// List Projects and full issue lifecycle tallies.
    List {
        /// Show only Projects that are not closed.
        #[arg(long)]
        active: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show a Project with issue status tallies.
    Show {
        project: String,
        #[arg(long)]
        json: bool,
    },
    /// Set scalar Project properties.
    Set {
        project: String,
        /// Move the Project to any available state.
        #[arg(long = "as", value_name = "STATE")]
        as_state: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Add relationships or collection members.
    Add {
        project: String,
        /// Attach a label.
        #[arg(long)]
        label: String,
    },
    /// Remove relationships or collection members.
    Remove {
        project: String,
        /// Detach a label.
        #[arg(long)]
        label: String,
    },
    /// Close a Project: `project list --active` stops showing it.
    Close {
        project: String,
        /// Close into a closed-type state other than the default.
        #[arg(long = "as", value_name = "STATE")]
        as_state: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Reopen a closed Project.
    Reopen {
        project: String,
        /// Reopen into an open-type state other than the default.
        #[arg(long = "as", value_name = "STATE")]
        as_state: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum MilestoneCommand {
    /// Create a milestone in a Project.
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long, default_value = "planned")]
        status: String,
        #[arg(long)]
        position: Option<i64>,
        #[arg(long)]
        start_date: Option<String>,
        #[arg(long)]
        target_date: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List milestones in stable phase order.
    List {
        #[arg(long)]
        project: String,
        #[arg(long)]
        json: bool,
    },
    /// Show one milestone.
    Show {
        #[arg(long)]
        project: String,
        milestone: String,
        #[arg(long)]
        json: bool,
    },
    /// Set milestone metadata or phase order.
    Set {
        #[arg(long)]
        project: String,
        milestone: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        position: Option<i64>,
        #[arg(long)]
        start_date: Option<String>,
        #[arg(long)]
        target_date: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Unset optional milestone properties.
    Unset {
        milestone: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        start_date: bool,
        #[arg(long)]
        target_date: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum StateCommand {
    /// List configured states.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Create a configured issue state.
    Create {
        name: String,
        /// State type.
        #[arg(
            long = "type",
            value_name = "TYPE",
            default_value = "open",
            value_parser = issue_state_types(),
        )]
        state_type: String,
        /// Make this its type's default, replacing the current one.
        #[arg(long)]
        default: bool,
    },
    /// Update a configured issue state. Renaming moves its issues with it.
    Set {
        name: String,
        /// New name for the state.
        #[arg(long = "name")]
        new_name: Option<String>,
        /// Make this its type's default, replacing the current one.
        #[arg(long)]
        default: bool,
    },
    /// Delete a configured issue state.
    Delete {
        name: String,
        /// State to move this state's issues to before deleting it.
        #[arg(long)]
        move_to: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum ProjectStateCommand {
    /// List configured states.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Create a configured project state.
    Create {
        name: String,
        /// State type.
        #[arg(
            long = "type",
            value_name = "TYPE",
            default_value = "open",
            value_parser = project_state_types(),
        )]
        state_type: String,
        /// Make this its type's default, replacing the current one.
        #[arg(long)]
        default: bool,
    },
    /// Update a configured project state. Renaming moves its projects with it.
    Set {
        name: String,
        /// New name for the state.
        #[arg(long = "name")]
        new_name: Option<String>,
        /// Make this its type's default, replacing the current one.
        #[arg(long)]
        default: bool,
    },
    /// Delete a configured project state.
    Delete {
        name: String,
        /// State to move this state's projects to before deleting it.
        #[arg(long)]
        move_to: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum PullRequestCommand {
    /// Create a pull request.
    Create {
        #[arg(long)]
        title: String,
        #[arg(long)]
        branch: String,
        #[arg(long, default_value = "")]
        body: String,
        /// Link the new pull request to an issue atomically.
        #[arg(long)]
        issue: Option<i64>,
        #[arg(long, requires = "issue")]
        lease: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List pull requests.
    List {
        /// Which pull requests to include.
        #[arg(long, default_value = "open", value_parser = pull_request_state_filters())]
        state: String,
        #[arg(long)]
        json: bool,
    },
    /// Show a pull request and its comments.
    Show {
        number: i64,
        #[arg(long)]
        json: bool,
    },
    /// Add a comment to a pull request.
    Comment {
        number: i64,
        #[arg(long)]
        body: String,
    },
    /// Set a pull request state.
    SetState { number: i64, state: String },
    /// Set pull request metadata.
    Set {
        number: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
    /// Add an Issue relationship.
    Add {
        number: i64,
        #[arg(long)]
        issue: i64,
        #[arg(long)]
        lease: Option<String>,
    },
    /// Remove an Issue relationship.
    Remove {
        number: i64,
        #[arg(long)]
        issue: i64,
        #[arg(long)]
        lease: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum WikiCommand {
    /// Create a wiki page.
    Create {
        #[arg(long)]
        title: String,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long, default_value = "")]
        body: String,
    },
    /// Set wiki page properties.
    Set {
        slug: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
    /// Show a wiki page and its links.
    Show {
        slug: String,
        #[arg(long)]
        json: bool,
    },
    /// List wiki pages.
    List {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum LabelCommand {
    /// Create a label.
    Create {
        name: String,
        /// Label group to put this label in.
        #[arg(long)]
        group: Option<String>,
    },
    /// List labels.
    List {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum LabelGroupCommand {
    /// Create a label group.
    Create {
        name: String,
        /// How many of the group's labels one record may carry.
        #[arg(long, value_parser = label_selections())]
        selection: String,
    },
    /// List label groups.
    List {
        #[arg(long)]
        json: bool,
    },
}
pub(crate) fn parse_pull_request_state(value: &str) -> Result<StateFilter> {
    StateFilter::parse(value)
}

pub async fn run(mut cli: Cli) -> Result<()> {
    // Validate cross-option semantics before opening the persistent store. Clap
    // handles the simple requirements and conflicts; this boundary covers the
    // operation-level combinations that would otherwise fail after migrations
    // and repository registration have already run.
    cli.validate()?;
    // Configuration is global. A repository selector would suggest the command
    // targets one repository's settings, so reject it instead of ignoring it.
    if matches!(cli.command, TopCommand::Config { .. }) {
        if cli.scope.all_repositories {
            anyhow::bail!(
                "configuration is global; --all-repositories does not apply to `octa config`"
            );
        }
        if cli.scope.repository.is_some() {
            anyhow::bail!("configuration is global; --repository does not apply to `octa config`");
        }
    }
    // `octa repository` reports on the store itself, so a repository selector has
    // nothing to select. Rejecting it says so; ignoring it would let a caller
    // believe the listing had been narrowed.
    if matches!(cli.command, TopCommand::Repository { .. }) {
        if cli.scope.all_repositories {
            anyhow::bail!(
                "`octa repository` already covers every repository; drop --all-repositories"
            );
        }
        if cli.scope.repository.is_some() {
            anyhow::bail!(
                "`octa repository` reports on the whole store; --repository does not apply"
            );
        }
    }
    // The listing is store-wide, so it resolves no current repository. That
    // keeps it usable outside a Git repository and stops a plain listing from
    // registering the repository the caller happens to be standing in.
    let scope = match cli.command {
        TopCommand::Repository { .. } => RepositoryScope::All,
        _ => cli.scope.to_scope(),
    };
    let store = Store::open(scope).await?;
    match cli.command {
        TopCommand::Query {
            file,
            variables,
            schema,
        } => {
            if schema {
                println!("{}", crate::query::schema_sdl(&store)?);
                Ok(())
            } else {
                let document = crate::query::read_document(file.as_deref())?;
                let response =
                    crate::query::execute(&store, document, variables.as_deref()).await?;
                println!("{}", serde_json::to_string(&response)?);
                Ok(())
            }
        }
        TopCommand::Repository { command } => repository::run(&store, command).await,
        TopCommand::Issue { command } => issue::run(&store, command).await,
        TopCommand::PullRequest { command } => pull_request::run(&store, command).await,
        TopCommand::Wiki { command } => wiki::run(&store, command).await,
        TopCommand::Config { command } => match command {
            ConfigCommand::Issue { command } => match command {
                IssueConfigCommand::State { command } => state::run(&store, command).await,
                IssueConfigCommand::Label { command } => {
                    label::run(&store, LabelTarget::Issue, command).await
                }
                IssueConfigCommand::LabelGroup { command } => {
                    label::run_group(&store, LabelTarget::Issue, command).await
                }
            },
            ConfigCommand::Project { command } => match command {
                ProjectConfigCommand::State { command } => {
                    project_state::run(&store, command).await
                }
                ProjectConfigCommand::Label { command } => {
                    label::run(&store, LabelTarget::Project, command).await
                }
                ProjectConfigCommand::LabelGroup { command } => {
                    label::run_group(&store, LabelTarget::Project, command).await
                }
            },
        },
        TopCommand::Project { command } => project::run(&store, command).await,
        TopCommand::Milestone { command } => project::run_milestone(&store, command).await,
    }
}
