use super::{ProjectCommand, ProjectMilestoneCommand};
use crate::store::Store;
use anyhow::Result;

pub(crate) async fn run(store: &Store, command: ProjectCommand) -> Result<()> {
    match command {
        ProjectCommand::Milestone { command } => match command {
            ProjectMilestoneCommand::Create {
                project,
                name,
                description,
                status,
                position,
                start_date,
                target_date,
                json,
            } => {
                let id = store
                    .create_project_milestone(
                        &project,
                        &name,
                        &description,
                        &status,
                        position,
                        start_date.as_deref(),
                        target_date.as_deref(),
                    )
                    .await?;
                if json {
                    println!(
                        "{}",
                        serde_json::json!({ "project": project, "id": id, "name": name })
                    );
                } else {
                    println!("milestone {id}: {name}");
                }
            }
            ProjectMilestoneCommand::List { project, json } => {
                let milestones = store.list_project_milestones(&project).await?;
                if json {
                    println!("{}", serde_json::to_string(&milestones)?);
                } else if milestones.is_empty() {
                    println!("no milestones");
                } else {
                    for milestone in milestones {
                        println!(
                            "{:<4} {:<4} {:<12} {}",
                            milestone.id, milestone.position, milestone.status, milestone.name
                        );
                    }
                }
            }
            ProjectMilestoneCommand::Show {
                project,
                milestone,
                json,
            } => {
                let milestone = store.project_milestone(&project, &milestone).await?;
                if json {
                    println!("{}", serde_json::to_string(&milestone)?);
                } else {
                    println!(
                        "{}: {} (position {}, {})",
                        milestone.id, milestone.name, milestone.position, milestone.status
                    );
                    println!(
                        "dates: {} -> {}",
                        milestone.start_date.as_deref().unwrap_or("(none)"),
                        milestone.target_date.as_deref().unwrap_or("(none)")
                    );
                    println!();
                    if milestone.description.is_empty() {
                        println!("(no description)");
                    } else {
                        println!("{}", milestone.description);
                    }
                }
            }
            ProjectMilestoneCommand::Edit {
                project,
                milestone,
                name,
                description,
                status,
                position,
                start_date,
                target_date,
                clear_start_date,
                clear_target_date,
                json,
            } => {
                store
                    .edit_project_milestone(
                        &project,
                        &milestone,
                        name.as_deref(),
                        description.as_deref(),
                        status.as_deref(),
                        position,
                        start_date.as_deref(),
                        target_date.as_deref(),
                        clear_start_date,
                        clear_target_date,
                    )
                    .await?;
                if json {
                    println!("{}", serde_json::json!({ "updated": true }));
                } else {
                    println!("updated milestone {milestone}");
                }
            }
        },
        ProjectCommand::Create {
            name,
            summary,
            description,
            state,
            status_type,
            priority,
            json,
        } => {
            let id = store
                .create_project(
                    &name,
                    &summary,
                    &description,
                    &state,
                    &status_type,
                    priority,
                )
                .await?;
            if json {
                println!("{}", serde_json::json!({ "id": id, "name": name }));
            } else {
                println!("project {id}: {name}");
            }
        }
        ProjectCommand::List { active, json } => {
            let projects = store.list_projects(active).await?;
            if json {
                println!("{}", serde_json::to_string(&projects)?);
            } else if projects.is_empty() {
                println!("no projects");
            } else {
                for project in projects {
                    println!(
                        "{:<4} {:<12} {:<10} P{} issues B/U/S/D/C {}/{}/{}/{}/{} {}",
                        project.project.id,
                        project.project.state,
                        project.project.status_type,
                        project.project.priority,
                        project.tally.backlog,
                        project.tally.unstarted,
                        project.tally.started,
                        project.tally.completed,
                        project.tally.canceled,
                        project.project.name
                    );
                    for milestone in &project.milestones {
                        println!(
                            "      milestone: {:<4} {:<12} {}",
                            milestone.position, milestone.status, milestone.name
                        );
                    }
                }
            }
        }
        ProjectCommand::Show { project, json } => {
            let detail = store.project_detail(&project).await?;
            if json {
                println!("{}", serde_json::to_string(&detail)?);
            } else {
                println!(
                    "{}: {} ({})",
                    detail.project.id, detail.project.name, detail.project.state
                );
                println!("status type: {}", detail.project.status_type);
                println!("priority: {}", detail.project.priority);
                println!("summary: {}", detail.project.summary);
                println!(
                    "issues: {} (backlog {}, unstarted {}, started {}, completed {}, canceled {})",
                    detail.tally.total,
                    detail.tally.backlog,
                    detail.tally.unstarted,
                    detail.tally.started,
                    detail.tally.completed,
                    detail.tally.canceled
                );
                if !detail.issue_numbers.is_empty() {
                    println!(
                        "issue numbers: {}",
                        detail
                            .issue_numbers
                            .iter()
                            .map(|number| format!("#{number}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                if !detail.milestones.is_empty() {
                    println!("milestones:");
                    for milestone in &detail.milestones {
                        println!(
                            "  {}. {} [{}] (id {})",
                            milestone.position, milestone.name, milestone.status, milestone.id
                        );
                    }
                }
                println!();
                if detail.project.description.is_empty() {
                    println!("(no description)");
                } else {
                    println!("{}", detail.project.description);
                }
            }
        }
        ProjectCommand::Edit {
            project,
            name,
            summary,
            description,
            priority,
            json,
        } => {
            store
                .edit_project(
                    &project,
                    name.as_deref(),
                    summary.as_deref(),
                    description.as_deref(),
                    priority,
                )
                .await?;
            if json {
                println!("{}", serde_json::json!({ "updated": true }));
            } else {
                println!("updated project {project}");
            }
        }
        ProjectCommand::SetState {
            project,
            state,
            status_type,
            json,
        } => {
            store
                .set_project_state(&project, &state, &status_type)
                .await?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "state": state, "status_type": status_type })
                );
            } else {
                println!("project {project} -> {state} ({status_type})");
            }
        }
    }
    Ok(())
}
