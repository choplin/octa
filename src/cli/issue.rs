use super::output::{Output, Tone};
use super::IssueCommand;
use crate::store::{LeaseOutcome, Store};
use anyhow::Result;

pub(crate) async fn run(store: &Store, command: IssueCommand) -> Result<()> {
    let output = Output::stdout();
    match command {
        IssueCommand::Tui => {
            // Details are repository-scoped (issue numbers are only unique
            // within a repository), so the TUI deliberately rejects
            // `--all-repos` before entering raw terminal mode.
            store.repo_id()?;
            // The TUI is a general-purpose issue browser, so it shows review,
            // closed, canceled, and legacy states.
            let details = store.list_all_issue_details().await?;
            crate::tui::run(details)?;
        }
        IssueCommand::Create {
            title,
            body,
            state,
            project,
            milestone,
            parent,
            json,
        } => {
            let number = store
                .create_issue(
                    &title,
                    &body,
                    state.as_deref(),
                    project.as_deref(),
                    milestone.as_deref(),
                    parent,
                )
                .await?;
            if json {
                println!("{}", serde_json::json!({ "number": number }));
            } else {
                output.print(output.line(Tone::Success, format!("#{number}")));
            }
        }
        IssueCommand::List {
            state_filter,
            label,
            project,
            milestone,
            related_to,
            unblocked,
            json,
        } => {
            let (filter, state_name) = state_filter.into_filter();
            let issues = store
                .list_issues(
                    filter,
                    state_name.as_deref(),
                    label.as_deref(),
                    project.as_deref(),
                    milestone.as_deref(),
                    related_to,
                    unblocked,
                )
                .await?;
            if json {
                println!("{}", serde_json::to_string(&issues)?);
            } else if issues.is_empty() {
                output.print(output.line(Tone::Warning, "no issues"));
            } else {
                let rows = issues.iter().map(|issue| {
                    [
                        format!("#{}", issue.number),
                        issue.state.clone(),
                        issue.title.clone(),
                        issue
                            .project
                            .as_ref()
                            .map(|project| project.name.clone())
                            .unwrap_or_default(),
                        issue
                            .milestone
                            .as_ref()
                            .map(|milestone| milestone.name.clone())
                            .unwrap_or_default(),
                        if issue.leased { "yes" } else { "" }.to_owned(),
                    ]
                });
                output.print(output.table(
                    ["Issue", "State", "Title", "Project", "Milestone", "Leased"],
                    rows,
                ));
            }
        }
        IssueCommand::Show { number, json } => {
            let detail = store.issue_detail(number).await?;
            if json {
                println!("{}", serde_json::to_string(&detail)?);
            } else {
                let issue = &detail.issue;
                let mut lines = vec![
                    output.row(
                        format!("#{}", issue.number),
                        format!(" {} ({})", issue.title, issue.state),
                    ),
                    output.field(
                        "project: ",
                        issue
                            .project
                            .as_ref()
                            .map(|project| project.name.as_str())
                            .unwrap_or("No Project"),
                    ),
                    output.field(
                        "milestone: ",
                        issue
                            .milestone
                            .as_ref()
                            .map(|milestone| milestone.name.as_str())
                            .unwrap_or("No Milestone"),
                    ),
                ];
                if let Some(parent) = &detail.parent {
                    lines.push(
                        output.field("parent: ", format!("#{} {}", parent.number, parent.title)),
                    );
                }
                if !detail.sub_issues.is_empty() {
                    lines.push(
                        output.field(
                            "sub-issues: ",
                            detail
                                .sub_issues
                                .iter()
                                .map(|issue| format!("#{} {}", issue.number, issue.title))
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                    );
                }
                if issue.leased {
                    lines.push(output.field("leased: ", "yes"));
                }
                if !detail.labels.is_empty() {
                    lines.push(output.field("labels: ", detail.labels.join(", ")));
                }
                if !detail.blocked_by.is_empty() {
                    lines.push(output.field("blocked by: ", join_numbers(&detail.blocked_by)));
                }
                if !detail.blocks.is_empty() {
                    lines.push(output.field("blocks: ", join_numbers(&detail.blocks)));
                }
                if !detail.related.is_empty() {
                    lines.push(output.field("related: ", join_numbers(&detail.related)));
                }
                for pr in &detail.pull_requests {
                    lines.push(output.field(
                        "pull request: ",
                        format!(
                            "#{} {} (branch: {}, state: {})",
                            pr.number, pr.title, pr.branch, pr.state
                        ),
                    ));
                }
                lines.push(output.line(Tone::Body, ""));
                if issue.body.is_empty() {
                    lines.push(output.line(Tone::Warning, "(no description)"));
                } else {
                    lines.push(output.line(Tone::Body, &issue.body));
                }
                if !detail.comments.is_empty() {
                    lines.push(output.line(Tone::Body, ""));
                    lines.push(output.line(Tone::Accent, "--- comments ---"));
                    for comment in &detail.comments {
                        lines.push(
                            output.field(format!("[{}] ", comment.created_at), &comment.body),
                        );
                    }
                }
                output.print_lines(lines);
            }
        }
        IssueCommand::Comment { number, body } => {
            store.add_issue_comment(number, &body).await?;
            output.print(output.line(Tone::Success, format!("commented on issue #{number}")));
        }
        IssueCommand::SetState {
            number,
            state,
            lease,
        } => {
            store
                .set_issue_state(number, &state, lease.as_deref())
                .await?;
            output.print(output.line(Tone::Success, format!("issue #{number} -> {state}")));
        }
        IssueCommand::Set {
            number,
            title,
            body,
            project,
            milestone,
            parent,
            lease,
        } => {
            if title.is_none()
                && body.is_none()
                && project.is_none()
                && milestone.is_none()
                && parent.is_none()
            {
                anyhow::bail!("specify at least one property to set");
            }
            if title.is_some() || body.is_some() {
                store
                    .edit_issue(number, title.as_deref(), body.as_deref(), lease.as_deref())
                    .await?;
            }
            if let Some(project) = project {
                store
                    .set_issue_project(number, &project, lease.as_deref())
                    .await?;
            }
            if let Some(milestone) = milestone {
                store
                    .set_issue_milestone(number, &milestone, lease.as_deref())
                    .await?;
            }
            if let Some(parent) = parent {
                store
                    .set_issue_parent(number, parent, lease.as_deref())
                    .await?;
            }
            output.print(output.line(Tone::Success, format!("updated issue #{number}")));
        }
        IssueCommand::Unset {
            number,
            project,
            milestone,
            parent,
            lease,
        } => {
            if !project && !milestone && !parent {
                anyhow::bail!("specify at least one property to unset");
            }
            if milestone {
                store
                    .clear_issue_milestone(number, lease.as_deref())
                    .await?;
            }
            if project {
                store.clear_issue_project(number, lease.as_deref()).await?;
            }
            if parent {
                store.clear_issue_parent(number, lease.as_deref()).await?;
            }
            output.print(output.line(Tone::Success, format!("updated issue #{number}")));
        }
        IssueCommand::Add {
            number,
            label,
            blocker,
            blocks,
            related,
            pr,
            lease,
        } => {
            if label.is_none()
                && blocker.is_none()
                && blocks.is_none()
                && related.is_none()
                && pr.is_none()
            {
                anyhow::bail!("specify at least one relationship to add");
            }
            if let Some(label) = label {
                store.label_issue(number, &label, lease.as_deref()).await?;
            }
            if let Some(blocker) = blocker {
                store
                    .add_dependency(number, blocker, number, lease.as_deref())
                    .await?;
            }
            if let Some(blocked) = blocks {
                store
                    .add_dependency(number, number, blocked, lease.as_deref())
                    .await?;
            }
            if let Some(other) = related {
                store
                    .add_issue_relation(number, other, lease.as_deref())
                    .await?;
            }
            if let Some(pr) = pr {
                store.link_pr(number, pr, lease.as_deref()).await?;
            }
            output.print(output.line(Tone::Success, format!("updated issue #{number}")));
        }
        IssueCommand::Remove {
            number,
            label,
            blocker,
            blocks,
            related,
            pr,
            lease,
        } => {
            if label.is_none()
                && blocker.is_none()
                && blocks.is_none()
                && related.is_none()
                && pr.is_none()
            {
                anyhow::bail!("specify at least one relationship to remove");
            }
            if let Some(label) = label {
                store
                    .unlabel_issue(number, &label, lease.as_deref())
                    .await?;
            }
            if let Some(blocker) = blocker {
                store
                    .remove_dependency(number, blocker, number, lease.as_deref())
                    .await?;
            }
            if let Some(blocked) = blocks {
                store
                    .remove_dependency(number, number, blocked, lease.as_deref())
                    .await?;
            }
            if let Some(other) = related {
                store
                    .remove_issue_relation(number, other, lease.as_deref())
                    .await?;
            }
            if let Some(pr) = pr {
                store.unlink_pr(number, pr, lease.as_deref()).await?;
            }
            output.print(output.line(Tone::Success, format!("updated issue #{number}")));
        }
        IssueCommand::Lock { number } => match store.lock_issue(number).await? {
            LeaseOutcome::Acquired(lease) => println!("{lease}"),
            LeaseOutcome::AlreadyLeased => {
                anyhow::bail!("issue #{number} is already leased")
            }
        },
        IssueCommand::Unlock {
            number,
            lease,
            force,
        } => {
            if store.unlock_issue(number, lease.as_deref(), force).await? {
                output.print(output.line(Tone::Success, format!("unlocked issue #{number}")));
            } else {
                anyhow::bail!("valid lease required for issue #{number} (use --force to override)");
            }
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
