use crate::domain::label::{Label, LabelGroup};
use anyhow::Result;
use sqlx::{Sqlite, SqlitePool, Transaction};
pub async fn insert_group(pool: &SqlitePool, repo: i64, name: &str, selection: &str) -> Result<()> {
    sqlx::query!(
        "INSERT INTO label_groups (repo_id, name, selection) VALUES (?, ?, ?)",
        repo,
        name,
        selection
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn group_exists(pool: &SqlitePool, repo: i64, name: &str) -> Result<bool> {
    Ok(sqlx::query_scalar!(
        "SELECT COUNT(*) FROM label_groups WHERE repo_id = ? AND name = ?",
        repo,
        name
    )
    .fetch_one(pool)
    .await?
        != 0)
}

pub async fn insert(pool: &SqlitePool, repo: i64, name: &str, group: Option<&str>) -> Result<()> {
    sqlx::query!(
        "INSERT INTO labels (repo_id, name, group_name) VALUES (?, ?, ?)",
        repo,
        name,
        group
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(pool: &SqlitePool, repo: i64) -> Result<Vec<Label>> {
    Ok(sqlx::query_as!(
        Label,
        r#"
        SELECT
            name AS "name!: String",
            group_name AS "group?: String"
        FROM
            labels
        WHERE
            repo_id = ?
        ORDER BY
            group_name, name
    "#,
        repo
    )
    .fetch_all(pool)
    .await?)
}

pub async fn list_groups(pool: &SqlitePool, repo: i64) -> Result<Vec<LabelGroup>> {
    Ok(sqlx::query_as!(
        LabelGroup,
        r#"
        SELECT
            name AS "name!: String",
            selection AS "selection!: String"
        FROM
            label_groups
        WHERE
            repo_id = ?
        ORDER BY
            name
    "#,
        repo
    )
    .fetch_all(pool)
    .await?)
}

pub async fn label_group_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    label: &str,
) -> Result<Option<Option<String>>> {
    Ok(sqlx::query_scalar!(
        r#"
            SELECT
                group_name AS "g?: String"
            FROM
                labels
            WHERE
                repo_id = ?
            AND
                name = ?
        "#,
        repo,
        label
    )
    .fetch_optional(&mut **tx)
    .await?)
}

pub async fn group_selection_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    group: &str,
) -> Result<String> {
    Ok(sqlx::query_scalar!(
        r#"
            SELECT
                selection AS "s!: String"
            FROM
                label_groups
            WHERE
                repo_id = ?
            AND
                name = ?
        "#,
        repo,
        group
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn replace_single_group(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    number: i64,
    group: &str,
) -> Result<()> {
    sqlx::query!("DELETE FROM issue_labels WHERE repo_id = ? AND issue_number = ? AND label_name IN (SELECT name FROM labels WHERE repo_id = ? AND group_name = ?)", repo, number, repo, group).execute(&mut **tx).await?;
    Ok(())
}

pub async fn attach(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    number: i64,
    label: &str,
) -> Result<()> {
    sqlx::query!(
        "INSERT OR IGNORE INTO issue_labels (repo_id, issue_number, label_name) VALUES (?, ?, ?)",
        repo,
        number,
        label
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn detach(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    number: i64,
    label: &str,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM issue_labels WHERE repo_id = ? AND issue_number = ? AND label_name = ?",
        repo,
        number,
        label
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn insert_project_group(
    pool: &SqlitePool,
    repo: i64,
    name: &str,
    selection: &str,
) -> Result<()> {
    sqlx::query!(
        "INSERT INTO project_label_groups (repo_id, name, selection) VALUES (?, ?, ?)",
        repo,
        name,
        selection
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn project_group_exists(pool: &SqlitePool, repo: i64, name: &str) -> Result<bool> {
    Ok(sqlx::query_scalar!(
        "SELECT COUNT(*) FROM project_label_groups WHERE repo_id = ? AND name = ?",
        repo,
        name
    )
    .fetch_one(pool)
    .await?
        != 0)
}

pub async fn insert_project_label(
    pool: &SqlitePool,
    repo: i64,
    name: &str,
    group: Option<&str>,
) -> Result<()> {
    sqlx::query!(
        "INSERT INTO project_labels (repo_id, name, group_name) VALUES (?, ?, ?)",
        repo,
        name,
        group
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_project_labels(pool: &SqlitePool, repo: i64) -> Result<Vec<Label>> {
    Ok(sqlx::query_as!(
        Label,
        r#"SELECT name AS "name!: String", group_name AS "group?: String"
           FROM project_labels WHERE repo_id = ? ORDER BY group_name, name"#,
        repo
    )
    .fetch_all(pool)
    .await?)
}

pub async fn list_project_groups(pool: &SqlitePool, repo: i64) -> Result<Vec<LabelGroup>> {
    Ok(sqlx::query_as!(
        LabelGroup,
        r#"SELECT name AS "name!: String", selection AS "selection!: String"
           FROM project_label_groups WHERE repo_id = ? ORDER BY name"#,
        repo
    )
    .fetch_all(pool)
    .await?)
}

pub async fn project_label_group_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    label: &str,
) -> Result<Option<Option<String>>> {
    Ok(sqlx::query_scalar!(
        r#"SELECT group_name AS "g?: String" FROM project_labels
           WHERE repo_id = ? AND name = ?"#,
        repo,
        label
    )
    .fetch_optional(&mut **tx)
    .await?)
}

pub async fn project_group_selection_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    group: &str,
) -> Result<String> {
    Ok(sqlx::query_scalar!(
        r#"SELECT selection AS "s!: String" FROM project_label_groups
           WHERE repo_id = ? AND name = ?"#,
        repo,
        group
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn replace_project_single_group(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    project: i64,
    group: &str,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM project_label_links WHERE repo_id = ? AND project_id = ? AND label_name IN (SELECT name FROM project_labels WHERE repo_id = ? AND group_name = ?)",
        repo,
        project,
        repo,
        group
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn attach_project(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    project: i64,
    label: &str,
) -> Result<()> {
    sqlx::query!(
        "INSERT OR IGNORE INTO project_label_links (repo_id, project_id, label_name) VALUES (?, ?, ?)",
        repo,
        project,
        label
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn detach_project(pool: &SqlitePool, repo: i64, project: i64, label: &str) -> Result<()> {
    sqlx::query!(
        "DELETE FROM project_label_links WHERE repo_id = ? AND project_id = ? AND label_name = ?",
        repo,
        project,
        label
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn labels_for_project(pool: &SqlitePool, repo: i64, project: i64) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar!(
        "SELECT label_name FROM project_label_links WHERE repo_id = ? AND project_id = ? ORDER BY label_name",
        repo,
        project
    )
    .fetch_all(pool)
    .await?)
}
