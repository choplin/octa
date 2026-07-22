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
    #[arg(long, global = true)]
    repo: Option<String>,
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
    Issue {
        #[command(subcommand)]
        command: IssueCommand,
    },
    State {
        #[command(subcommand)]
        command: StateCommand,
    },
    Pr {
        #[command(subcommand)]
        command: PrCommand,
    },
    Wiki {
        #[command(subcommand)]
        command: WikiCommand,
    },
    Label {
        #[command(subcommand)]
        command: LabelCommand,
    },
}
#[derive(Subcommand)]
pub(crate) enum IssueCommand {
    Create {
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        body: String,
        #[arg(long)]
        json: bool,
    },
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
    Show {
        number: i64,
        #[arg(long)]
        json: bool,
    },
    Comment {
        number: i64,
        #[arg(long)]
        body: String,
    },
    SetState {
        number: i64,
        state: String,
    },
    Close {
        number: i64,
    },
    Reopen {
        number: i64,
    },
    Edit {
        number: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
    Dep {
        #[command(subcommand)]
        command: DepCommand,
    },
    Lock {
        number: i64,
        #[arg(long)]
        r#as: Option<String>,
    },
    Unlock {
        number: i64,
        #[arg(long)]
        r#as: Option<String>,
        #[arg(long)]
        force: bool,
    },
    Label {
        number: i64,
        label: String,
    },
    Unlabel {
        number: i64,
        label: String,
    },
}
#[derive(Subcommand)]
pub(crate) enum DepCommand {
    Add { blocker: i64, blocked: i64 },
    Rm { blocker: i64, blocked: i64 },
}
#[derive(Subcommand)]
pub(crate) enum StateCommand {
    List {
        #[arg(long)]
        json: bool,
    },
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
    List {
        #[arg(long, default_value = "open")]
        state: String,
        #[arg(long)]
        json: bool,
    },
    Show {
        number: i64,
        #[arg(long)]
        json: bool,
    },
    Comment {
        number: i64,
        #[arg(long)]
        body: String,
    },
    SetState {
        number: i64,
        state: String,
    },
    Close {
        number: i64,
    },
    Reopen {
        number: i64,
    },
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
    Create {
        #[arg(long)]
        title: String,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long, default_value = "")]
        body: String,
    },
    Edit {
        slug: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
    Show {
        slug: String,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        json: bool,
    },
}
#[derive(Subcommand)]
pub(crate) enum LabelCommand {
    Group {
        name: String,
        #[arg(long)]
        selection: String,
    },
    Create {
        name: String,
        #[arg(long)]
        group: Option<String>,
    },
    List {
        #[arg(long)]
        json: bool,
    },
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
