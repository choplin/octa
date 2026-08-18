//! The value sets `--help` can only learn from the database.
//!
//! Sets fixed in code are declared on the argument itself. The rest — states,
//! labels, and label groups — live in configuration, so the help text is filled
//! in from the store just before clap renders it. Without this, an option whose
//! values are configurable can only name a command to run, which leaves the
//! reader one round trip short of knowing what to type.
//!
//! Loading is skipped unless the invocation actually asks for help, so the
//! ordinary command path pays nothing for it.

use crate::domain::issue::StateType;
use crate::domain::project::ProjectStateType;
use clap::builder::PossibleValuesParser;
use clap::Command;
use std::ffi::OsString;

/// Whether this invocation is going to print help.
pub(crate) fn wants_help<I: IntoIterator<Item = OsString>>(args: I) -> bool {
    args.into_iter()
        .any(|arg| matches!(arg.to_str(), Some("-h" | "--help" | "help")))
}

/// The configured sets, read once per help invocation.
pub(crate) struct HelpValues {
    issue_states: Vec<String>,
    issue_open_states: Vec<String>,
    issue_closed_states: Vec<String>,
    project_states: Vec<String>,
    project_open_states: Vec<String>,
    project_closed_states: Vec<String>,
    issue_labels: Vec<String>,
    project_labels: Vec<String>,
    issue_label_groups: Vec<String>,
    project_label_groups: Vec<String>,
}

pub(crate) async fn load() -> Option<HelpValues> {
    let pool = crate::store::open_existing_pool().await?;
    let issue_states = crate::sql::issue::list_states(&pool).await.ok()?;
    let project_states = crate::sql::project::list_states(&pool).await.ok()?;
    let of_type = |wanted: StateType| -> Vec<String> {
        issue_states
            .iter()
            .filter(|state| state.state_type == wanted)
            .map(|state| state.name.clone())
            .collect()
    };
    let project_of_type = |wanted: ProjectStateType| -> Vec<String> {
        project_states
            .iter()
            .filter(|state| state.state_type == wanted)
            .map(|state| state.name.clone())
            .collect()
    };
    Some(HelpValues {
        issue_open_states: of_type(StateType::Open),
        issue_closed_states: of_type(StateType::Closed),
        issue_states: issue_states.iter().map(|s| s.name.clone()).collect(),
        project_open_states: project_of_type(ProjectStateType::Open),
        project_closed_states: project_of_type(ProjectStateType::Closed),
        project_states: project_states.iter().map(|s| s.name.clone()).collect(),
        issue_labels: names(crate::sql::label::list(&pool).await.ok()?),
        project_labels: names(crate::sql::label::list_project_labels(&pool).await.ok()?),
        issue_label_groups: group_names(crate::sql::label::list_groups(&pool).await.ok()?),
        project_label_groups: group_names(
            crate::sql::label::list_project_groups(&pool).await.ok()?,
        ),
    })
}

fn names(labels: Vec<crate::domain::label::Label>) -> Vec<String> {
    labels.into_iter().map(|label| label.name).collect()
}

fn group_names(groups: Vec<crate::domain::label::LabelGroup>) -> Vec<String> {
    groups.into_iter().map(|group| group.name).collect()
}

/// Fill every option whose values are configured rather than fixed.
///
/// A verb narrowed to one state type advertises only that type's states: the
/// wider set would list values `--as` would then refuse.
pub(crate) fn augment(command: Command, values: &HelpValues) -> Command {
    let entries: [(&[&str], &str, &Vec<String>); 21] = [
        (&["issue", "open"], "as_state", &values.issue_open_states),
        (&["issue", "create"], "as_state", &values.issue_open_states),
        (&["issue", "close"], "as_state", &values.issue_closed_states),
        (&["issue", "reopen"], "as_state", &values.issue_open_states),
        (&["issue", "set"], "as_state", &values.issue_states),
        (&["issue", "list"], "state", &values.issue_states),
        (&["issue", "list"], "label", &values.issue_labels),
        (&["issue", "add"], "label", &values.issue_labels),
        (&["issue", "remove"], "label", &values.issue_labels),
        (
            &["project", "create"],
            "as_state",
            &values.project_open_states,
        ),
        (&["project", "set"], "as_state", &values.project_states),
        (
            &["project", "close"],
            "as_state",
            &values.project_closed_states,
        ),
        (
            &["project", "reopen"],
            "as_state",
            &values.project_open_states,
        ),
        (&["project", "add"], "label", &values.project_labels),
        (&["project", "remove"], "label", &values.project_labels),
        (
            &["config", "issue", "state", "set"],
            "name",
            &values.issue_states,
        ),
        (
            &["config", "issue", "state", "delete"],
            "name",
            &values.issue_states,
        ),
        (
            &["config", "issue", "state", "delete"],
            "move_to",
            &values.issue_states,
        ),
        (
            &["config", "project", "state", "set"],
            "name",
            &values.project_states,
        ),
        (
            &["config", "project", "state", "delete"],
            "name",
            &values.project_states,
        ),
        (
            &["config", "project", "state", "delete"],
            "move_to",
            &values.project_states,
        ),
    ];
    let mut command = command;
    for (path, arg, allowed) in entries {
        command = set_values(command, path, arg, allowed);
    }
    // `label create --group` is one definition shared by the Issue and Project
    // command paths, so each path is filled with its own groups.
    command = set_values(
        command,
        &["config", "issue", "label", "create"],
        "group",
        &values.issue_label_groups,
    );
    set_values(
        command,
        &["config", "project", "label", "create"],
        "group",
        &values.project_label_groups,
    )
}

fn set_values(command: Command, path: &[&str], arg: &str, allowed: &[String]) -> Command {
    // An empty set would render as an option that accepts nothing at all, which
    // is worse than saying nothing: leave the declared help in place.
    if allowed.is_empty() {
        return command;
    }
    match path.split_first() {
        None => command.mut_arg(arg, |argument| {
            argument.value_parser(PossibleValuesParser::new(allowed.iter()))
        }),
        Some((head, rest)) => {
            command.mut_subcommand(head, |sub| set_values(sub, rest, arg, allowed))
        }
    }
}

/// Render every possible-value list with each value quoted.
///
/// clap quotes a value only when it contains a space, so one set can come out
/// as `open, "in progress", closed` — the quoting then reads as if it meant
/// something about that value rather than about the rendering. Quoting all of
/// them keeps the list uniform, and also shows where each value begins and ends
/// when a name itself contains punctuation.
pub(crate) fn quote_possible_values(command: Command) -> Command {
    let arg_ids: Vec<String> = command
        .get_arguments()
        // Flags carry an implicit true/false set that is never rendered, and
        // clap rejects hiding possible values on an argument that takes none.
        .filter(|arg| arg.get_action().takes_values())
        .filter(|arg| !arg.is_hide_possible_values_set() && !arg.get_possible_values().is_empty())
        .map(|arg| arg.get_id().to_string())
        .collect();
    let mut command = command;
    for id in arg_ids {
        command = command.mut_arg(id, |arg| {
            let quoted: Vec<String> = arg
                .get_possible_values()
                .iter()
                .map(|value| format!("{:?}", value.get_name()))
                .collect();
            let mut help = arg
                .get_help()
                .map(ToString::to_string)
                .unwrap_or_default()
                .trim_end()
                .to_string();
            if !help.is_empty() {
                help.push(' ');
            }
            help.push_str(&format!("[possible values: {}]", quoted.join(", ")));
            arg.hide_possible_values(true).help(help)
        });
    }
    let names: Vec<String> = command
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect();
    for name in names {
        command = command.mut_subcommand(name, quote_possible_values);
    }
    command
}
