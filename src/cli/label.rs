use super::LabelCommand;
use crate::store::Store;
use anyhow::Result;
pub(crate) async fn run(store: &Store, command: LabelCommand) -> Result<()> {
    match command {
        LabelCommand::Group { name, selection } => {
            store.create_label_group(&name, &selection).await?;
            println!("created {selection} label group {name}")
        }
        LabelCommand::Create { name, group } => {
            store.create_label(&name, group.as_deref()).await?;
            println!("created label {name}")
        }
        LabelCommand::List { json } => {
            let labels = store.list_labels().await?;
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
        LabelCommand::Groups { json } => {
            let groups = store.list_label_groups().await?;
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
