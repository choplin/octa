use super::StateCommand;
use crate::store::Store;
use anyhow::Result;
pub(crate) async fn run(store: &Store, command: StateCommand) -> Result<()> {
    match command {
        StateCommand::List { json } => {
            let states = store.list_states().await?;
            if json {
                println!("{}", serde_json::to_string(&states)?)
            } else {
                for state in states {
                    let mut flags = Vec::new();
                    if state.is_starting {
                        flags.push("starting")
                    }
                    if state.is_terminal {
                        flags.push("terminal")
                    }
                    println!(
                        "{:<14} {:<10} {}",
                        state.name,
                        state.status_type,
                        flags.join(", ")
                    );
                }
            }
        }
        StateCommand::Create {
            name,
            status_type,
            starting,
            terminal,
        } => {
            store
                .add_state(&name, status_type.as_deref(), starting, terminal)
                .await?;
            println!("created state {name}");
        }
        StateCommand::Set {
            name,
            new_name,
            status_type,
            terminal,
        } => {
            store
                .set_state_config(&name, new_name.as_deref(), status_type.as_deref(), terminal)
                .await?;
            match new_name {
                Some(new_name) if new_name != name => {
                    println!("updated state {name} -> {new_name}")
                }
                _ => println!("updated state {name}"),
            }
        }
        StateCommand::Delete { name, move_to } => {
            let moved = store.delete_state(&name, move_to.as_deref()).await?;
            match (moved, move_to) {
                (0, _) => println!("deleted state {name}"),
                (moved, Some(move_to)) => {
                    println!("deleted state {name}; moved {moved} issue(s) to {move_to}")
                }
                (moved, None) => println!("deleted state {name}; {moved} issue(s) affected"),
            }
        }
        StateCommand::SetDefault { name } => {
            store.set_default_state(&name).await?;
            println!("new issues now start in {name}");
        }
    }
    Ok(())
}
