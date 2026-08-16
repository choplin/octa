use super::output::{Output, Tone};
use super::StateCommand;
use crate::store::Store;
use anyhow::Result;
pub(crate) async fn run(store: &Store, command: StateCommand) -> Result<()> {
    let output = Output::stdout();
    match command {
        StateCommand::List { json } => {
            let states = store.list_states().await?;
            if json {
                println!("{}", serde_json::to_string(&states)?)
            } else {
                let rows = states.iter().map(|state| {
                    let mut flags = Vec::new();
                    if state.is_starting {
                        flags.push("starting")
                    }
                    if state.is_terminal {
                        flags.push("terminal")
                    }
                    [state.name.clone(), flags.join(", ")]
                });
                output.print(output.table(["Name", "Flags"], rows));
            }
        }
        StateCommand::Create {
            name,
            starting,
            terminal,
        } => {
            store.add_state(&name, starting, terminal).await?;
            output.print(output.line(Tone::Success, format!("created state {name}")));
        }
        StateCommand::Set {
            name,
            new_name,
            terminal,
        } => {
            store
                .set_state_config(&name, new_name.as_deref(), terminal)
                .await?;
            match new_name {
                Some(new_name) if new_name != name => output.print(
                    output.line(Tone::Success, format!("updated state {name} -> {new_name}")),
                ),
                _ => output.print(output.line(Tone::Success, format!("updated state {name}"))),
            }
        }
        StateCommand::Delete { name, move_to } => {
            let moved = store.delete_state(&name, move_to.as_deref()).await?;
            let message = match (moved, move_to) {
                (0, _) => format!("deleted state {name}"),
                (moved, Some(move_to)) => {
                    format!("deleted state {name}; moved {moved} issue(s) to {move_to}")
                }
                (moved, None) => format!("deleted state {name}; {moved} issue(s) affected"),
            };
            output.print(output.line(Tone::Success, message))
        }
        StateCommand::SetDefault { name } => {
            store.set_default_state(&name).await?;
            output.print(output.line(Tone::Success, format!("new issues now start in {name}")));
        }
    }
    Ok(())
}
