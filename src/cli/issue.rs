use super::IssueCommand;
use crate::store::{LeaseOutcome, Store};
use anyhow::Result;

pub(crate) async fn run(store: &Store, command: IssueCommand) -> Result<()> {
    match command {
        IssueCommand::Tui => {
            // Details are repository-scoped (issue numbers are only unique
            // within a repository), so the TUI deliberately rejects
            // `--all-repos` before entering raw terminal mode.
            store.repo_id()?;
            // The TUI is a general-purpose issue browser, so it shows review,
            // terminal, canceled, and legacy states.
            let details = store.list_all_issue_details().await?;
            crate::tui::run(details)?;
        }
        IssueCommand::Create {
            title,
            body,
            state,
            priority,
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
                    priority,
                    project.as_deref(),
                    milestone.as_deref(),
                    parent,
                )
                .await?;
            if json {
                println!("{}", serde_json::json!({ "number": number }));
            } else {
                println!("#{number}");
            }
        }
        IssueCommand::List {
            state_filter,
            label,
            status_type,
            priority,
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
                    status_type.as_deref(),
                    priority,
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
                println!("no issues");
            } else {
                for issue in &issues {
                    let lease = if issue.leased { " [leased]" } else { "" };
                    println!(
                        "#{:<4} {:<12} {:<10} P{} {}{}",
                        issue.number,
                        issue.state,
                        issue.status_type,
                        issue.priority,
                        issue.title,
                        lease
                    );
                    if let Some(project) = &issue.project {
                        println!("      project: {}", project.name);
                    }
                    if let Some(milestone) = &issue.milestone {
                        println!("      milestone: {}", milestone.name);
                    }
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
                println!("status type: {}", issue.status_type);
                println!("priority: {}", issue.priority);
                println!(
                    "project: {}",
                    issue
                        .project
                        .as_ref()
                        .map(|project| project.name.as_str())
                        .unwrap_or("No Project")
                );
                println!(
                    "milestone: {}",
                    issue
                        .milestone
                        .as_ref()
                        .map(|milestone| milestone.name.as_str())
                        .unwrap_or("No Milestone")
                );
                if let Some(parent) = &detail.parent {
                    println!("parent: #{} {}", parent.number, parent.title);
                }
                if !detail.sub_issues.is_empty() {
                    println!(
                        "sub-issues: {}",
                        detail
                            .sub_issues
                            .iter()
                            .map(|issue| format!("#{} {}", issue.number, issue.title))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                if issue.leased {
                    println!("leased: yes");
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
                if !detail.related.is_empty() {
                    println!("related: {}", join_numbers(&detail.related));
                }
                for pr in &detail.pull_requests {
                    println!(
                        "pull request: #{} {} (branch: {}, state: {})",
                        pr.number, pr.title, pr.branch, pr.state
                    );
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
        IssueCommand::SetState {
            number,
            state,
            lease,
        } => {
            store
                .set_issue_state(number, &state, lease.as_deref())
                .await?;
            println!("issue #{number} -> {state}");
        }
        IssueCommand::Set {
            number,
            title,
            body,
            priority,
            project,
            milestone,
            parent,
            lease,
        } => {
            if title.is_none()
                && body.is_none()
                && priority.is_none()
                && project.is_none()
                && milestone.is_none()
                && parent.is_none()
            {
                anyhow::bail!("specify at least one property to set");
            }
            if title.is_some() || body.is_some() || priority.is_some() {
                store
                    .edit_issue(
                        number,
                        title.as_deref(),
                        body.as_deref(),
                        priority,
                        lease.as_deref(),
                    )
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
            println!("updated issue #{number}");
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
            println!("updated issue #{number}");
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
            println!("updated issue #{number}");
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
            println!("updated issue #{number}");
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
                println!("unlocked issue #{number}");
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
