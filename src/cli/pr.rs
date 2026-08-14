use super::output::{Output, Tone};
use super::{parse_pr_state, PrCommand};
use crate::store::Store;
use anyhow::Result;
use urushi::View;
pub(crate) async fn run(store: &Store, command: PrCommand) -> Result<()> {
    let output = Output::stdout();
    match command {
        PrCommand::Create {
            title,
            branch,
            body,
            issue,
            lease,
            json,
        } => {
            let number = store
                .create_pr(&title, &body, &branch, issue, lease.as_deref())
                .await?;
            if json {
                println!("{}", serde_json::json!({"number":number}))
            } else {
                output.print(View::line(output.line(Tone::Success, format!("#{number}"))))
            }
        }
        PrCommand::List { state, json } => {
            let prs = store.list_prs(parse_pr_state(&state)?).await?;
            if json {
                println!("{}", serde_json::to_string(&prs)?)
            } else if prs.is_empty() {
                output.print(View::line(output.line(Tone::Warning, "no pull requests")))
            } else {
                let rows = prs.iter().map(|pr| {
                    [
                        format!("#{}", pr.number),
                        pr.state.clone(),
                        pr.title.clone(),
                        pr.branch.clone(),
                    ]
                });
                output.print(output.table(["PR", "State", "Title", "Branch"], rows))
            }
        }
        PrCommand::Show { number, json } => {
            let detail = store.pr_detail(number).await?;
            if json {
                println!("{}", serde_json::to_string(&detail)?)
            } else {
                let pr = &detail.pr;
                let mut view = View::line(output.row(
                    format!("#{}", pr.number),
                    format!(" {} ({})", pr.title, pr.state),
                ))
                .push(output.field("branch: ", &pr.branch))
                .push(output.line(Tone::Body, ""))
                .push(output.line(
                    if pr.body.is_empty() {
                        Tone::Warning
                    } else {
                        Tone::Body
                    },
                    if pr.body.is_empty() {
                        "(no description)"
                    } else {
                        &pr.body
                    },
                ));
                if !detail.comments.is_empty() {
                    view = view
                        .push(output.line(Tone::Body, ""))
                        .push(output.line(Tone::Accent, "--- comments ---"));
                    for comment in detail.comments {
                        view = view
                            .push(output.field(format!("[{}] ", comment.created_at), comment.body));
                    }
                }
                output.print(view)
            }
        }
        PrCommand::Comment { number, body } => {
            store.add_pr_comment(number, &body).await?;
            output.print(View::line(
                output.line(Tone::Success, format!("commented on PR #{number}")),
            ))
        }
        PrCommand::SetState { number, state } => {
            store.set_pr_state(number, &state).await?;
            output.print(View::line(
                output.line(Tone::Success, format!("PR #{number} -> {state}")),
            ))
        }
        PrCommand::Set {
            number,
            title,
            body,
        } => {
            store
                .edit_pr(number, title.as_deref(), body.as_deref())
                .await?;
            output.print(View::line(
                output.line(Tone::Success, format!("updated PR #{number}")),
            ))
        }
        PrCommand::Add {
            number,
            issue,
            lease,
        } => {
            store.link_pr(issue, number, lease.as_deref()).await?;
            output.print(View::line(
                output.line(Tone::Success, format!("updated PR #{number}")),
            ))
        }
        PrCommand::Remove {
            number,
            issue,
            lease,
        } => {
            store.unlink_pr(issue, number, lease.as_deref()).await?;
            output.print(View::line(
                output.line(Tone::Success, format!("updated PR #{number}")),
            ))
        }
    }
    Ok(())
}
