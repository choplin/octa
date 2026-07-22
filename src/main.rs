//! octa: GitHub-style Issue / Pull Request / Wiki collaboration, fully local.

mod app;
mod domain;
mod sql;
mod store;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use store::{LockOutcome, RepoScope, StateFilter, Store};

#[derive(Parser)]
#[command(
    name = "octa",
    version,
    about = "GitHub-style Issue / PR / Wiki collaboration, fully local"
)]
struct Cli {
    #[command(flatten)]
    scope: ScopeArgs,
    #[command(subcommand)]
    command: TopCommand,
}

/// Repository scope, shared by every command.
#[derive(Args)]
struct ScopeArgs {
    /// Target a named repository instead of the current one.
    #[arg(long, global = true)]
    repo: Option<String>,
    /// Operate across every repository in the store (read/aggregate only).
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
    /// Manage issues
    Issue {
        #[command(subcommand)]
        command: IssueCommand,
    },
    /// Configure the issue state set (mechanism only)
    State {
        #[command(subcommand)]
        command: StateCommand,
    },
    /// Manage pull requests (branch-linked discussion entities)
    Pr {
        #[command(subcommand)]
        command: PrCommand,
    },
    /// Manage wiki pages
    Wiki {
        #[command(subcommand)]
        command: WikiCommand,
    },
    /// Manage labels and label groups
    Label {
        #[command(subcommand)]
        command: LabelCommand,
    },
}

#[derive(Subcommand)]
enum IssueCommand {
    /// Create a new issue
    Create {
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        body: String,
        #[arg(long)]
        json: bool,
    },
    /// List issues
    List {
        /// open | closed | all | <exact state name>
        #[arg(long, default_value = "open")]
        state: String,
        /// Only issues carrying this label
        #[arg(long)]
        label: Option<String>,
        /// Only unblocked issues (no incomplete blockers)
        #[arg(long)]
        unblocked: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show an issue with its labels, dependencies, and comments
    Show {
        number: i64,
        #[arg(long)]
        json: bool,
    },
    /// Add a comment to an issue
    Comment {
        number: i64,
        #[arg(long)]
        body: String,
    },
    /// Move an issue to a state
    SetState { number: i64, state: String },
    /// Close an issue (move to the default terminal state)
    Close { number: i64 },
    /// Reopen an issue (move to the default starting state)
    Reopen { number: i64 },
    /// Edit an issue's title and/or body
    Edit {
        number: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
    /// Manage dependency edges
    Dep {
        #[command(subcommand)]
        command: DepCommand,
    },
    /// Atomically acquire the exclusive lock
    Lock {
        number: i64,
        #[arg(long)]
        r#as: Option<String>,
    },
    /// Release the exclusive lock
    Unlock {
        number: i64,
        #[arg(long)]
        r#as: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Attach a label to an issue
    Label { number: i64, label: String },
    /// Detach a label from an issue
    Unlabel { number: i64, label: String },
}

#[derive(Subcommand)]
enum DepCommand {
    /// Record that BLOCKER blocks BLOCKED
    Add { blocker: i64, blocked: i64 },
    /// Remove a dependency edge
    Rm { blocker: i64, blocked: i64 },
}

#[derive(Subcommand)]
enum StateCommand {
    /// List the configured states
    List {
        #[arg(long)]
        json: bool,
    },
    /// Add a state to the set
    Add {
        name: String,
        #[arg(long)]
        starting: bool,
        #[arg(long)]
        terminal: bool,
    },
}

#[derive(Subcommand)]
enum PrCommand {
    /// Open a PR tied to a git branch
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
    /// List pull requests
    List {
        #[arg(long, default_value = "open")]
        state: String,
        #[arg(long)]
        json: bool,
    },
    /// Show a PR with its comments
    Show {
        number: i64,
        #[arg(long)]
        json: bool,
    },
    /// Add a comment to a PR
    Comment {
        number: i64,
        #[arg(long)]
        body: String,
    },
    /// Move a PR to a state (e.g. open, merged, closed)
    SetState { number: i64, state: String },
    /// Close a PR
    Close { number: i64 },
    /// Reopen a PR
    Reopen { number: i64 },
    /// Edit a PR's title and/or body
    Edit {
        number: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
}

#[derive(Subcommand)]
enum WikiCommand {
    /// Create a wiki page
    Create {
        #[arg(long)]
        title: String,
        /// Slug (defaults to a slugified title)
        #[arg(long)]
        slug: Option<String>,
        #[arg(long, default_value = "")]
        body: String,
    },
    /// Edit a wiki page
    Edit {
        slug: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
    /// Show a wiki page with its links and backlinks
    Show {
        slug: String,
        #[arg(long)]
        json: bool,
    },
    /// List wiki pages
    List {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum LabelCommand {
    /// Create a label group (single = mutually exclusive, multi = coexisting)
    Group {
        name: String,
        #[arg(long)]
        selection: String,
    },
    /// Create a label, optionally within a group
    Create {
        name: String,
        #[arg(long)]
        group: Option<String>,
    },
    /// List labels
    List {
        #[arg(long)]
        json: bool,
    },
    /// List label groups
    Groups {
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn holder(explicit: Option<String>) -> String {
    explicit
        .or_else(|| std::env::var("OCTA_ACTOR").ok())
        .unwrap_or_else(|| "local".to_string())
}

/// Map a `--state` string onto the coarse filter plus an optional exact name.
fn parse_issue_state(state: &str) -> (StateFilter, Option<String>) {
    match state {
        "open" => (StateFilter::Open, None),
        "closed" => (StateFilter::Closed, None),
        "all" => (StateFilter::All, None),
        other => (StateFilter::All, Some(other.to_string())),
    }
}

fn parse_pr_state(state: &str) -> Result<StateFilter> {
    match state {
        "open" => Ok(StateFilter::Open),
        "closed" => Ok(StateFilter::Closed),
        "all" => Ok(StateFilter::All),
        other => anyhow::bail!("unknown --state {other:?}; use open, closed, or all"),
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::open(cli.scope.to_scope()).await?;
    match cli.command {
        TopCommand::Issue { command } => run_issue(&store, command).await,
        TopCommand::State { command } => run_state(&store, command).await,
        TopCommand::Pr { command } => run_pr(&store, command).await,
        TopCommand::Wiki { command } => run_wiki(&store, command).await,
        TopCommand::Label { command } => run_label(&store, command).await,
    }
}

async fn run_issue(store: &Store, command: IssueCommand) -> Result<()> {
    match command {
        IssueCommand::Create { title, body, json } => {
            let number = store.create_issue(&title, &body).await?;
            if json {
                println!("{}", serde_json::json!({ "number": number }));
            } else {
                println!("#{number}");
            }
        }
        IssueCommand::List {
            state,
            label,
            unblocked,
            json,
        } => {
            let (filter, state_name) = parse_issue_state(&state);
            let issues = store
                .list_issues(filter, state_name.as_deref(), label.as_deref(), unblocked)
                .await?;
            if json {
                println!("{}", serde_json::to_string(&issues)?);
            } else if issues.is_empty() {
                println!("no issues");
            } else {
                for issue in &issues {
                    let lock = match &issue.locked_by {
                        Some(h) => format!(" [locked: {h}]"),
                        None => String::new(),
                    };
                    println!(
                        "#{:<4} {:<12} {}{}",
                        issue.number, issue.state, issue.title, lock
                    );
                }
            }
        }
        IssueCommand::Show { number, json } => {
            let detail = store.issue_detail(number).await?;
            if json {
                println!("{}", serde_json::to_string(&detail)?);
            } else {
                let issue = &detail.issue;
                println!("#{} {} ({})", issue.number, issue.title, issue.state);
                if let Some(h) = &issue.locked_by {
                    println!("locked by: {h}");
                }
                if !detail.labels.is_empty() {
                    println!("labels: {}", detail.labels.join(", "));
                }
                if !detail.blocked_by.is_empty() {
                    println!("blocked by: {}", join_numbers(&detail.blocked_by));
                }
                if !detail.blocks.is_empty() {
                    println!("blocks: {}", join_numbers(&detail.blocks));
                }
                println!();
                if issue.body.is_empty() {
                    println!("(no description)");
                } else {
                    println!("{}", issue.body);
                }
                if !detail.comments.is_empty() {
                    println!();
                    println!("--- comments ---");
                    for c in &detail.comments {
                        println!("[{}] {}", c.created_at, c.body);
                    }
                }
            }
        }
        IssueCommand::Comment { number, body } => {
            store.add_issue_comment(number, &body).await?;
            println!("commented on issue #{number}");
        }
        IssueCommand::SetState { number, state } => {
            store.set_issue_state(number, &state).await?;
            println!("issue #{number} -> {state}");
        }
        IssueCommand::Close { number } => {
            let state = store.close_issue(number).await?;
            println!("closed issue #{number} ({state})");
        }
        IssueCommand::Reopen { number } => {
            let state = store.reopen_issue(number).await?;
            println!("reopened issue #{number} ({state})");
        }
        IssueCommand::Edit {
            number,
            title,
            body,
        } => {
            store
                .edit_issue(number, title.as_deref(), body.as_deref())
                .await?;
            println!("updated issue #{number}");
        }
        IssueCommand::Dep { command } => match command {
            DepCommand::Add { blocker, blocked } => {
                store.add_dependency(blocker, blocked).await?;
                println!("#{blocker} now blocks #{blocked}");
            }
            DepCommand::Rm { blocker, blocked } => {
                store.remove_dependency(blocker, blocked).await?;
                println!("removed: #{blocker} blocks #{blocked}");
            }
        },
        IssueCommand::Lock { number, r#as } => {
            let holder = holder(r#as);
            match store.lock_issue(number, &holder).await? {
                LockOutcome::Acquired => println!("locked issue #{number} as {holder}"),
                LockOutcome::AlreadyHeld(h) => {
                    anyhow::bail!("issue #{number} is already locked by {h}");
                }
            }
        }
        IssueCommand::Unlock {
            number,
            r#as,
            force,
        } => {
            let holder = holder(r#as);
            let released = store.unlock_issue(number, &holder, force).await?;
            if released {
                println!("unlocked issue #{number}");
            } else {
                anyhow::bail!(
                    "issue #{number} was not locked by {holder} (use --force to override)"
                );
            }
        }
        IssueCommand::Label { number, label } => {
            store.label_issue(number, &label).await?;
            println!("labelled issue #{number} with {label}");
        }
        IssueCommand::Unlabel { number, label } => {
            store.unlabel_issue(number, &label).await?;
            println!("removed label {label} from issue #{number}");
        }
    }
    Ok(())
}

fn join_numbers(nums: &[i64]) -> String {
    nums.iter()
        .map(|n| format!("#{n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

async fn run_state(store: &Store, command: StateCommand) -> Result<()> {
    match command {
        StateCommand::List { json } => {
            let states = store.list_states().await?;
            if json {
                println!("{}", serde_json::to_string(&states)?);
            } else {
                for s in &states {
                    let mut flags = Vec::new();
                    if s.is_starting {
                        flags.push("starting");
                    }
                    if s.is_terminal {
                        flags.push("terminal");
                    }
                    println!("{:<14} {}", s.name, flags.join(", "));
                }
            }
        }
        StateCommand::Add {
            name,
            starting,
            terminal,
        } => {
            store.add_state(&name, starting, terminal).await?;
            println!("added state {name}");
        }
    }
    Ok(())
}

async fn run_pr(store: &Store, command: PrCommand) -> Result<()> {
    match command {
        PrCommand::Create {
            title,
            branch,
            body,
            json,
        } => {
            let number = store.create_pr(&title, &body, &branch).await?;
            if json {
                println!("{}", serde_json::json!({ "number": number }));
            } else {
                println!("#{number}");
            }
        }
        PrCommand::List { state, json } => {
            let filter = parse_pr_state(&state)?;
            let prs = store.list_prs(filter).await?;
            if json {
                println!("{}", serde_json::to_string(&prs)?);
            } else if prs.is_empty() {
                println!("no pull requests");
            } else {
                for pr in &prs {
                    println!(
                        "#{:<4} {:<8} {} ({})",
                        pr.number, pr.state, pr.title, pr.branch
                    );
                }
            }
        }
        PrCommand::Show { number, json } => {
            let detail = store.pr_detail(number).await?;
            if json {
                println!("{}", serde_json::to_string(&detail)?);
            } else {
                let pr = &detail.pr;
                println!("#{} {} ({})", pr.number, pr.title, pr.state);
                println!("branch: {}", pr.branch);
                println!();
                if pr.body.is_empty() {
                    println!("(no description)");
                } else {
                    println!("{}", pr.body);
                }
                if !detail.comments.is_empty() {
                    println!();
                    println!("--- comments ---");
                    for c in &detail.comments {
                        println!("[{}] {}", c.created_at, c.body);
                    }
                }
            }
        }
        PrCommand::Comment { number, body } => {
            store.add_pr_comment(number, &body).await?;
            println!("commented on PR #{number}");
        }
        PrCommand::SetState { number, state } => {
            store.set_pr_state(number, &state).await?;
            println!("PR #{number} -> {state}");
        }
        PrCommand::Close { number } => {
            store.set_pr_state(number, "closed").await?;
            println!("closed PR #{number}");
        }
        PrCommand::Reopen { number } => {
            store.set_pr_state(number, "open").await?;
            println!("reopened PR #{number}");
        }
        PrCommand::Edit {
            number,
            title,
            body,
        } => {
            store
                .edit_pr(number, title.as_deref(), body.as_deref())
                .await?;
            println!("updated PR #{number}");
        }
    }
    Ok(())
}

async fn run_wiki(store: &Store, command: WikiCommand) -> Result<()> {
    match command {
        WikiCommand::Create { title, slug, body } => {
            let slug = slug.unwrap_or_else(|| title.clone());
            let slug = store.create_wiki(&slug, &title, &body).await?;
            println!("created wiki page {slug}");
        }
        WikiCommand::Edit { slug, title, body } => {
            store
                .edit_wiki(&slug, title.as_deref(), body.as_deref())
                .await?;
            println!("updated wiki page {slug}");
        }
        WikiCommand::Show { slug, json } => {
            let detail = store.wiki_detail(&slug).await?;
            if json {
                println!("{}", serde_json::to_string(&detail)?);
            } else {
                let page = &detail.page;
                println!("{} ({})", page.title, page.slug);
                if !detail.links_to.is_empty() {
                    println!("links to: {}", detail.links_to.join(", "));
                }
                if !detail.backlinks.is_empty() {
                    println!("backlinks: {}", detail.backlinks.join(", "));
                }
                println!();
                if page.body.is_empty() {
                    println!("(empty page)");
                } else {
                    println!("{}", page.body);
                }
            }
        }
        WikiCommand::List { json } => {
            let pages = store.list_wiki().await?;
            if json {
                println!("{}", serde_json::to_string(&pages)?);
            } else if pages.is_empty() {
                println!("no wiki pages");
            } else {
                for p in &pages {
                    println!("{:<24} {}", p.slug, p.title);
                }
            }
        }
    }
    Ok(())
}

async fn run_label(store: &Store, command: LabelCommand) -> Result<()> {
    match command {
        LabelCommand::Group { name, selection } => {
            store.create_label_group(&name, &selection).await?;
            println!("created {selection} label group {name}");
        }
        LabelCommand::Create { name, group } => {
            store.create_label(&name, group.as_deref()).await?;
            println!("created label {name}");
        }
        LabelCommand::List { json } => {
            let labels = store.list_labels().await?;
            if json {
                println!("{}", serde_json::to_string(&labels)?);
            } else if labels.is_empty() {
                println!("no labels");
            } else {
                for l in &labels {
                    match &l.group {
                        Some(g) => println!("{:<20} ({g})", l.name),
                        None => println!("{}", l.name),
                    }
                }
            }
        }
        LabelCommand::Groups { json } => {
            let groups = store.list_label_groups().await?;
            if json {
                println!("{}", serde_json::to_string(&groups)?);
            } else if groups.is_empty() {
                println!("no label groups");
            } else {
                for g in &groups {
                    println!("{:<20} {}", g.name, g.selection);
                }
            }
        }
    }
    Ok(())
}
