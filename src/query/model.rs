//! GraphQL input and output models backed by projected JSON.

use async_graphql::{Context, Enum, InputObject, Object};
use serde_json::{Map, Value as JsonValue};

/// `state` and `stateType` both accept one value or a list of them, and match
/// an issue carrying any of the listed values.
#[derive(InputObject, Default, Clone)]
pub(super) struct IssueFilter {
    pub(super) state: Option<Vec<String>>,
    pub(super) state_type: Option<Vec<String>>,
    pub(super) label: Option<String>,
    pub(super) project_id: Option<i64>,
}

#[derive(InputObject, Default, Clone)]
pub(super) struct ProjectFilter {
    pub(super) state: Option<Vec<String>>,
    pub(super) state_type: Option<Vec<String>>,
    pub(super) label: Option<String>,
}

#[derive(InputObject, Default, Clone)]
pub(super) struct PullRequestFilter {
    pub(super) state: Option<String>,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(super) enum LabelTarget {
    Issue,
    Project,
}

#[derive(Clone)]
pub(super) struct JsonObject(Map<String, JsonValue>);

impl JsonObject {
    pub(super) fn parse(raw: &str) -> async_graphql::Result<Self> {
        let value: JsonValue = serde_json::from_str(raw)
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Self::from_value(value)
    }

    fn from_value(value: JsonValue) -> async_graphql::Result<Self> {
        value
            .as_object()
            .cloned()
            .map(Self)
            .ok_or_else(|| "query projection did not return an object".into())
    }

    fn value<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a JsonValue> {
        let field = ctx.field();
        let key = field.alias().unwrap_or_else(|| field.name());
        self.0.get(key).ok_or_else(|| {
            format!("selected field {key:?} is missing from query projection").into()
        })
    }

    fn string<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.value(ctx)?
            .as_str()
            .ok_or_else(|| "projected value is not a string".into())
    }

    fn optional_string<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<Option<&'a str>> {
        let value = self.value(ctx)?;
        if value.is_null() {
            Ok(None)
        } else {
            value
                .as_str()
                .map(Some)
                .ok_or_else(|| "projected value is not a string".into())
        }
    }

    fn integer(&self, ctx: &Context<'_>) -> async_graphql::Result<i64> {
        self.value(ctx)?
            .as_i64()
            .ok_or_else(|| "projected value is not an integer".into())
    }

    fn boolean(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        self.value(ctx)?
            .as_bool()
            .ok_or_else(|| "projected value is not a boolean".into())
    }

    fn object<T>(
        &self,
        ctx: &Context<'_>,
        make: impl Fn(JsonObject) -> T,
    ) -> async_graphql::Result<Option<T>> {
        let value = self.value(ctx)?;
        if value.is_null() {
            Ok(None)
        } else {
            Ok(Some(make(Self::from_value(value.clone())?)))
        }
    }

    fn objects<T>(
        &self,
        ctx: &Context<'_>,
        make: impl Fn(JsonObject) -> T,
    ) -> async_graphql::Result<Vec<T>> {
        self.value(ctx)?
            .as_array()
            .ok_or_else(|| async_graphql::Error::new("projected value is not a list"))?
            .iter()
            .cloned()
            .map(|value| Ok(make(Self::from_value(value)?)))
            .collect()
    }
}

#[derive(Clone)]
pub(super) struct IssueObject(pub(super) JsonObject);

#[Object]
impl IssueObject {
    async fn number(&self, ctx: &Context<'_>) -> async_graphql::Result<i64> {
        self.0.integer(ctx)
    }
    async fn title<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn body<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn state<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    /// The type of the issue's configured state: open, in progress, or closed.
    async fn state_type<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn leased(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        self.0.boolean(ctx)
    }
    async fn created_at<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn updated_at<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }

    async fn project(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<ProjectObject>> {
        self.0.object(ctx, ProjectObject)
    }
    async fn milestone(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<MilestoneObject>> {
        self.0.object(ctx, MilestoneObject)
    }
    async fn labels(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<LabelObject>> {
        self.0.objects(ctx, LabelObject)
    }
    async fn blocks(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<IssueObject>> {
        self.0.objects(ctx, IssueObject)
    }
    async fn blocked_by(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<IssueObject>> {
        self.0.objects(ctx, IssueObject)
    }
    async fn related(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<IssueObject>> {
        self.0.objects(ctx, IssueObject)
    }
    async fn parent(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<IssueObject>> {
        self.0.object(ctx, IssueObject)
    }
    async fn sub_issues(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<IssueObject>> {
        self.0.objects(ctx, IssueObject)
    }
    async fn pull_requests(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<PullRequestObject>> {
        self.0.objects(ctx, PullRequestObject)
    }
}

#[derive(Clone)]
pub(super) struct ProjectObject(pub(super) JsonObject);

#[Object]
impl ProjectObject {
    async fn id(&self, ctx: &Context<'_>) -> async_graphql::Result<i64> {
        self.0.integer(ctx)
    }
    async fn name<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn summary<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn description<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn state<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    /// The type of the project's configured state: open or closed.
    async fn state_type<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn created_at<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn updated_at<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn issues(
        &self,
        ctx: &Context<'_>,
        _filter: Option<IssueFilter>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<IssueObject>> {
        self.0.objects(ctx, IssueObject)
    }
    async fn milestones(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<MilestoneObject>> {
        self.0.objects(ctx, MilestoneObject)
    }
    async fn labels(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<LabelObject>> {
        self.0.objects(ctx, LabelObject)
    }
}

#[derive(Clone)]
pub(super) struct MilestoneObject(pub(super) JsonObject);

#[Object]
impl MilestoneObject {
    async fn id(&self, ctx: &Context<'_>) -> async_graphql::Result<i64> {
        self.0.integer(ctx)
    }
    async fn position(&self, ctx: &Context<'_>) -> async_graphql::Result<i64> {
        self.0.integer(ctx)
    }
    async fn name<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn description<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn status<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn created_at<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn updated_at<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn start_date<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<Option<&'a str>> {
        self.0.optional_string(ctx)
    }
    async fn target_date<'a>(
        &'a self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<&'a str>> {
        self.0.optional_string(ctx)
    }
    async fn project(&self, ctx: &Context<'_>) -> async_graphql::Result<ProjectObject> {
        self.0
            .object(ctx, ProjectObject)?
            .ok_or_else(|| "milestone project not found".into())
    }
    async fn issues(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<IssueObject>> {
        self.0.objects(ctx, IssueObject)
    }
}

#[derive(Clone)]
pub(super) struct PullRequestObject(pub(super) JsonObject);

#[Object]
impl PullRequestObject {
    async fn number(&self, ctx: &Context<'_>) -> async_graphql::Result<i64> {
        self.0.integer(ctx)
    }
    async fn title<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn body<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn branch<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn state<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn created_at<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn updated_at<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn issues(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<IssueObject>> {
        self.0.objects(ctx, IssueObject)
    }
}

#[derive(Clone)]
pub(super) struct WikiPageObject(pub(super) JsonObject);

#[Object]
impl WikiPageObject {
    async fn slug<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn title<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn body<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn created_at<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn updated_at<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn links_to(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<WikiPageObject>> {
        self.0.objects(ctx, WikiPageObject)
    }
    async fn backlinks(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<WikiPageObject>> {
        self.0.objects(ctx, WikiPageObject)
    }
}

/// A repository octa has recorded.
///
/// Deliberately scalar-only. Every other root here is scoped to one active
/// repository, and `repos` is the one cross-repository root; letting it descend
/// into Issues or Projects would make that scope premise unstatable.
#[derive(Clone)]
pub(super) struct RepoObject(pub(super) JsonObject);

#[Object]
impl RepoObject {
    async fn name<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    /// The repository's Git common directory, which is how octa identifies it.
    async fn path<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn created_at<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    /// Issues whose state carries the open type.
    async fn open_issues(&self, ctx: &Context<'_>) -> async_graphql::Result<i64> {
        self.0.integer(ctx)
    }
    /// Issues whose state carries the in progress type.
    async fn in_progress_issues(&self, ctx: &Context<'_>) -> async_graphql::Result<i64> {
        self.0.integer(ctx)
    }
}

#[derive(Clone)]
pub(super) struct LabelObject(pub(super) JsonObject);

#[Object]
impl LabelObject {
    async fn name<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<&'a str> {
        self.0.string(ctx)
    }
    async fn group<'a>(&'a self, ctx: &Context<'_>) -> async_graphql::Result<Option<&'a str>> {
        self.0.optional_string(ctx)
    }
    async fn target(&self, ctx: &Context<'_>) -> async_graphql::Result<LabelTarget> {
        match self.0.string(ctx)? {
            "ISSUE" => Ok(LabelTarget::Issue),
            "PROJECT" => Ok(LabelTarget::Project),
            _ => Err("invalid projected label target".into()),
        }
    }
    async fn issues(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<IssueObject>> {
        self.0.objects(ctx, IssueObject)
    }
    async fn projects(
        &self,
        ctx: &Context<'_>,
        _offset: Option<i64>,
        _limit: Option<i64>,
    ) -> async_graphql::Result<Vec<ProjectObject>> {
        self.0.objects(ctx, ProjectObject)
    }
}
