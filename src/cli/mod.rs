//! CLI schema and top-level dispatch.
use crate::store::{RepoScope, StateFilter, Store};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
mod issue;
mod label;
mod pr;
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
}

#[derive(Subcommand)]
pub(crate) enum IssueCommand {
    /// Create a new issue.
    Create {
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        body: String,
        #[arg(long)]
        json: bool,
    },
    /// List issues.
    List {
        #[arg(long, default_value = "open")]
        state: String,
        #[arg(long)]
        label: Option<String>,
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
    },
    /// Manage issue dependencies.
    Dep {
        #[command(subcommand)]
        command: DepCommand,
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
}

#[derive(Subcommand)]
pub(crate) enum DepCommand {
    /// Add a blocking dependency.
    Add { blocker: i64, blocked: i64 },
    /// Remove a blocking dependency.
    Rm { blocker: i64, blocked: i64 },
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
    }
}
