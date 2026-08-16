//! Root GraphQL fields and database execution boundary.

use super::model::*;
use super::sql::{issue_filter_sql, issue_state_join, project_filter_sql, quote, Page, Planner};
use async_graphql::{Context, Object};
#[cfg(test)]
use std::sync::Mutex;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Clone)]
pub(super) struct QueryDb {
    pub(super) pool: sqlx::SqlitePool,
    pub(super) repo: i64,
    pub(super) accesses: Arc<AtomicUsize>,
    #[cfg(test)]
    pub(super) statements: Arc<Mutex<Vec<String>>>,
}

impl QueryDb {
    async fn rows<T>(
        &self,
        sql: &str,
        make: impl Fn(JsonObject) -> T,
    ) -> async_graphql::Result<Vec<T>> {
        self.accesses.fetch_add(1, Ordering::Relaxed);
        #[cfg(test)]
        self.statements.lock().unwrap().push(sql.to_string());
        let rows = sqlx::query_scalar::<_, String>(sql)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| Ok(make(JsonObject::parse(&row)?)))
            .collect()
    }

    async fn optional<T>(
        &self,
        sql: &str,
        make: impl Fn(JsonObject) -> T,
    ) -> async_graphql::Result<Option<T>> {
        Ok(self.rows(sql, make).await?.into_iter().next())
    }
}

pub(super) struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn issue(
        &self,
        ctx: &Context<'_>,
        number: i64,
    ) -> async_graphql::Result<Option<IssueObject>> {
        let db = ctx.data::<QueryDb>()?;
        let fields = ctx.field().selection_set().collect::<Vec<_>>();
        let state = issue_state_join(&fields, "i", "s", false);
        let projection = Planner::new(db.repo).issue(&fields, "i", state.alias())?;
        let sql = format!(
            "SELECT {projection} FROM issues i {} WHERE i.repo_id={} AND i.number={number}",
            state.sql(),
            db.repo
        );
        db.optional(&sql, IssueObject).await
    }

    async fn issues(
        &self,
        ctx: &Context<'_>,
        filter: Option<IssueFilter>,
        offset: Option<i64>,
        limit: Option<i64>,
    ) -> async_graphql::Result<Vec<IssueObject>> {
        let db = ctx.data::<QueryDb>()?;
        let fields = ctx.field().selection_set().collect::<Vec<_>>();
        let page = Page::new(offset, limit)?;
        let filter = filter.unwrap_or_default();
        let state = issue_state_join(&fields, "i", "s", filter.is_terminal.is_some());
        let projection = Planner::new(db.repo).issue(&fields, "i", state.alias())?;
        let filter = issue_filter_sql("i", state.alias(), &filter);
        let sql = format!(
            "SELECT {projection} FROM issues i {} WHERE i.repo_id={} {filter} ORDER BY i.number {}",
            state.sql(),
            db.repo,
            page.sql()
        );
        db.rows(&sql, IssueObject).await
    }

    async fn project(
        &self,
        ctx: &Context<'_>,
        id: Option<i64>,
        name: Option<String>,
    ) -> async_graphql::Result<Option<ProjectObject>> {
        let db = ctx.data::<QueryDb>()?;
        let predicate = match (id, name) {
            (Some(id), None) => format!("p.id={id}"),
            (None, Some(name)) => format!("p.name={} COLLATE NOCASE", quote(&name)),
            _ => return Err("provide exactly one of id or name".into()),
        };
        let projection =
            Planner::new(db.repo).project(&ctx.field().selection_set().collect::<Vec<_>>(), "p")?;
        let sql = format!(
            "SELECT {projection} FROM projects p WHERE p.repo_id={} AND {predicate}",
            db.repo
        );
        db.optional(&sql, ProjectObject).await
    }

    async fn projects(
        &self,
        ctx: &Context<'_>,
        filter: Option<ProjectFilter>,
        offset: Option<i64>,
        limit: Option<i64>,
    ) -> async_graphql::Result<Vec<ProjectObject>> {
        let db = ctx.data::<QueryDb>()?;
        let projection =
            Planner::new(db.repo).project(&ctx.field().selection_set().collect::<Vec<_>>(), "p")?;
        let filter = project_filter_sql("p", &filter.unwrap_or_default());
        let page = Page::new(offset, limit)?;
        let sql = format!(
            "SELECT {projection} FROM projects p WHERE p.repo_id={} {filter} ORDER BY p.id {}",
            db.repo,
            page.sql()
        );
        db.rows(&sql, ProjectObject).await
    }

    async fn milestone(
        &self,
        ctx: &Context<'_>,
        project_id: i64,
        id: i64,
    ) -> async_graphql::Result<Option<MilestoneObject>> {
        let db = ctx.data::<QueryDb>()?;
        let projection = Planner::new(db.repo)
            .milestone(&ctx.field().selection_set().collect::<Vec<_>>(), "m")?;
        let sql = format!("SELECT {projection} FROM project_milestones m WHERE m.repo_id={} AND m.project_id={project_id} AND m.id={id}", db.repo);
        db.optional(&sql, MilestoneObject).await
    }

    async fn milestones(
        &self,
        ctx: &Context<'_>,
        project_id: i64,
        offset: Option<i64>,
        limit: Option<i64>,
    ) -> async_graphql::Result<Vec<MilestoneObject>> {
        let db = ctx.data::<QueryDb>()?;
        let projection = Planner::new(db.repo)
            .milestone(&ctx.field().selection_set().collect::<Vec<_>>(), "m")?;
        let page = Page::new(offset, limit)?;
        let sql = format!("SELECT {projection} FROM project_milestones m WHERE m.repo_id={} AND m.project_id={project_id} ORDER BY m.position,m.id {}", db.repo, page.sql());
        db.rows(&sql, MilestoneObject).await
    }

    async fn pull_request(
        &self,
        ctx: &Context<'_>,
        number: i64,
    ) -> async_graphql::Result<Option<PullRequestObject>> {
        let db = ctx.data::<QueryDb>()?;
        let projection = Planner::new(db.repo)
            .pull_request(&ctx.field().selection_set().collect::<Vec<_>>(), "p")?;
        let sql = format!(
            "SELECT {projection} FROM prs p WHERE p.repo_id={} AND p.number={number}",
            db.repo
        );
        db.optional(&sql, PullRequestObject).await
    }

    async fn pull_requests(
        &self,
        ctx: &Context<'_>,
        filter: Option<PullRequestFilter>,
        offset: Option<i64>,
        limit: Option<i64>,
    ) -> async_graphql::Result<Vec<PullRequestObject>> {
        let db = ctx.data::<QueryDb>()?;
        let projection = Planner::new(db.repo)
            .pull_request(&ctx.field().selection_set().collect::<Vec<_>>(), "p")?;
        let state = filter
            .and_then(|filter| filter.state)
            .map(|state| format!("AND p.state={}", quote(&state)))
            .unwrap_or_default();
        let page = Page::new(offset, limit)?;
        let sql = format!(
            "SELECT {projection} FROM prs p WHERE p.repo_id={} {state} ORDER BY p.number {}",
            db.repo,
            page.sql()
        );
        db.rows(&sql, PullRequestObject).await
    }

    async fn wiki_page(
        &self,
        ctx: &Context<'_>,
        slug: String,
    ) -> async_graphql::Result<Option<WikiPageObject>> {
        let db = ctx.data::<QueryDb>()?;
        let projection =
            Planner::new(db.repo).wiki(&ctx.field().selection_set().collect::<Vec<_>>(), "w")?;
        let sql = format!(
            "SELECT {projection} FROM wiki_pages w WHERE w.repo_id={} AND w.slug={}",
            db.repo,
            quote(&slug)
        );
        db.optional(&sql, WikiPageObject).await
    }

    async fn wiki_pages(
        &self,
        ctx: &Context<'_>,
        offset: Option<i64>,
        limit: Option<i64>,
    ) -> async_graphql::Result<Vec<WikiPageObject>> {
        let db = ctx.data::<QueryDb>()?;
        let projection =
            Planner::new(db.repo).wiki(&ctx.field().selection_set().collect::<Vec<_>>(), "w")?;
        let page = Page::new(offset, limit)?;
        let sql = format!(
            "SELECT {projection} FROM wiki_pages w WHERE w.repo_id={} ORDER BY w.slug {}",
            db.repo,
            page.sql()
        );
        db.rows(&sql, WikiPageObject).await
    }

    async fn labels(
        &self,
        ctx: &Context<'_>,
        target: LabelTarget,
        offset: Option<i64>,
        limit: Option<i64>,
    ) -> async_graphql::Result<Vec<LabelObject>> {
        let db = ctx.data::<QueryDb>()?;
        let (table, target_name) = match target {
            LabelTarget::Issue => ("labels", "ISSUE"),
            LabelTarget::Project => ("project_labels", "PROJECT"),
        };
        let projection = Planner::new(db.repo).label(
            &ctx.field().selection_set().collect::<Vec<_>>(),
            "l",
            target,
        )?;
        let page = Page::new(offset, limit)?;
        let sql = format!(
            "SELECT {projection} FROM {table} l ORDER BY l.name {}",
            page.sql()
        )
        .replace("$TARGET", target_name);
        db.rows(&sql, LabelObject).await
    }
}
