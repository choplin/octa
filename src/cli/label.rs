use super::{LabelCommand, LabelGroupCommand, LabelTarget};
use crate::store::Store;
use anyhow::Result;
pub(crate) async fn run(store: &Store, command: LabelCommand) -> Result<()> {
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
            println!("created label {name}")
        }
        LabelCommand::List { target, json } => {
            let labels = match target {
                LabelTarget::Issue => store.list_labels().await?,
                LabelTarget::Project => store.list_project_labels().await?,
            };
            if json {
                println!("{}", serde_json::to_string(&labels)?)
            } else if labels.is_empty() {
                println!("no labels")
            } else {
                for label in labels {
                    match label.group {
                        Some(group) => println!("{:<20} ({group})", label.name),
                        None => println!("{}", label.name),
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) async fn run_group(store: &Store, command: LabelGroupCommand) -> Result<()> {
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
            println!("created {selection} label group {name}")
        }
        LabelGroupCommand::List { target, json } => {
            let groups = match target {
                LabelTarget::Issue => store.list_label_groups().await?,
                LabelTarget::Project => store.list_project_label_groups().await?,
            };
            if json {
                println!("{}", serde_json::to_string(&groups)?)
            } else if groups.is_empty() {
                println!("no label groups")
            } else {
                for group in groups {
                    println!("{:<20} {}", group.name, group.selection)
                }
            }
        }
    }
    Ok(())
}
