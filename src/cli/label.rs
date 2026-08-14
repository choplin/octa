use super::output::{Output, Tone};
use super::{LabelCommand, LabelGroupCommand, LabelTarget};
use crate::store::Store;
use anyhow::Result;
use urushi::View;
pub(crate) async fn run(store: &Store, command: LabelCommand) -> Result<()> {
    let output = Output::stdout();
    match command {
        LabelCommand::Create {
            name,
            target,
            group,
        } => {
            match target {
                LabelTarget::Issue => store.create_label(&name, group.as_deref()).await?,
                LabelTarget::Project => store.create_project_label(&name, group.as_deref()).await?,
            }
            output.print(View::line(
                output.line(Tone::Success, format!("created label {name}")),
            ))
        }
        LabelCommand::List { target, json } => {
            let labels = match target {
                LabelTarget::Issue => store.list_labels().await?,
                LabelTarget::Project => store.list_project_labels().await?,
            };
            if json {
                println!("{}", serde_json::to_string(&labels)?)
            } else if labels.is_empty() {
                output.print(View::line(output.line(Tone::Warning, "no labels")))
            } else {
                let rows = labels
                    .iter()
                    .map(|label| [label.name.clone(), label.group.clone().unwrap_or_default()]);
                output.print(output.table(["Name", "Group"], rows))
            }
        }
    }
    Ok(())
}

pub(crate) async fn run_group(store: &Store, command: LabelGroupCommand) -> Result<()> {
    let output = Output::stdout();
    match command {
        LabelGroupCommand::Create {
            name,
            target,
            selection,
        } => {
            match target {
                LabelTarget::Issue => store.create_label_group(&name, &selection).await?,
                LabelTarget::Project => store.create_project_label_group(&name, &selection).await?,
            }
            output.print(View::line(output.line(
                Tone::Success,
                format!("created {selection} label group {name}"),
            )))
        }
        LabelGroupCommand::List { target, json } => {
            let groups = match target {
                LabelTarget::Issue => store.list_label_groups().await?,
                LabelTarget::Project => store.list_project_label_groups().await?,
            };
            if json {
                println!("{}", serde_json::to_string(&groups)?)
            } else if groups.is_empty() {
                output.print(View::line(output.line(Tone::Warning, "no label groups")))
            } else {
                let rows = groups
                    .iter()
                    .map(|group| [group.name.clone(), group.selection.clone()]);
                output.print(output.table(["Name", "Selection"], rows))
            }
        }
    }
    Ok(())
}
