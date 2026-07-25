use super::{
    holder, parse_issue_state, DepCommand, IssueCommand, IssueMilestoneCommand, IssueParentCommand,
    IssueProjectCommand, RelateCommand,
};
use crate::store::{LockOutcome, Store};
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
            state,
            label,
            status_type,
            priority,
            project,
            milestone,
            related_to,
            unblocked,
            json,
        } => {
            let (filter, state_name) = parse_issue_state(&state);
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
                    let lock = issue
                        .locked_by
                        .as_ref()
                        .map(|holder| format!(" [locked: {holder}]"))
                        .unwrap_or_default();
                    println!(
                        "#{:<4} {:<12} {:<10} P{} {}{}",
                        issue.number,
                        issue.state,
                        issue.status_type,
                        issue.priority,
                        issue.title,
                        lock
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
                println!(
                    "type: {}",
                    detail.type_label.as_deref().unwrap_or("untyped")
                );
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
                if !detail.related.is_empty() {
                    println!("related: {}", join_numbers(&detail.related));
                }
                if let Some(pr) = &detail.pull_request {
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
        IssueCommand::SetState { number, state } => {
            store.set_issue_state(number, &state).await?;
            println!("issue #{number} -> {state}");
        }
        IssueCommand::Transition {
            number,
            state,
            completion_note,
        } => {
            store
                .transition_issue(number, &state, completion_note.as_deref())
                .await?;
            println!("transitioned issue #{number} -> {state}");
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
            priority,
        } => {
            store
                .edit_issue(number, title.as_deref(), body.as_deref(), priority)
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
        IssueCommand::Relate { command } => match command {
            RelateCommand::Add { first, second } => {
                store.add_issue_relation(first, second).await?;
                println!("related issues #{first} and #{second}");
            }
            RelateCommand::Rm { first, second } => {
                store.remove_issue_relation(first, second).await?;
                println!("removed relation between #{first} and #{second}");
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
        IssueCommand::Project { command } => match command {
            IssueProjectCommand::Set { number, project } => {
                store.set_issue_project(number, &project).await?;
                println!("issue #{number} -> project {project}");
            }
            IssueProjectCommand::Clear { number } => {
                store.clear_issue_project(number).await?;
                println!("cleared project from issue #{number}");
            }
        },
        IssueCommand::Milestone { command } => match command {
            IssueMilestoneCommand::Set { number, milestone } => {
                store.set_issue_milestone(number, &milestone).await?;
                println!("issue #{number} -> milestone {milestone}");
            }
            IssueMilestoneCommand::Clear { number } => {
                store.clear_issue_milestone(number).await?;
                println!("cleared milestone from issue #{number}");
            }
        },
        IssueCommand::Parent { command } => match command {
            IssueParentCommand::Set { number, parent } => {
                store.set_issue_parent(number, parent).await?;
                println!("issue #{number} -> parent #{parent}");
            }
            IssueParentCommand::Clear { number } => {
                store.clear_issue_parent(number).await?;
                println!("cleared parent from issue #{number}");
            }
        },
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
