//! CLI schema and top-level dispatch.
use crate::store::{RepoScope, StateFilter, Store};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
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
    /// Manage issues.
    Issue {
        #[command(subcommand)]
        command: IssueCommand,
    },
    /// Manage configured issue states.
    State {
        #[command(subcommand)]
        command: StateCommand,
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
    /// Manage labels and label groups.
    Label {
        #[command(subcommand)]
        command: LabelCommand,
    },
    /// Manage finite, repository-scoped projects.
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
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
        #[arg(long, default_value = "open")]
        state: String,
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
    SetState { number: i64, state: String },
    /// Close an issue.
    Close { number: i64 },
    /// Reopen an issue.
    Reopen { number: i64 },
    /// Edit an issue.
    Edit {
        number: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
        /// Priority: 0=None, 1=Urgent, 2=High, 3=Medium, 4=Low.
        #[arg(long)]
        priority: Option<i64>,
    },
    /// Manage issue dependencies.
    Dep {
        #[command(subcommand)]
        command: DepCommand,
    },
    /// Manage symmetric issue relations.
    Relate {
        #[command(subcommand)]
        command: RelateCommand,
    },
    /// Lock an issue.
    Lock {
        number: i64,
        #[arg(long)]
        r#as: Option<String>,
    },
    /// Unlock an issue.
    Unlock {
        number: i64,
        #[arg(long)]
        r#as: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Attach a label to an issue.
    Label { number: i64, label: String },
    /// Remove a label from an issue.
    Unlabel { number: i64, label: String },
    /// Set or clear an issue's Project.
    Project {
        #[command(subcommand)]
        command: IssueProjectCommand,
    },
    /// Set or clear an issue's Project milestone.
    Milestone {
        #[command(subcommand)]
        command: IssueMilestoneCommand,
    },
    /// Set or clear an issue's parent.
    Parent {
        #[command(subcommand)]
        command: IssueParentCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum IssueProjectCommand {
    Set { number: i64, project: String },
    Clear { number: i64 },
}

#[derive(Subcommand)]
pub(crate) enum IssueMilestoneCommand {
    Set { number: i64, milestone: String },
    Clear { number: i64 },
}

#[derive(Subcommand)]
pub(crate) enum IssueParentCommand {
    Set { number: i64, parent: i64 },
    Clear { number: i64 },
}

#[derive(Subcommand)]
pub(crate) enum ProjectCommand {
    /// Manage ordered milestones (phases) within a Project.
    Milestone {
        #[command(subcommand)]
        command: ProjectMilestoneCommand,
    },
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
    /// Edit Project metadata.
    Edit {
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
pub(crate) enum ProjectMilestoneCommand {
    /// Create a milestone in a Project.
    Create {
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
        project: String,
        #[arg(long)]
        json: bool,
    },
    /// Show one milestone.
    Show {
        project: String,
        milestone: String,
        #[arg(long)]
        json: bool,
    },
    /// Edit milestone metadata or phase order.
    Edit {
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
        clear_start_date: bool,
        #[arg(long)]
        clear_target_date: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum DepCommand {
    /// Add a blocking dependency.
    Add { blocker: i64, blocked: i64 },
    /// Remove a blocking dependency.
    Rm { blocker: i64, blocked: i64 },
}

#[derive(Subcommand)]
pub(crate) enum RelateCommand {
    /// Relate two issues. Repeating the same pair is harmless.
    Add { first: i64, second: i64 },
    /// Remove a relation in either argument order.
    Rm { first: i64, second: i64 },
}

#[derive(Subcommand)]
pub(crate) enum StateCommand {
    /// List configured states.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Add a configured issue state.
    Add {
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
    /// Close a pull request.
    Close { number: i64 },
    /// Reopen a pull request.
    Reopen { number: i64 },
    /// Edit a pull request.
    Edit {
        number: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
    /// Link an existing issue and PR.
    Link { issue: i64, pr: i64 },
    /// Remove an existing issue/PR link.
    Unlink { issue: i64, pr: i64 },
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
    /// Edit a wiki page.
    Edit {
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
    /// Create a label group.
    Group {
        name: String,
        #[arg(long)]
        selection: String,
    },
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
    /// List label groups.
    Groups {
        #[arg(long)]
        json: bool,
    },
}
pub(crate) fn holder(value: Option<String>) -> String {
    value
        .or_else(|| std::env::var("OCTA_ACTOR").ok())
        .unwrap_or_else(|| "local".to_string())
}
pub(crate) fn parse_issue_state(value: &str) -> (StateFilter, Option<String>) {
    match value {
        "open" => (StateFilter::Open, None),
        "closed" => (StateFilter::Closed, None),
        "all" => (StateFilter::All, None),
        state => (StateFilter::All, Some(state.to_string())),
    }
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
        TopCommand::Issue { command } => issue::run(&store, command).await,
        TopCommand::State { command } => state::run(&store, command).await,
        TopCommand::Pr { command } => pr::run(&store, command).await,
        TopCommand::Wiki { command } => wiki::run(&store, command).await,
        TopCommand::Label { command } => label::run(&store, command).await,
        TopCommand::Project { command } => project::run(&store, command).await,
    }
}
