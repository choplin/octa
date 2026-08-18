use super::output::{Output, Tone};
use super::StateCommand;
use crate::store::{StateType, Store};
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
                    [
                        state.name.clone(),
                        state.state_type.to_string(),
                        if state.is_default { "yes" } else { "" }.to_owned(),
                    ]
                });
                output.print(output.table(["Name", "Type", "Default"], rows));
            }
        }
        StateCommand::Create {
            name,
            state_type,
            default,
        } => {
            let state_type = StateType::parse(&state_type)?;
            let promoted = store.add_state(&name, state_type, default).await?;
            output.print(output.line(
                Tone::Success,
                promotion_note(format!("created state {name}"), promoted, state_type),
            ));
        }
        StateCommand::Set {
            name,
            new_name,
            state_type,
            default,
        } => {
            let state_type = state_type.as_deref().map(StateType::parse).transpose()?;
            let promoted = store
                .set_state_config(&name, new_name.as_deref(), state_type, default)
                .await?;
            let message = match &new_name {
                Some(new_name) if new_name != &name => {
                    format!("updated state {name} -> {new_name}")
                }
                _ => format!("updated state {name}"),
            };
            let message = match state_type {
                Some(state_type) => promotion_note(message, promoted, state_type),
                None => message,
            };
            output.print(output.line(Tone::Success, message));
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
    }
    Ok(())
}

/// Say so when a state became its type's default without being asked to, so the
/// promotion is never a silent side effect of creating or retyping it.
fn promotion_note(message: String, promoted: bool, state_type: StateType) -> String {
    if promoted {
        format!("{message}; it is the first {state_type} state, so it is now that type's default")
    } else {
        message
    }
}
