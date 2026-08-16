//! Selection-aware SQL compiler.

use super::model::{IssueFilter, LabelTarget, ProjectFilter};
use super::{DEFAULT_LIMIT, MAX_LIMIT};
use async_graphql::{Name, SelectionField, Value};

pub(super) struct Planner {
    alias: usize,
    /// The active repository. Labels are global configuration, so a label's
    /// issues and projects are scoped by the active repository rather than by
    /// the label row itself.
    repo: i64,
}

impl Planner {
    pub(super) fn new(repo: i64) -> Self {
        Self { alias: 0, repo }
    }
    fn next(&mut self, prefix: &str) -> String {
        self.alias += 1;
        format!("{prefix}{}", self.alias)
    }

    pub(super) fn issue(
        &mut self,
        fields: &[SelectionField<'_>],
        issue: &str,
        state: Option<&str>,
    ) -> async_graphql::Result<String> {
        let mut pairs = Vec::new();
        for merged in merged_fields(fields) {
            let field = &merged.field;
            let key = response_key(field);
            let value = match field.name() {
                "number" | "title" | "body" | "priority" | "createdAt" | "updatedAt" => {
                    format!("{issue}.{}", snake(field.name()))
                }
                "state" => format!("{issue}.state"),
                "isTerminal" => format!(
                    "json(CASE WHEN {}.is_terminal=1 THEN 'true' ELSE 'false' END)",
                    state.expect("isTerminal requires state join")
                ),
                "leased" => {
                    let lease = self.next("lease");
                    format!(
                        "json(CASE WHEN EXISTS(SELECT 1 FROM issue_leases {lease} WHERE {lease}.repo_id={issue}.repo_id AND {lease}.issue_number={issue}.number) THEN 'true' ELSE 'false' END)"
                    )
                }
                "project" => {
                    let p = self.next("p");
                    let ip = self.next("ip");
                    let nested = self.project(&merged.children, &p)?;
                    format!("(SELECT {nested} FROM issue_projects {ip} JOIN projects {p} ON {p}.repo_id={ip}.repo_id AND {p}.id={ip}.project_id WHERE {ip}.repo_id={issue}.repo_id AND {ip}.issue_number={issue}.number)")
                }
                "milestone" => {
                    let m = self.next("m");
                    let im = self.next("im");
                    let nested = self.milestone(&merged.children, &m)?;
                    format!("(SELECT {nested} FROM issue_milestones {im} JOIN project_milestones {m} ON {m}.repo_id={im}.repo_id AND {m}.project_id={im}.project_id AND {m}.id={im}.milestone_id WHERE {im}.repo_id={issue}.repo_id AND {im}.issue_number={issue}.number)")
                }
                "labels" => {
                    let l = self.next("l");
                    let x = self.next("il");
                    let item = self.label(&merged.children, &l, LabelTarget::Issue)?;
                    let page = Page::from_field(field)?;
                    list(format!("SELECT {item} item FROM issue_labels {x} JOIN labels {l} ON {l}.name={x}.label_name WHERE {x}.repo_id={issue}.repo_id AND {x}.issue_number={issue}.number ORDER BY {l}.name {}", page.sql()))
                }
                "blocks" | "blockedBy" => {
                    let i = self.next("i");
                    let s = self.next("s");
                    let d = self.next("d");
                    let state = issue_state_join(&merged.children, &i, &s, false);
                    let nested = self.issue(&merged.children, &i, state.alias())?;
                    let (join_col, parent_col) = if field.name() == "blocks" {
                        ("blocked_number", "blocker_number")
                    } else {
                        ("blocker_number", "blocked_number")
                    };
                    let page = Page::from_field(field)?;
                    list(format!("SELECT {nested} item FROM issue_deps {d} JOIN issues {i} ON {i}.repo_id={d}.repo_id AND {i}.number={d}.{join_col} {} WHERE {d}.repo_id={issue}.repo_id AND {d}.{parent_col}={issue}.number ORDER BY {i}.number {}", state.sql(), page.sql()))
                }
                "related" => {
                    let i = self.next("i");
                    let s = self.next("s");
                    let r = self.next("r");
                    let state = issue_state_join(&merged.children, &i, &s, false);
                    let nested = self.issue(&merged.children, &i, state.alias())?;
                    let page = Page::from_field(field)?;
                    list(format!("SELECT {nested} item FROM issue_relations {r} JOIN issues {i} ON {i}.repo_id={r}.repo_id AND {i}.number=CASE WHEN {r}.low_number={issue}.number THEN {r}.high_number ELSE {r}.low_number END {} WHERE {r}.repo_id={issue}.repo_id AND ({r}.low_number={issue}.number OR {r}.high_number={issue}.number) ORDER BY {i}.number {}", state.sql(), page.sql()))
                }
                "parent" => {
                    let i = self.next("i");
                    let s = self.next("s");
                    let r = self.next("par");
                    let state = issue_state_join(&merged.children, &i, &s, false);
                    let nested = self.issue(&merged.children, &i, state.alias())?;
                    format!("(SELECT {nested} FROM issue_parents {r} JOIN issues {i} ON {i}.repo_id={r}.repo_id AND {i}.number={r}.parent_number {} WHERE {r}.repo_id={issue}.repo_id AND {r}.child_number={issue}.number)", state.sql())
                }
                "subIssues" => {
                    let i = self.next("i");
                    let s = self.next("s");
                    let r = self.next("par");
                    let state = issue_state_join(&merged.children, &i, &s, false);
                    let nested = self.issue(&merged.children, &i, state.alias())?;
                    let page = Page::from_field(field)?;
                    list(format!("SELECT {nested} item FROM issue_parents {r} JOIN issues {i} ON {i}.repo_id={r}.repo_id AND {i}.number={r}.child_number {} WHERE {r}.repo_id={issue}.repo_id AND {r}.parent_number={issue}.number ORDER BY {i}.number {}", state.sql(), page.sql()))
                }
                "pullRequests" => {
                    let p = self.next("pr");
                    let x = self.next("ipl");
                    let nested = self.pull_request(&merged.children, &p)?;
                    let page = Page::from_field(field)?;
                    list(format!("SELECT {nested} item FROM issue_pr_links {x} JOIN prs {p} ON {p}.repo_id={x}.repo_id AND {p}.number={x}.pr_number WHERE {x}.repo_id={issue}.repo_id AND {x}.issue_number={issue}.number ORDER BY {p}.number {}", page.sql()))
                }
                name => return Err(format!("unsupported Issue selection {name}").into()),
            };
            pairs.push(json_pair(&key, &value));
        }
        Ok(json_object(pairs))
    }

    pub(super) fn project(
        &mut self,
        fields: &[SelectionField<'_>],
        project: &str,
    ) -> async_graphql::Result<String> {
        let mut pairs = Vec::new();
        for merged in merged_fields(fields) {
            let field = &merged.field;
            let key = response_key(field);
            let value = match field.name() {
                "id" | "name" | "summary" | "description" | "state" | "priority" | "createdAt"
                | "updatedAt" => format!("{project}.{}", snake(field.name())),
                "isTerminal" => {
                    format!("json(CASE WHEN {project}.is_terminal=1 THEN 'true' ELSE 'false' END)")
                }
                "issues" => {
                    let i = self.next("i");
                    let s = self.next("s");
                    let x = self.next("ip");
                    let filter_args = issue_filter(field)?;
                    let state = issue_state_join(
                        &merged.children,
                        &i,
                        &s,
                        filter_args.is_terminal.is_some(),
                    );
                    let nested = self.issue(&merged.children, &i, state.alias())?;
                    let filter = issue_filter_sql(&i, state.alias(), &filter_args);
                    let page = Page::from_field(field)?;
                    list(format!("SELECT {nested} item FROM issue_projects {x} JOIN issues {i} ON {i}.repo_id={x}.repo_id AND {i}.number={x}.issue_number {} WHERE {x}.repo_id={project}.repo_id AND {x}.project_id={project}.id {filter} ORDER BY {i}.number {}", state.sql(), page.sql()))
                }
                "milestones" => {
                    let m = self.next("m");
                    let nested = self.milestone(&merged.children, &m)?;
                    let page = Page::from_field(field)?;
                    list(format!("SELECT {nested} item FROM project_milestones {m} WHERE {m}.repo_id={project}.repo_id AND {m}.project_id={project}.id ORDER BY {m}.position,{m}.id {}", page.sql()))
                }
                "labels" => {
                    let l = self.next("l");
                    let x = self.next("pl");
                    let nested = self.label(&merged.children, &l, LabelTarget::Project)?;
                    let page = Page::from_field(field)?;
                    list(format!("SELECT {nested} item FROM project_label_links {x} JOIN project_labels {l} ON {l}.name={x}.label_name WHERE {x}.repo_id={project}.repo_id AND {x}.project_id={project}.id ORDER BY {l}.name {}", page.sql()))
                }
                name => return Err(format!("unsupported Project selection {name}").into()),
            };
            pairs.push(json_pair(&key, &value));
        }
        Ok(json_object(pairs))
    }

    pub(super) fn milestone(
        &mut self,
        fields: &[SelectionField<'_>],
        milestone: &str,
    ) -> async_graphql::Result<String> {
        let mut pairs = Vec::new();
        for merged in merged_fields(fields) {
            let field = &merged.field;
            let key = response_key(field);
            let value = match field.name() {
                "id" | "position" | "name" | "description" | "status" | "startDate"
                | "targetDate" | "createdAt" | "updatedAt" => {
                    format!("{milestone}.{}", snake(field.name()))
                }
                "project" => {
                    let p = self.next("p");
                    let nested = self.project(&merged.children, &p)?;
                    format!("(SELECT {nested} FROM projects {p} WHERE {p}.repo_id={milestone}.repo_id AND {p}.id={milestone}.project_id)")
                }
                "issues" => {
                    let i = self.next("i");
                    let s = self.next("s");
                    let x = self.next("im");
                    let state = issue_state_join(&merged.children, &i, &s, false);
                    let nested = self.issue(&merged.children, &i, state.alias())?;
                    let page = Page::from_field(field)?;
                    list(format!("SELECT {nested} item FROM issue_milestones {x} JOIN issues {i} ON {i}.repo_id={x}.repo_id AND {i}.number={x}.issue_number {} WHERE {x}.repo_id={milestone}.repo_id AND {x}.project_id={milestone}.project_id AND {x}.milestone_id={milestone}.id ORDER BY {i}.number {}", state.sql(), page.sql()))
                }
                name => return Err(format!("unsupported Milestone selection {name}").into()),
            };
            pairs.push(json_pair(&key, &value));
        }
        Ok(json_object(pairs))
    }

    pub(super) fn pull_request(
        &mut self,
        fields: &[SelectionField<'_>],
        pr: &str,
    ) -> async_graphql::Result<String> {
        let mut pairs = Vec::new();
        for merged in merged_fields(fields) {
            let field = &merged.field;
            let key = response_key(field);
            let value = match field.name() {
                "number" | "title" | "body" | "branch" | "state" | "createdAt" | "updatedAt" => {
                    format!("{pr}.{}", snake(field.name()))
                }
                "issues" => {
                    let i = self.next("i");
                    let s = self.next("s");
                    let x = self.next("ipl");
                    let state = issue_state_join(&merged.children, &i, &s, false);
                    let nested = self.issue(&merged.children, &i, state.alias())?;
                    let page = Page::from_field(field)?;
                    list(format!("SELECT {nested} item FROM issue_pr_links {x} JOIN issues {i} ON {i}.repo_id={x}.repo_id AND {i}.number={x}.issue_number {} WHERE {x}.repo_id={pr}.repo_id AND {x}.pr_number={pr}.number ORDER BY {i}.number {}", state.sql(), page.sql()))
                }
                name => return Err(format!("unsupported PullRequest selection {name}").into()),
            };
            pairs.push(json_pair(&key, &value));
        }
        Ok(json_object(pairs))
    }

    pub(super) fn wiki(
        &mut self,
        fields: &[SelectionField<'_>],
        wiki: &str,
    ) -> async_graphql::Result<String> {
        let mut pairs = Vec::new();
        for merged in merged_fields(fields) {
            let field = &merged.field;
            let key = response_key(field);
            let value = match field.name() {
                "slug" | "title" | "body" | "createdAt" | "updatedAt" => {
                    format!("{wiki}.{}", snake(field.name()))
                }
                "linksTo" | "backlinks" => {
                    let w = self.next("w");
                    let x = self.next("wl");
                    let nested = self.wiki(&merged.children, &w)?;
                    let page = Page::from_field(field)?;
                    let (join, predicate) = if field.name() == "linksTo" {
                        ("to_slug", "from_slug")
                    } else {
                        ("from_slug", "to_slug")
                    };
                    list(format!("SELECT {nested} item FROM wiki_links {x} JOIN wiki_pages {w} ON {w}.repo_id={x}.repo_id AND {w}.slug={x}.{join} WHERE {x}.repo_id={wiki}.repo_id AND {x}.{predicate}={wiki}.slug ORDER BY {w}.slug {}", page.sql()))
                }
                name => return Err(format!("unsupported WikiPage selection {name}").into()),
            };
            pairs.push(json_pair(&key, &value));
        }
        Ok(json_object(pairs))
    }

    pub(super) fn label(
        &mut self,
        fields: &[SelectionField<'_>],
        label: &str,
        target: LabelTarget,
    ) -> async_graphql::Result<String> {
        let mut pairs = Vec::new();
        for merged in merged_fields(fields) {
            let field = &merged.field;
            let key = response_key(field);
            let value = match field.name() {
                "name" => format!("{label}.name"),
                "group" => format!("{label}.group_name"),
                "target" => quote(if target == LabelTarget::Issue {
                    "ISSUE"
                } else {
                    "PROJECT"
                }),
                "issues" if target == LabelTarget::Issue => {
                    let i = self.next("i");
                    let s = self.next("s");
                    let x = self.next("il");
                    let state = issue_state_join(&merged.children, &i, &s, false);
                    let nested = self.issue(&merged.children, &i, state.alias())?;
                    let page = Page::from_field(field)?;
                    let repo = self.repo;
                    list(format!("SELECT {nested} item FROM issue_labels {x} JOIN issues {i} ON {i}.repo_id={x}.repo_id AND {i}.number={x}.issue_number {} WHERE {x}.repo_id={repo} AND {x}.label_name={label}.name ORDER BY {i}.number {}", state.sql(), page.sql()))
                }
                "projects" if target == LabelTarget::Project => {
                    let p = self.next("p");
                    let x = self.next("pl");
                    let nested = self.project(&merged.children, &p)?;
                    let page = Page::from_field(field)?;
                    let repo = self.repo;
                    list(format!("SELECT {nested} item FROM project_label_links {x} JOIN projects {p} ON {p}.repo_id={x}.repo_id AND {p}.id={x}.project_id WHERE {x}.repo_id={repo} AND {x}.label_name={label}.name ORDER BY {p}.id {}", page.sql()))
                }
                "issues" | "projects" => "json('[]')".to_string(),
                name => return Err(format!("unsupported Label selection {name}").into()),
            };
            pairs.push(json_pair(&key, &value));
        }
        Ok(json_object(pairs))
    }
}

#[derive(Clone, Copy)]
pub(super) struct Page {
    offset: i64,
    limit: i64,
}

impl Page {
    pub(super) fn new(offset: Option<i64>, limit: Option<i64>) -> async_graphql::Result<Self> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(DEFAULT_LIMIT);
        if offset < 0 {
            return Err("offset must be non-negative".into());
        }
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(format!("limit must be between 1 and {MAX_LIMIT}").into());
        }
        Ok(Self { offset, limit })
    }
    fn from_field(field: &SelectionField<'_>) -> async_graphql::Result<Self> {
        Self::new(
            integer_argument(field, "offset")?,
            integer_argument(field, "limit")?,
        )
    }
    pub(super) fn sql(self) -> String {
        format!("LIMIT {} OFFSET {}", self.limit, self.offset)
    }
}

fn response_key(field: &SelectionField<'_>) -> String {
    field.alias().unwrap_or_else(|| field.name()).to_string()
}

struct MergedField<'a> {
    field: SelectionField<'a>,
    children: Vec<SelectionField<'a>>,
}

pub(super) struct StateJoin<'a> {
    issue: &'a str,
    alias: &'a str,
    required: bool,
}
impl StateJoin<'_> {
    pub(super) fn alias(&self) -> Option<&str> {
        self.required.then_some(self.alias)
    }
    pub(super) fn sql(&self) -> String {
        if self.required {
            format!(
                "JOIN issue_states {} ON {}.name={}.state",
                self.alias, self.alias, self.issue
            )
        } else {
            String::new()
        }
    }
}

pub(super) fn issue_state_join<'a>(
    fields: &[SelectionField<'_>],
    issue: &'a str,
    alias: &'a str,
    filter_requires: bool,
) -> StateJoin<'a> {
    let selected = merged_fields(fields)
        .iter()
        .any(|field| field.field.name() == "isTerminal");
    StateJoin {
        issue,
        alias,
        required: selected || filter_requires,
    }
}

fn merged_fields<'a>(fields: &[SelectionField<'a>]) -> Vec<MergedField<'a>> {
    let mut merged = Vec::<MergedField<'a>>::new();
    for field in fields
        .iter()
        .copied()
        .filter(|field| field.name() != "__typename")
    {
        let key = response_key(&field);
        if let Some(existing) = merged
            .iter_mut()
            .find(|item| response_key(&item.field) == key)
        {
            existing.children.extend(field.selection_set());
        } else {
            merged.push(MergedField {
                field,
                children: field.selection_set().collect(),
            });
        }
    }
    merged
}
fn snake(name: &str) -> String {
    let mut output = String::new();
    for character in name.chars() {
        if character.is_ascii_uppercase() {
            output.push('_');
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}
pub(super) fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn json_pair(key: &str, value: &str) -> String {
    format!(
        "{}, {value}",
        quote(&format!("$.{}", key.replace('"', "\\\"")))
    )
}
fn json_object(pairs: Vec<String>) -> String {
    pairs
        .into_iter()
        .fold("json('{}')".to_string(), |object, pair| {
            let (path, value) = pair.split_once(", ").expect("JSON pair contains separator");
            let encoded = if value.starts_with("(SELECT") || value.starts_with("json(") {
                format!("json(COALESCE({value},'null'))")
            } else {
                format!("json(json_quote({value}))")
            };
            format!("json_set({object},{path},{encoded})")
        })
}
fn list(select: String) -> String {
    format!("(SELECT COALESCE(json_group_array(json(item)),json('[]')) FROM ({select}))")
}

fn argument(field: &SelectionField<'_>, name: &str) -> async_graphql::Result<Option<Value>> {
    Ok(field
        .arguments()?
        .into_iter()
        .find_map(|(key, value)| (key.as_str() == name).then_some(value)))
}
fn integer_argument(field: &SelectionField<'_>, name: &str) -> async_graphql::Result<Option<i64>> {
    match argument(field, name)? {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_i64()
            .map(Some)
            .ok_or_else(|| format!("{name} must be an integer").into()),
        Some(_) => Err(format!("{name} must be an integer").into()),
    }
}
fn object_argument(
    field: &SelectionField<'_>,
    name: &str,
) -> async_graphql::Result<Option<async_graphql::indexmap::IndexMap<Name, Value>>> {
    match argument(field, name)? {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(object)) => Ok(Some(object)),
        Some(_) => Err(format!("{name} must be an object").into()),
    }
}
fn object_string(
    object: &async_graphql::indexmap::IndexMap<Name, Value>,
    name: &str,
) -> async_graphql::Result<Option<String>> {
    match object.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("{name} must be a string").into()),
    }
}
fn object_boolean(
    object: &async_graphql::indexmap::IndexMap<Name, Value>,
    name: &str,
) -> async_graphql::Result<Option<bool>> {
    match object.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Boolean(value)) => Ok(Some(*value)),
        Some(_) => Err(format!("{name} must be a boolean").into()),
    }
}
fn object_integer(
    object: &async_graphql::indexmap::IndexMap<Name, Value>,
    name: &str,
) -> async_graphql::Result<Option<i64>> {
    match object.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(value)) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| format!("{name} must be an integer").into()),
        Some(_) => Err(format!("{name} must be an integer").into()),
    }
}
fn issue_filter(field: &SelectionField<'_>) -> async_graphql::Result<IssueFilter> {
    let Some(object) = object_argument(field, "filter")? else {
        return Ok(IssueFilter::default());
    };
    Ok(IssueFilter {
        state: object_string(&object, "state")?,
        is_terminal: object_boolean(&object, "isTerminal")?,
        priority: object_integer(&object, "priority")?,
        label: object_string(&object, "label")?,
        project_id: object_integer(&object, "projectId")?,
    })
}
pub(super) fn issue_filter_sql(issue: &str, state: Option<&str>, filter: &IssueFilter) -> String {
    let mut sql = Vec::new();
    if let Some(value) = &filter.state {
        sql.push(format!("{issue}.state={}", quote(value)));
    }
    if let Some(value) = filter.is_terminal {
        sql.push(format!(
            "{}.is_terminal={}",
            state.expect("isTerminal filter requires state join"),
            value as i64
        ));
    }
    if let Some(value) = filter.priority {
        sql.push(format!("{issue}.priority={value}"));
    }
    if let Some(value) = &filter.label {
        sql.push(format!("EXISTS(SELECT 1 FROM issue_labels fx WHERE fx.repo_id={issue}.repo_id AND fx.issue_number={issue}.number AND fx.label_name={})", quote(value)));
    }
    if let Some(value) = filter.project_id {
        sql.push(format!("EXISTS(SELECT 1 FROM issue_projects fp WHERE fp.repo_id={issue}.repo_id AND fp.issue_number={issue}.number AND fp.project_id={value})"));
    }
    if sql.is_empty() {
        String::new()
    } else {
        format!("AND {}", sql.join(" AND "))
    }
}
pub(super) fn project_filter_sql(project: &str, filter: &ProjectFilter) -> String {
    let mut sql = Vec::new();
    if let Some(value) = filter.is_terminal {
        sql.push(format!("{project}.is_terminal={}", value as i64));
    }
    if let Some(value) = filter.priority {
        sql.push(format!("{project}.priority={value}"));
    }
    if let Some(value) = &filter.label {
        sql.push(format!("EXISTS(SELECT 1 FROM project_label_links fx WHERE fx.repo_id={project}.repo_id AND fx.project_id={project}.id AND fx.label_name={})", quote(value)));
    }
    if sql.is_empty() {
        String::new()
    } else {
        format!("AND {}", sql.join(" AND "))
    }
}

#[cfg(test)]
#[path = "sql_tests.rs"]
mod tests;
