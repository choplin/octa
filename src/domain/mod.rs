pub mod comment;
pub mod issue;
pub mod label;
pub mod milestone;
pub mod project;
pub mod pull_request;
pub mod repository;
pub mod state_filter;
pub mod wiki;

pub use comment::Comment;
pub use state_filter::StateFilter;

/// Render a value set the way an error message should read: `"a", "b", or "c"`.
///
/// Errors that reject a value are only useful if they say what would have been
/// accepted, so every closed set is formatted through here rather than being
/// spelled out again at each call site.
pub fn join_options<I>(values: I) -> String
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let values = quoted(values);
    match values.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, [first])) => format!("{first} or {last}"),
        Some((last, rest)) => format!("{}, or {last}", rest.join(", ")),
    }
}

/// Render the values a rejected argument would have accepted.
///
/// A command to run still costs the caller another round trip before they can
/// retry; the values themselves are what makes the error actionable. Sets
/// backed by configuration stay small enough to print in full.
pub fn known_values<I>(values: I) -> String
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let values = quoted(values);
    match values.is_empty() {
        true => "none".to_string(),
        false => values.join(", "),
    }
}

/// Quote every value in a list, whether or not it needs it.
///
/// A name may contain a space — `in progress` is one — so an unquoted list is
/// ambiguous about where one value ends and the next begins. Quoting only the
/// ambiguous ones reads as if the quotes said something about that particular
/// value, so all of them are quoted.
fn quoted<I>(values: I) -> Vec<String>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    values
        .into_iter()
        .map(|value| format!("{:?}", value.as_ref()))
        .collect()
}
