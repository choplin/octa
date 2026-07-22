use super::{parse_pr_state, PrCommand};
use crate::store::Store;
use anyhow::Result;
pub(crate) async fn run(store: &Store, command: PrCommand) -> Result<()> {
    match command {
        PrCommand::Create {
            title,
            branch,
            body,
            json,
        } => {
            let number = store.create_pr(&title, &body, &branch).await?;
            if json {
                println!("{}", serde_json::json!({"number":number}))
            } else {
                println!("#{number}")
            }
        }
        PrCommand::List { state, json } => {
            let prs = store.list_prs(parse_pr_state(&state)?).await?;
            if json {
                println!("{}", serde_json::to_string(&prs)?)
            } else if prs.is_empty() {
                println!("no pull requests")
            } else {
                for pr in prs {
                    println!(
                        "#{:<4} {:<8} {} ({})",
                        pr.number, pr.state, pr.title, pr.branch
                    )
                }
            }
        }
        PrCommand::Show { number, json } => {
            let detail = store.pr_detail(number).await?;
            if json {
                println!("{}", serde_json::to_string(&detail)?)
            } else {
                let pr = &detail.pr;
                println!("#{} {} ({})", pr.number, pr.title, pr.state);
                println!("branch: {}", pr.branch);
                println!();
                println!(
                    "{}",
                    if pr.body.is_empty() {
                        "(no description)"
                    } else {
                        &pr.body
                    }
                );
                if !detail.comments.is_empty() {
                    println!();
                    println!("--- comments ---");
                    for comment in detail.comments {
                        println!("[{}] {}", comment.created_at, comment.body)
                    }
                }
            }
        }
        PrCommand::Comment { number, body } => {
            store.add_pr_comment(number, &body).await?;
            println!("commented on PR #{number}")
        }
        PrCommand::SetState { number, state } => {
            store.set_pr_state(number, &state).await?;
            println!("PR #{number} -> {state}")
        }
        PrCommand::Close { number } => {
            store.set_pr_state(number, "closed").await?;
            println!("closed PR #{number}")
        }
        PrCommand::Reopen { number } => {
            store.set_pr_state(number, "open").await?;
            println!("reopened PR #{number}")
        }
        PrCommand::Edit {
            number,
            title,
            body,
        } => {
            store
                .edit_pr(number, title.as_deref(), body.as_deref())
                .await?;
            println!("updated PR #{number}")
        }
    }
    Ok(())
}
