use super::output::{Output, Tone};
use super::{parse_pull_request_state, PullRequestCommand};
use crate::store::Store;
use anyhow::Result;
pub(crate) async fn run(store: &Store, command: PullRequestCommand) -> Result<()> {
    let output = Output::stdout();
    match command {
        PullRequestCommand::Create {
            title,
            branch,
            body,
            issue,
            lease,
            json,
        } => {
            let number = store
                .create_pull_request(&title, &body, &branch, issue, lease.as_deref())
                .await?;
            if json {
                println!("{}", serde_json::json!({"number":number}))
            } else {
                output.print(output.line(Tone::Success, format!("#{number}")))
            }
        }
        PullRequestCommand::List { state, json } => {
            let pull_requests = store
                .list_pull_requests(parse_pull_request_state(&state)?)
                .await?;
            if json {
                println!("{}", serde_json::to_string(&pull_requests)?)
            } else if pull_requests.is_empty() {
                output.print(output.line(Tone::Warning, "no pull requests"))
            } else {
                let rows = pull_requests.iter().map(|pull_request| {
                    [
                        format!("#{}", pull_request.number),
                        pull_request.state.clone(),
                        pull_request.title.clone(),
                        pull_request.branch.clone(),
                    ]
                });
                output.print(output.table(["pull request", "State", "Title", "Branch"], rows))
            }
        }
        PullRequestCommand::Show { number, json } => {
            let detail = store.pull_request_detail(number).await?;
            if json {
                println!("{}", serde_json::to_string(&detail)?)
            } else {
                let pull_request = &detail.pull_request;
                let mut lines = vec![
                    output.row(
                        format!("#{}", pull_request.number),
                        format!(" {} ({})", pull_request.title, pull_request.state),
                    ),
                    output.field("branch: ", &pull_request.branch),
                    output.line(Tone::Body, ""),
                    output.line(
                        if pull_request.body.is_empty() {
                            Tone::Warning
                        } else {
                            Tone::Body
                        },
                        if pull_request.body.is_empty() {
                            "(no description)"
                        } else {
                            &pull_request.body
                        },
                    ),
                ];
                if !detail.comments.is_empty() {
                    lines.push(output.line(Tone::Body, ""));
                    lines.push(output.line(Tone::Accent, "--- comments ---"));
                    for comment in detail.comments {
                        lines
                            .push(output.field(format!("[{}] ", comment.created_at), comment.body));
                    }
                }
                output.print_lines(lines)
            }
        }
        PullRequestCommand::Comment { number, body } => {
            store.add_pull_request_comment(number, &body).await?;
            output.print(output.line(
                Tone::Success,
                format!("commented on pull request #{number}"),
            ))
        }
        PullRequestCommand::SetState { number, state } => {
            store.set_pull_request_state(number, &state).await?;
            output.print(output.line(Tone::Success, format!("pull request #{number} -> {state}")))
        }
        PullRequestCommand::Set {
            number,
            title,
            body,
        } => {
            store
                .edit_pull_request(number, title.as_deref(), body.as_deref())
                .await?;
            output.print(output.line(Tone::Success, format!("updated pull request #{number}")))
        }
        PullRequestCommand::Add {
            number,
            issue,
            lease,
        } => {
            store
                .link_pull_request(issue, number, lease.as_deref())
                .await?;
            output.print(output.line(Tone::Success, format!("updated pull request #{number}")))
        }
        PullRequestCommand::Remove {
            number,
            issue,
            lease,
        } => {
            store
                .unlink_pull_request(issue, number, lease.as_deref())
                .await?;
            output.print(output.line(Tone::Success, format!("updated pull request #{number}")))
        }
    }
    Ok(())
}
