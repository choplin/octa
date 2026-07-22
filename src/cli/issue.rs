use super::{holder, parse_issue_state, DepCommand, IssueCommand};
use crate::store::{LockOutcome, Store};
use anyhow::Result;

pub(crate) async fn run(store: &Store, command: IssueCommand) -> Result<()> {
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
                    let lock = issue
                        .locked_by
                        .as_ref()
                        .map(|holder| format!(" [locked: {holder}]"))
                        .unwrap_or_default();
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
                if let Some(holder) = &issue.locked_by {
                    println!("locked by: {holder}");
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
                    for comment in &detail.comments {
                        println!("[{}] {}", comment.created_at, comment.body);
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
                LockOutcome::AlreadyHeld(holder) => {
                    anyhow::bail!("issue #{number} is already locked by {holder}")
                }
            }
        }
        IssueCommand::Unlock {
            number,
            r#as,
            force,
        } => {
            let holder = holder(r#as);
            if store.unlock_issue(number, &holder, force).await? {
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

fn join_numbers(numbers: &[i64]) -> String {
    numbers
        .iter()
        .map(|number| format!("#{number}"))
        .collect::<Vec<_>>()
        .join(", ")
}
