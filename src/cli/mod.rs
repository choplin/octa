//! CLI schema and top-level dispatch.
use crate::store::{IssueListSelector, RepoScope, StateFilter, StateType, Store};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use label::LabelTarget;
use std::path::PathBuf;
mod issue;
mod label;
mod output;
mod pr;
mod project;
mod project_state;
mod state;
mod wiki;

#[derive(Parser)]
#[command(
    name = "octa",
    version,
    about = "GitHub-style Issue / PR / Wiki collaboration, fully local"
)]
pub struct Cli {
    #[command(flatten)]
    scope: ScopeArgs,
    #[command(subcommand)]
    command: TopCommand,
}

#[derive(Args)]
struct ScopeArgs {
    /// Select an already-known repository by name.
    #[arg(long, global = true)]
    repo: Option<String>,
    /// Aggregate read-only commands across all repositories.
    #[arg(long, global = true, conflicts_with = "repo")]
    all_repos: bool,
}

impl ScopeArgs {
    fn to_scope(&self) -> RepoScope {
        if self.all_repos {
            RepoScope::All
        } else if let Some(name) = &self.repo {
            RepoScope::Named(name.clone())
        } else {
            RepoScope::Current
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
    Pr {
        #[command(subcommand)]
        command: PrCommand,
    },
    /// Manage wiki pages.
    Wiki {
        #[command(subcommand)]
        command: WikiCommand,
    },
    /// Manage repository configuration.
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
        #[arg(long)]
        label: Option<String>,
        /// Filter by Project id or name.
        #[arg(long)]
        project: Option<String>,
        /// Filter by milestone id or name within --project.
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
    /// Add a comment to an issue.
    Comment {
        number: i64,
        #[arg(long)]
        body: String,
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
        /// Move the issue to any configured state, of any type. This is the
        /// only unconstrained move; every other verb is narrowed to its type.
        #[arg(long = "as", value_name = "STATE")]
        as_state: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        project: Option<String>,
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
        #[arg(long)]
        pr: Option<i64>,
        #[arg(long)]
        lease: Option<String>,
    },
    /// Remove relationships or collection members.
    Remove {
        number: i64,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        blocker: Option<i64>,
        #[arg(long)]
        blocks: Option<i64>,
        #[arg(long)]
        related: Option<i64>,
        #[arg(long)]
        pr: Option<i64>,
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
pub(crate) struct IssueOpenArgs {
    #[arg(long)]
    title: String,
    #[arg(long, default_value = "")]
    body: String,
    /// An `open` state other than the type's default.
    #[arg(long = "as", value_name = "STATE")]
    as_state: Option<String>,
    /// Project id or name.
    #[arg(long)]
    project: Option<String>,
    /// Milestone id or name; requires an explicit --project.
    #[arg(long, requires = "project")]
    milestone: Option<String>,
    /// Parent issue number. The parent's Project is inherited when omitted.
    #[arg(long)]
    parent: Option<i64>,
    #[arg(long)]
    json: bool,
}

/// The `issue list` state selectors.
///
/// The three are mutually exclusive. A state name already determines its type,
/// so combining `--state` with `--state-type` could only be redundant or empty.
#[derive(Args)]
#[group(multiple = false)]
pub(crate) struct IssueListStateArgs {
    /// Configured state names, comma-separated. Matches any of them.
    #[arg(long, value_name = "NAMES", value_delimiter = ',')]
    state: Vec<String>,
    /// State types, comma-separated: open, in progress, closed.
    #[arg(long, value_name = "TYPES", value_delimiter = ',')]
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
        /// Move the Project to any configured state, of any type. This is the
        /// only unconstrained move; every other verb is narrowed to its type.
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
        #[arg(long)]
        label: String,
    },
    /// Remove relationships or collection members.
    Remove {
        project: String,
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
        /// State type: open, in progress, or closed.
        #[arg(long = "type", value_name = "TYPE", default_value = "open")]
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
        /// New state type: open, in progress, or closed.
        #[arg(long = "type", value_name = "TYPE")]
        state_type: Option<String>,
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
        /// State type: open or closed.
        #[arg(long = "type", value_name = "TYPE", default_value = "open")]
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
        /// New state type: open or closed.
        #[arg(long = "type", value_name = "TYPE")]
        state_type: Option<String>,
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
pub(crate) enum PrCommand {
    /// Create a pull request.
    Create {
        #[arg(long)]
        title: String,
        #[arg(long)]
        branch: String,
        #[arg(long, default_value = "")]
        body: String,
        /// Link the new PR to an issue atomically.
        #[arg(long)]
        issue: Option<i64>,
        #[arg(long, requires = "issue")]
        lease: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List pull requests.
    List {
        #[arg(long, default_value = "open")]
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
        #[arg(long)]
        selection: String,
    },
    /// List label groups.
    List {
        #[arg(long)]
        json: bool,
    },
}
pub(crate) fn parse_pr_state(value: &str) -> Result<StateFilter> {
    match value {
        "open" => Ok(StateFilter::Open),
        "closed" => Ok(StateFilter::Closed),
        "all" => Ok(StateFilter::All),
        state => anyhow::bail!("unknown --state {state:?}; use open, closed, or all"),
    }
}

pub async fn run(cli: Cli) -> Result<()> {
    // Configuration is global. A repository selector would suggest the command
    // targets one repository's settings, so reject it instead of ignoring it.
    if matches!(cli.command, TopCommand::Config { .. }) {
        if cli.scope.all_repos {
            anyhow::bail!("configuration is global; --all-repos does not apply to `octa config`");
        }
        if cli.scope.repo.is_some() {
            anyhow::bail!("configuration is global; --repo does not apply to `octa config`");
        }
    }
    let store = Store::open(cli.scope.to_scope()).await?;
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
        TopCommand::Issue { command } => issue::run(&store, command).await,
        TopCommand::Pr { command } => pr::run(&store, command).await,
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
