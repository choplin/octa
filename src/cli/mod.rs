//! CLI schema and top-level dispatch.
use crate::store::{RepoScope, StateFilter, Store};
use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
mod issue;
mod label;
mod pr;
mod project;
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
    /// Manage configured Issue states.
    State {
        #[command(subcommand)]
        command: StateCommand,
    },
    /// Manage labels available to Issues or Projects.
    Label {
        #[command(subcommand)]
        command: LabelCommand,
    },
    /// Manage label groups available to Issues or Projects.
    LabelGroup {
        #[command(subcommand)]
        command: LabelGroupCommand,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum LabelTarget {
    Issue,
    Project,
}

#[derive(Subcommand)]
pub(crate) enum IssueCommand {
    /// Browse issues in a read-only terminal interface.
    Tui,
    /// Create a new issue.
    Create {
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        body: String,
        /// Initial configured state (defaults to the repo's starting state).
        #[arg(long)]
        state: Option<String>,
        /// Priority: 0=None, 1=Urgent, 2=High, 3=Medium, 4=Low.
        #[arg(long, default_value_t = 0)]
        priority: i64,
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
    },
    /// List issues.
    List {
        #[command(flatten)]
        state_filter: IssueListStateArgs,
        #[arg(long)]
        label: Option<String>,
        /// Filter by status type: backlog, unstarted, started, completed, canceled.
        #[arg(long)]
        status_type: Option<String>,
        /// Filter by priority (0 through 4).
        #[arg(long)]
        priority: Option<i64>,
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
    /// Set an issue state.
    SetState {
        number: i64,
        state: String,
        #[arg(long)]
        lease: Option<String>,
    },
    /// Set scalar Issue properties.
    Set {
        number: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
        /// Priority: 0=None, 1=Urgent, 2=High, 3=Medium, 4=Low.
        #[arg(long)]
        priority: Option<i64>,
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
#[group(multiple = false)]
pub(crate) struct IssueListStateArgs {
    /// List non-terminal issues (the default).
    #[arg(long)]
    open: bool,
    /// List terminal issues.
    #[arg(long)]
    closed: bool,
    /// List both terminal and non-terminal issues.
    #[arg(long)]
    all: bool,
    /// List issues whose configured state exactly matches this name.
    #[arg(long, value_name = "NAME")]
    state: Option<String>,
}

impl IssueListStateArgs {
    fn into_filter(self) -> (StateFilter, Option<String>) {
        if self.open {
            (StateFilter::Open, None)
        } else if self.closed {
            (StateFilter::Closed, None)
        } else if self.all {
            (StateFilter::All, None)
        } else if let Some(state) = self.state {
            (StateFilter::All, Some(state))
        } else {
            // Explicit --open and no selector have the same safe default.
            (StateFilter::Open, None)
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum ProjectCommand {
    /// Create a Project.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        summary: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long, default_value = "planned")]
        state: String,
        #[arg(long = "type", default_value = "unstarted")]
        status_type: String,
        #[arg(long, default_value_t = 0)]
        priority: i64,
        #[arg(long)]
        json: bool,
    },
    /// List Projects and full issue lifecycle tallies.
    List {
        /// Show only Projects whose status type is not completed or canceled.
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
    /// Set Project metadata.
    Set {
        project: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        priority: Option<i64>,
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
    /// Set the Project state and its status category.
    SetState {
        project: String,
        state: String,
        #[arg(long = "type")]
        status_type: String,
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
        /// Status type: backlog, unstarted, started, completed, canceled.
        #[arg(long = "type")]
        status_type: Option<String>,
        #[arg(long)]
        starting: bool,
        #[arg(long)]
        terminal: bool,
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
        #[arg(long, value_enum)]
        target: LabelTarget,
        #[arg(long)]
        group: Option<String>,
    },
    /// List labels.
    List {
        #[arg(long, value_enum)]
        target: LabelTarget,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum LabelGroupCommand {
    /// Create a label group.
    Create {
        name: String,
        #[arg(long, value_enum)]
        target: LabelTarget,
        #[arg(long)]
        selection: String,
    },
    /// List label groups.
    List {
        #[arg(long, value_enum)]
        target: LabelTarget,
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
            ConfigCommand::State { command } => state::run(&store, command).await,
            ConfigCommand::Label { command } => label::run(&store, command).await,
            ConfigCommand::LabelGroup { command } => label::run_group(&store, command).await,
        },
        TopCommand::Project { command } => project::run(&store, command).await,
        TopCommand::Milestone { command } => project::run_milestone(&store, command).await,
    }
}
