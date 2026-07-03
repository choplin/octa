//! octa: GitHub-style Issue collaboration, fully local.

mod store;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use store::{StateFilter, Store};

#[derive(Parser)]
#[command(
    name = "octa",
    version,
    about = "GitHub-style Issue collaboration, fully local"
)]
struct Cli {
    #[command(subcommand)]
    command: TopCommand,
}

#[derive(Subcommand)]
enum TopCommand {
    /// Manage issues
    Issue {
        #[command(subcommand)]
        command: IssueCommand,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum StateArg {
    Open,
    Closed,
    All,
}

impl From<StateArg> for StateFilter {
    fn from(value: StateArg) -> Self {
        match value {
            StateArg::Open => StateFilter::Open,
            StateArg::Closed => StateFilter::Closed,
            StateArg::All => StateFilter::All,
        }
    }
}

#[derive(Subcommand)]
enum IssueCommand {
    /// Create a new issue
    Create {
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        body: String,
        /// Emit the created issue number as JSON
        #[arg(long)]
        json: bool,
    },
    /// List issues
    List {
        #[arg(long, value_enum, default_value_t = StateArg::Open)]
        state: StateArg,
        /// Emit the issues as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show an issue and its comment thread
    Show {
        number: i64,
        /// Emit the issue and comments as JSON
        #[arg(long)]
        json: bool,
    },
    /// Add a comment to an issue
    Comment {
        number: i64,
        #[arg(long)]
        body: String,
    },
    /// Close an issue
    Close { number: i64 },
    /// Reopen a closed issue
    Reopen { number: i64 },
    /// Edit an issue's title and/or body
    Edit {
        number: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::open().await?;
    match cli.command {
        TopCommand::Issue { command } => run_issue(&store, command).await,
    }
}

async fn run_issue(store: &Store, command: IssueCommand) -> Result<()> {
    match command {
        IssueCommand::Create { title, body, json } => {
            let number = store.create(&title, &body).await?;
            if json {
                println!("{}", serde_json::json!({ "number": number }));
            } else {
                println!("#{number}");
            }
        }
        IssueCommand::List { state, json } => {
            let issues = store.list(state.into()).await?;
            if json {
                println!("{}", serde_json::to_string(&issues)?);
            } else if issues.is_empty() {
                println!("no issues");
            } else {
                for issue in &issues {
                    println!(
                        "#{:<4} {:<6} {}",
                        issue.number,
                        issue.state.to_uppercase(),
                        issue.title
                    );
                }
            }
        }
        IssueCommand::Show { number, json } => {
            let detail = store.detail(number).await?;
            if json {
                println!("{}", serde_json::to_string(&detail)?);
            } else {
                let issue = &detail.issue;
                println!(
                    "#{} {} ({})",
                    issue.number,
                    issue.title,
                    issue.state.to_uppercase()
                );
                println!();
                if issue.body.is_empty() {
                    println!("(no description)");
                } else {
                    println!("{}", issue.body);
                }
                if !detail.comments.is_empty() {
                    println!();
                    println!("--- comments ---");
                    for comment in &detail.comments {
                        println!("[{}] {}", comment.created_at, comment.body);
                    }
                }
            }
        }
        IssueCommand::Comment { number, body } => {
            store.add_comment(number, &body).await?;
            println!("commented on issue #{number}");
        }
        IssueCommand::Close { number } => {
            store.set_state(number, "closed").await?;
            println!("closed issue #{number}");
        }
        IssueCommand::Reopen { number } => {
            store.set_state(number, "open").await?;
            println!("reopened issue #{number}");
        }
        IssueCommand::Edit {
            number,
            title,
            body,
        } => {
            store
                .edit(number, title.as_deref(), body.as_deref())
                .await?;
            println!("updated issue #{number}");
        }
    }
    Ok(())
}
