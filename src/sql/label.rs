use crate::domain::label::{Label, LabelGroup};
use anyhow::Result;
use sqlx::{Sqlite, SqlitePool, Transaction};
pub async fn insert_group(pool: &SqlitePool, name: &str, selection: &str) -> Result<()> {
    sqlx::query!(
        "INSERT INTO label_groups (name, selection) VALUES (?, ?)",
        name,
        selection
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn group_exists(pool: &SqlitePool, name: &str) -> Result<bool> {
    Ok(
        sqlx::query_scalar!("SELECT COUNT(*) FROM label_groups WHERE name = ?", name)
            .fetch_one(pool)
            .await?
            != 0,
    )
}

pub async fn insert(pool: &SqlitePool, name: &str, group: Option<&str>) -> Result<()> {
    sqlx::query!(
        "INSERT INTO labels (name, group_name) VALUES (?, ?)",
        name,
        group
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Label>> {
    Ok(sqlx::query_as!(
        Label,
        r#"
        SELECT
            name AS "name!: String",
            group_name AS "group?: String"
        FROM
            labels
        ORDER BY
            group_name, name
    "#
    )
    .fetch_all(pool)
    .await?)
}

pub async fn list_groups(pool: &SqlitePool) -> Result<Vec<LabelGroup>> {
    Ok(sqlx::query_as!(
        LabelGroup,
        r#"
        SELECT
            name AS "name!: String",
            selection AS "selection!: String"
        FROM
            label_groups
        ORDER BY
            name
    "#
    )
    .fetch_all(pool)
    .await?)
}

pub async fn label_group_tx(
    tx: &mut Transaction<'_, Sqlite>,
    label: &str,
) -> Result<Option<Option<String>>> {
    Ok(sqlx::query_scalar!(
        r#"
            SELECT
                group_name AS "g?: String"
            FROM
                labels
            WHERE
                name = ?
        "#,
        label
    )
    .fetch_optional(&mut **tx)
    .await?)
}

pub async fn group_selection_tx(tx: &mut Transaction<'_, Sqlite>, group: &str) -> Result<String> {
    Ok(sqlx::query_scalar!(
        r#"
            SELECT
                selection AS "s!: String"
            FROM
                label_groups
            WHERE
                name = ?
        "#,
        group
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn replace_single_group(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    number: i64,
    group: &str,
) -> Result<()> {
    sqlx::query!("DELETE FROM issue_labels WHERE repository_id = ? AND issue_number = ? AND label_name IN (SELECT name FROM labels WHERE group_name = ?)", repository, number, group).execute(&mut **tx).await?;
    Ok(())
}

pub async fn attach(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    number: i64,
    label: &str,
) -> Result<()> {
    sqlx::query!(
        "INSERT OR IGNORE INTO issue_labels (repository_id, issue_number, label_name) VALUES (?, ?, ?)",
        repository,
        number,
        label
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn detach(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    number: i64,
    label: &str,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM issue_labels WHERE repository_id = ? AND issue_number = ? AND label_name = ?",
        repository,
        number,
        label
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn insert_project_group(pool: &SqlitePool, name: &str, selection: &str) -> Result<()> {
    sqlx::query!(
        "INSERT INTO project_label_groups (name, selection) VALUES (?, ?)",
        name,
        selection
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn project_group_exists(pool: &SqlitePool, name: &str) -> Result<bool> {
    Ok(sqlx::query_scalar!(
        "SELECT COUNT(*) FROM project_label_groups WHERE name = ?",
        name
    )
    .fetch_one(pool)
    .await?
        != 0)
}

pub async fn insert_project_label(
    pool: &SqlitePool,
    name: &str,
    group: Option<&str>,
) -> Result<()> {
    sqlx::query!(
        "INSERT INTO project_labels (name, group_name) VALUES (?, ?)",
        name,
        group
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_project_labels(pool: &SqlitePool) -> Result<Vec<Label>> {
    Ok(sqlx::query_as!(
        Label,
        r#"SELECT name AS "name!: String", group_name AS "group?: String"
           FROM project_labels ORDER BY group_name, name"#
    )
    .fetch_all(pool)
    .await?)
}

pub async fn list_project_groups(pool: &SqlitePool) -> Result<Vec<LabelGroup>> {
    Ok(sqlx::query_as!(
        LabelGroup,
        r#"SELECT name AS "name!: String", selection AS "selection!: String"
           FROM project_label_groups ORDER BY name"#
    )
    .fetch_all(pool)
    .await?)
}

pub async fn project_label_group_tx(
    tx: &mut Transaction<'_, Sqlite>,
    label: &str,
) -> Result<Option<Option<String>>> {
    Ok(sqlx::query_scalar!(
        r#"SELECT group_name AS "g?: String" FROM project_labels
           WHERE name = ?"#,
        label
    )
    .fetch_optional(&mut **tx)
    .await?)
}

pub async fn project_group_selection_tx(
    tx: &mut Transaction<'_, Sqlite>,
    group: &str,
) -> Result<String> {
    Ok(sqlx::query_scalar!(
        r#"SELECT selection AS "s!: String" FROM project_label_groups
           WHERE name = ?"#,
        group
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn replace_project_single_group(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    project: i64,
    group: &str,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM project_label_links WHERE repository_id = ? AND project_id = ? AND label_name IN (SELECT name FROM project_labels WHERE group_name = ?)",
        repository,
        project,
        group
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn attach_project(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    project: i64,
    label: &str,
) -> Result<()> {
    sqlx::query!(
        "INSERT OR IGNORE INTO project_label_links (repository_id, project_id, label_name) VALUES (?, ?, ?)",
        repository,
        project,
        label
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn detach_project(
    pool: &SqlitePool,
    repository: i64,
    project: i64,
    label: &str,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM project_label_links WHERE repository_id = ? AND project_id = ? AND label_name = ?",
        repository,
        project,
        label
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn labels_for_project(
    pool: &SqlitePool,
    repository: i64,
    project: i64,
) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar!(
        "SELECT label_name FROM project_label_links WHERE repository_id = ? AND project_id = ? ORDER BY label_name",
        repository,
        project
    )
    .fetch_all(pool)
    .await?)
}
