use super::output::{Output, Tone};
use super::{MilestoneCommand, ProjectCommand};
use crate::store::Store;
use anyhow::Result;

pub(crate) async fn run(store: &Store, command: ProjectCommand) -> Result<()> {
    let output = Output::stdout();
    match command {
        ProjectCommand::Create {
            name,
            as_state,
            summary,
            description,
            json,
        } => {
            let id = store
                .create_project(&name, &summary, &description, as_state.as_deref())
                .await?;
            if json {
                println!("{}", serde_json::json!({ "id": id, "name": name }));
            } else {
                output.print(output.line(Tone::Success, format!("project {id}: {name}")))?;
            }
        }
        ProjectCommand::List { active, json } => {
            let projects = store.list_projects(active).await?;
            if json {
                println!("{}", serde_json::to_string(&projects)?);
            } else if projects.is_empty() {
                output.print(output.line(Tone::Warning, "no projects"))?;
            } else {
                let rows = projects.iter().map(|project| {
                    let milestones = project
                        .milestones
                        .iter()
                        .map(|milestone| {
                            format!(
                                "{} {} {}",
                                milestone.position, milestone.status, milestone.name
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    [
                        project.project.id.to_string(),
                        project.project.state.clone(),
                        format!("{}/{}", project.tally.open, project.tally.closed),
                        project.project.name.clone(),
                        milestones,
                    ]
                });
                output.print(
                    output.table(["ID", "State", "Open/Closed", "Name", "Milestones"], rows),
                )?;
            }
        }
        ProjectCommand::Show { project, json } => {
            let detail = store.project_detail(&project).await?;
            if json {
                println!("{}", serde_json::to_string(&detail)?);
            } else {
                let mut lines = vec![
                    output.row(
                        format!("{}", detail.project.id),
                        format!(": {} ({})", detail.project.name, detail.project.state),
                    ),
                    output.field("summary: ", &detail.project.summary),
                ];
                if !detail.labels.is_empty() {
                    lines.push(output.field("labels: ", detail.labels.join(", ")));
                }
                lines.push(output.field(
                    "issues: ",
                    format!(
                        "{} (open {}, closed {})",
                        detail.tally.total, detail.tally.open, detail.tally.closed
                    ),
                ));
                if !detail.issue_numbers.is_empty() {
                    lines.push(
                        output.field(
                            "issue numbers: ",
                            detail
                                .issue_numbers
                                .iter()
                                .map(|number| format!("#{number}"))
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                    );
                }
                if !detail.milestones.is_empty() {
                    lines.push(output.line(Tone::Accent, "milestones:"));
                    for milestone in &detail.milestones {
                        lines.push(output.line(
                            Tone::Body,
                            format!(
                                "  {}. {} [{}] (id {})",
                                milestone.position, milestone.name, milestone.status, milestone.id
                            ),
                        ));
                    }
                }
                lines.push(output.line(Tone::Body, ""));
                if detail.project.description.is_empty() {
                    lines.push(output.line(Tone::Warning, "(no description)"));
                } else {
                    lines.push(output.line(Tone::Body, &detail.project.description));
                }
                output.print_lines(lines)?;
            }
        }
        ProjectCommand::Set {
            project,
            as_state,
            name,
            summary,
            description,
            json,
        } => {
            if as_state.is_none() && name.is_none() && summary.is_none() && description.is_none() {
                anyhow::bail!("specify at least one property to set");
            }
            if let Some(state) = &as_state {
                store.set_project_state(&project, state).await?;
            }
            if name.is_some() || summary.is_some() || description.is_some() {
                store
                    .edit_project(
                        &project,
                        name.as_deref(),
                        summary.as_deref(),
                        description.as_deref(),
                    )
                    .await?;
            }
            if json {
                println!("{}", serde_json::json!({ "updated": true }));
            } else {
                output.print(output.line(Tone::Success, format!("updated project {project}")))?;
            }
        }
        ProjectCommand::Add { project, label } => {
            store.label_project(&project, &label).await?;
            output.print(output.line(Tone::Success, format!("updated project {project}")))?;
        }
        ProjectCommand::Remove { project, label } => {
            store.unlabel_project(&project, &label).await?;
            output.print(output.line(Tone::Success, format!("updated project {project}")))?;
        }
        ProjectCommand::Close {
            project,
            as_state,
            json,
        } => {
            let state = store.close_project(&project, as_state.as_deref()).await?;
            report_move(&output, json, &project, &state)?;
        }
        ProjectCommand::Reopen {
            project,
            as_state,
            json,
        } => {
            let state = store.reopen_project(&project, as_state.as_deref()).await?;
            report_move(&output, json, &project, &state)?;
        }
    }
    Ok(())
}

fn report_move(output: &Output, json: bool, project: &str, state: &str) -> Result<()> {
    if json {
        println!("{}", serde_json::json!({ "state": state }));
    } else {
        output.print(output.line(Tone::Success, format!("project {project} -> {state}")))?;
    }
    Ok(())
}

pub(crate) async fn run_milestone(store: &Store, command: MilestoneCommand) -> Result<()> {
    let output = Output::stdout();
    match command {
        MilestoneCommand::Create {
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
                output.print(output.line(Tone::Success, format!("milestone {id}: {name}")))?;
            }
        }
        MilestoneCommand::List { project, json } => {
            let milestones = store.list_project_milestones(&project).await?;
            if json {
                println!("{}", serde_json::to_string(&milestones)?);
            } else if milestones.is_empty() {
                output.print(output.line(Tone::Warning, "no milestones"))?;
            } else {
                let rows = milestones.iter().map(|milestone| {
                    [
                        milestone.id.to_string(),
                        milestone.position.to_string(),
                        milestone.status.clone(),
                        milestone.name.clone(),
                    ]
                });
                output.print(output.table(["ID", "Position", "Status", "Name"], rows))?;
            }
        }
        MilestoneCommand::Show {
            project,
            milestone,
            json,
        } => {
            let milestone = store.project_milestone(&project, &milestone).await?;
            if json {
                println!("{}", serde_json::to_string(&milestone)?);
            } else {
                let mut lines = vec![
                    output.row(
                        milestone.id.to_string(),
                        format!(
                            ": {} (position {}, {})",
                            milestone.name, milestone.position, milestone.status
                        ),
                    ),
                    output.field(
                        "dates: ",
                        format!(
                            "{} -> {}",
                            milestone.start_date.as_deref().unwrap_or("(none)"),
                            milestone.target_date.as_deref().unwrap_or("(none)")
                        ),
                    ),
                    output.line(Tone::Body, ""),
                ];
                if milestone.description.is_empty() {
                    lines.push(output.line(Tone::Warning, "(no description)"));
                } else {
                    lines.push(output.line(Tone::Body, &milestone.description));
                }
                output.print_lines(lines)?;
            }
        }
        MilestoneCommand::Set {
            project,
            milestone,
            name,
            description,
            status,
            position,
            start_date,
            target_date,
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
                    false,
                    false,
                )
                .await?;
            if json {
                println!("{}", serde_json::json!({ "updated": true }));
            } else {
                output
                    .print(output.line(Tone::Success, format!("updated milestone {milestone}")))?;
            }
        }
        MilestoneCommand::Unset {
            project,
            milestone,
            start_date,
            target_date,
            json,
        } => {
            if !start_date && !target_date {
                anyhow::bail!("specify at least one property to unset");
            }
            store
                .edit_project_milestone(
                    &project,
                    &milestone,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    start_date,
                    target_date,
                )
                .await?;
            if json {
                println!("{}", serde_json::json!({ "updated": true }));
            } else {
                output
                    .print(output.line(Tone::Success, format!("updated milestone {milestone}")))?;
            }
        }
    }
    Ok(())
}
