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
        StateCommand::Add {
            name,
            status_type,
            starting,
            terminal,
        } => {
            store
                .add_state(&name, status_type.as_deref(), starting, terminal)
                .await?;
            println!("added state {name}");
        }
    }
    Ok(())
}
