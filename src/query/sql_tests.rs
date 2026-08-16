//! SQL compiler tests grouped by behavior.

use super::*;
use crate::query::root::{QueryDb, QueryRoot};
use async_graphql::{EmptyMutation, EmptySubscription, Schema};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::{atomic::AtomicUsize, Arc, Mutex};

async fn executed_sql(document: &str) -> String {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../../migrations/0001_init.sql"))
        .execute(&pool)
        .await
        .unwrap();

    let statements = Arc::new(Mutex::new(Vec::new()));
    let db = QueryDb {
        pool,
        repo: 1,
        accesses: Arc::new(AtomicUsize::new(0)),
        statements: statements.clone(),
    };
    let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .data(db)
        .finish();
    let response = schema.execute(document).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);

    let executed = statements.lock().unwrap();
    assert_eq!(executed.len(), 1, "expected one top-level SQL statement");
    executed[0].clone()
}

fn assert_contains_in_order(sql: &str, fragments: &[&str]) {
    let mut remainder = sql;
    for fragment in fragments {
        let position = remainder
            .find(fragment)
            .unwrap_or_else(|| panic!("missing SQL fragment {fragment:?} in:\n{sql}"));
        remainder = &remainder[position + fragment.len()..];
    }
}

mod projection {
    use super::*;
    #[tokio::test]
    async fn scalar_selection_projects_only_requested_issue_columns() {
        let sql = executed_sql("{ issue(number: 7) { number title } }").await;

        assert!(sql.contains("'$.number',json(json_quote(i.number))"));
        assert!(sql.contains("'$.title',json(json_quote(i.title))"));
        assert!(sql.ends_with("FROM issues i  WHERE i.repo_id=1 AND i.number=7"));
        assert!(!sql.contains(" JOIN "));
        for unselected in ["i.body", "i.created_at", "i.updated_at"] {
            assert!(
                !sql.contains(unselected),
                "unexpected {unselected} in:\n{sql}"
            );
        }
    }

    #[tokio::test]
    async fn is_terminal_selection_adds_the_state_join() {
        let sql = executed_sql("{ issue(number: 1) { number isTerminal } }").await;

        assert_contains_in_order(
            &sql,
            &[
                "'$.isTerminal',json(COALESCE(json(CASE WHEN s.is_terminal=1 THEN 'true' ELSE 'false' END),'null'))",
                "FROM issues i JOIN issue_states s",
                "s.name=i.state",
            ],
        );
        assert!(!sql.contains("issue_projects"));
        assert!(!sql.contains("issue_labels"));
    }

    #[tokio::test]
    async fn leased_selection_projects_an_issue_scoped_exists() {
        let sql = executed_sql("{ issue(number: 1) { number leased } }").await;

        assert_contains_in_order(
            &sql,
            &[
                "EXISTS(SELECT 1 FROM issue_leases lease1",
                "lease1.repo_id=i.repo_id",
                "lease1.issue_number=i.number",
            ],
        );
        assert!(!sql.contains(" JOIN "));
    }

    #[tokio::test]
    async fn aliases_and_fragments_merge_one_relation_projection() {
        let sql = executed_sql(
            r#"
                query {
                    issue(number: 1) {
                        selectedProject: project { id }
                        ...ProjectName
                    }
                }
                fragment ProjectName on IssueObject {
                    selectedProject: project { name }
                }
            "#,
        )
        .await;

        assert_eq!(sql.matches("FROM issue_projects").count(), 1);
        assert_eq!(sql.matches("'$.selectedProject'").count(), 1);
        assert!(sql.contains("'$.id',json(json_quote(p1.id))"));
        assert!(sql.contains("'$.name',json(json_quote(p1.name))"));
    }

    #[test]
    fn json_projection_distinguishes_sql_text_from_encoded_relations() {
        let scalar = json_object(vec![json_pair("title", "i.title")]);
        let relation = json_object(vec![json_pair(
            "project",
            "(SELECT json('{}') FROM projects p)",
        )]);

        assert_eq!(
            scalar,
            "json_set(json('{}'),'$.title',json(json_quote(i.title)))"
        );
        assert_eq!(
            relation,
            "json_set(json('{}'),'$.project',json(COALESCE((SELECT json('{}') FROM projects p),'null')))"
        );
    }
}

mod issue_relations {
    use super::*;
    #[tokio::test]
    async fn singular_relation_compiles_its_join_and_parent_correlation() {
        let sql = executed_sql("{ issue(number: 1) { project { id name } } }").await;

        assert_contains_in_order(
            &sql,
            &[
                "'$.id',json(json_quote(p1.id))",
                "'$.name',json(json_quote(p1.name))",
                "FROM issue_projects ip2 JOIN projects p1",
                "p1.repo_id=ip2.repo_id AND p1.id=ip2.project_id",
                "WHERE ip2.repo_id=i.repo_id AND ip2.issue_number=i.number",
            ],
        );
        assert_eq!(sql.matches("JOIN projects").count(), 1);
        assert!(!sql.contains("project_label_links"));
    }

    #[tokio::test]
    async fn issue_milestone_compiles_composite_join_and_parent_correlation() {
        let sql = executed_sql("{ issue(number: 1) { milestone { id project { id } } } }").await;

        assert_contains_in_order(
            &sql,
            &[
                "FROM projects p3",
                "p3.repo_id=m1.repo_id AND p3.id=m1.project_id",
                "FROM issue_milestones im2 JOIN project_milestones m1",
                "m1.repo_id=im2.repo_id AND m1.project_id=im2.project_id AND m1.id=im2.milestone_id",
                "WHERE im2.repo_id=i.repo_id AND im2.issue_number=i.number",
            ],
        );
    }

    #[tokio::test]
    async fn collection_relation_compiles_order_and_page_inside_its_subquery() {
        let sql =
            executed_sql("{ issue(number: 1) { labels(limit: 8, offset: 3) { name } } }").await;

        assert_contains_in_order(
            &sql,
            &[
                "FROM issue_labels il2 JOIN labels l1",
                "l1.name=il2.label_name",
                "WHERE il2.repo_id=i.repo_id AND il2.issue_number=i.number",
                "ORDER BY l1.name LIMIT 8 OFFSET 3",
            ],
        );
        assert!(sql.contains("json_group_array(json(item))"));
        assert!(!sql.contains("issue_projects"));
    }

    #[tokio::test]
    async fn dependency_relations_compile_the_correct_direction() {
        let sql = executed_sql(
            "{ issue(number: 1) { blocks(limit: 2) { number } blockedBy(limit: 3) { number } } }",
        )
        .await;

        assert_contains_in_order(
            &sql,
            &[
                "FROM issue_deps d3 JOIN issues i1",
                "i1.number=d3.blocked_number",
                "WHERE d3.repo_id=i.repo_id AND d3.blocker_number=i.number",
                "ORDER BY i1.number LIMIT 2 OFFSET 0",
                "FROM issue_deps d6 JOIN issues i4",
                "i4.number=d6.blocker_number",
                "WHERE d6.repo_id=i.repo_id AND d6.blocked_number=i.number",
                "ORDER BY i4.number LIMIT 3 OFFSET 0",
            ],
        );
    }

    #[tokio::test]
    async fn symmetric_and_parent_relations_compile_their_distinct_correlations() {
        let related = executed_sql("{ issue(number: 1) { related { number } } }").await;
        assert_contains_in_order(
            &related,
            &[
                "FROM issue_relations r3 JOIN issues i1",
                "i1.number=CASE WHEN r3.low_number=i.number THEN r3.high_number ELSE r3.low_number END",
                "WHERE r3.repo_id=i.repo_id AND (r3.low_number=i.number OR r3.high_number=i.number)",
            ],
        );

        let hierarchy =
            executed_sql("{ issue(number: 1) { parent { number } subIssues { number } } }").await;
        assert_contains_in_order(
            &hierarchy,
            &[
                "FROM issue_parents par3 JOIN issues i1",
                "i1.number=par3.parent_number",
                "WHERE par3.repo_id=i.repo_id AND par3.child_number=i.number",
                "FROM issue_parents par6 JOIN issues i4",
                "i4.number=par6.child_number",
                "WHERE par6.repo_id=i.repo_id AND par6.parent_number=i.number",
            ],
        );
    }
}

mod other_relations {
    use super::*;
    #[tokio::test]
    async fn pull_request_links_compile_both_traversal_directions() {
        let from_issue =
            executed_sql("{ issue(number: 1) { pullRequests(limit: 2) { number } } }").await;
        assert_contains_in_order(
            &from_issue,
            &[
                "FROM issue_pr_links ipl2 JOIN prs pr1",
                "pr1.repo_id=ipl2.repo_id AND pr1.number=ipl2.pr_number",
                "WHERE ipl2.repo_id=i.repo_id AND ipl2.issue_number=i.number",
                "ORDER BY pr1.number LIMIT 2 OFFSET 0",
            ],
        );

        let from_pr =
            executed_sql("{ pullRequest(number: 9) { issues(limit: 4) { number } } }").await;
        assert_contains_in_order(
            &from_pr,
            &[
                "FROM issue_pr_links ipl3 JOIN issues i1",
                "i1.repo_id=ipl3.repo_id AND i1.number=ipl3.issue_number",
                "WHERE ipl3.repo_id=p.repo_id AND ipl3.pr_number=p.number",
                "ORDER BY i1.number LIMIT 4 OFFSET 0",
                "FROM prs p",
            ],
        );
    }

    #[tokio::test]
    async fn wiki_links_compile_forward_and_reverse_directions() {
        let sql = executed_sql(
            "{ wikiPage(slug: \"home\") { linksTo(limit: 2) { slug } backlinks(limit: 3) { slug } } }",
        )
        .await;

        assert_contains_in_order(
            &sql,
            &[
                "FROM wiki_links wl2 JOIN wiki_pages w1",
                "w1.slug=wl2.to_slug",
                "WHERE wl2.repo_id=w.repo_id AND wl2.from_slug=w.slug",
                "ORDER BY w1.slug LIMIT 2 OFFSET 0",
                "FROM wiki_links wl4 JOIN wiki_pages w3",
                "w3.slug=wl4.from_slug",
                "WHERE wl4.repo_id=w.repo_id AND wl4.to_slug=w.slug",
                "ORDER BY w3.slug LIMIT 3 OFFSET 0",
            ],
        );
    }

    #[tokio::test]
    async fn label_roots_compile_target_specific_reverse_relations() {
        let cases = [
            (
                "{ labels(target: ISSUE) { issues(limit: 2) { number } } }",
                [
                    "FROM issue_labels il3 JOIN issues i1",
                    // Labels are global, so the reverse relation is scoped by
                    // the active repository rather than by the label row.
                    "WHERE il3.repo_id=1 AND il3.label_name=l.name",
                    "ORDER BY i1.number LIMIT 2 OFFSET 0",
                    "FROM labels l ORDER BY l.name",
                ],
            ),
            (
                "{ labels(target: PROJECT) { projects(limit: 3) { id } } }",
                [
                    "FROM project_label_links pl2 JOIN projects p1",
                    "WHERE pl2.repo_id=1 AND pl2.label_name=l.name",
                    "ORDER BY p1.id LIMIT 3 OFFSET 0",
                    "FROM project_labels l ORDER BY l.name",
                ],
            ),
        ];

        for (document, fragments) in cases {
            let sql = executed_sql(document).await;
            assert_contains_in_order(&sql, &fragments);
        }
    }

    #[tokio::test]
    async fn label_relations_for_the_other_target_compile_to_empty_lists() {
        let issue_labels = executed_sql("{ labels(target: ISSUE) { projects { id } } }").await;
        assert!(issue_labels.contains("'$.projects',json(COALESCE(json('[]'),'null'))"));
        assert!(!issue_labels.contains("project_label_links"));
        assert!(!issue_labels.contains("JOIN projects"));

        let project_labels =
            executed_sql("{ labels(target: PROJECT) { issues { number } } }").await;
        assert!(project_labels.contains("'$.issues',json(COALESCE(json('[]'),'null'))"));
        assert!(!project_labels.contains("issue_labels"));
        assert!(!project_labels.contains("JOIN issues"));
    }
}

mod filters_and_roots {
    use super::*;
    #[tokio::test]
    async fn root_filters_compile_to_scoped_exists_without_projection_joins() {
        let sql = executed_sql(
            r#"{
                issues(
                    filter: { label: "reader's-choice", projectId: 7 }
                    limit: 12
                    offset: 4
                ) { number }
            }"#,
        )
        .await;

        assert_contains_in_order(
            &sql,
            &[
                "FROM issues i",
                "EXISTS(SELECT 1 FROM issue_labels fx",
                "fx.repo_id=i.repo_id AND fx.issue_number=i.number",
                "fx.label_name='reader''s-choice'",
                "EXISTS(SELECT 1 FROM issue_projects fp",
                "fp.repo_id=i.repo_id AND fp.issue_number=i.number AND fp.project_id=7",
                "ORDER BY i.number LIMIT 12 OFFSET 4",
            ],
        );
        assert!(!sql.contains(" JOIN "));
    }

    #[tokio::test]
    async fn issue_and_project_filters_compile_each_predicate_family() {
        let issues = executed_sql(
            r#"{
                issues(filter: {
                    state: "active"
                    isTerminal: false
                    label: "backend"
                    projectId: 7
                }) { number }
            }"#,
        )
        .await;
        for predicate in [
            "i.state='active'",
            "s.is_terminal=0",
            "fx.label_name='backend'",
            "fp.project_id=7",
        ] {
            assert!(
                issues.contains(predicate),
                "missing {predicate} in:\n{issues}"
            );
        }
        assert!(issues.contains("FROM issues i JOIN issue_states s"));

        let projects =
            executed_sql("{ projects(filter: { isTerminal: false, label: \"now\" }) { id } }")
                .await;
        for predicate in ["p.is_terminal=0", "fx.label_name='now'"] {
            assert!(
                projects.contains(predicate),
                "missing {predicate} in:\n{projects}"
            );
        }
        assert!(projects.contains("EXISTS(SELECT 1 FROM project_label_links fx"));
    }

    #[tokio::test]
    async fn entity_roots_compile_scope_filter_order_and_page() {
        let cases = [
            (
                "{ milestone(projectId: 4, id: 2) { id } }",
                "FROM project_milestones m WHERE m.repo_id=1 AND m.project_id=4 AND m.id=2",
            ),
            (
                "{ project(name: \"reader's roadmap\") { id } }",
                "FROM projects p WHERE p.repo_id=1 AND p.name='reader''s roadmap' COLLATE NOCASE",
            ),
            (
                "{ milestones(projectId: 4, offset: 2, limit: 7) { id } }",
                "FROM project_milestones m WHERE m.repo_id=1 AND m.project_id=4 ORDER BY m.position,m.id LIMIT 7 OFFSET 2",
            ),
            (
                "{ pullRequest(number: 8) { number } }",
                "FROM prs p WHERE p.repo_id=1 AND p.number=8",
            ),
            (
                "{ pullRequests(filter: { state: \"open\" }, offset: 1, limit: 6) { number } }",
                "FROM prs p WHERE p.repo_id=1 AND p.state='open' ORDER BY p.number LIMIT 6 OFFSET 1",
            ),
            (
                "{ wikiPage(slug: \"reader's-guide\") { slug } }",
                "FROM wiki_pages w WHERE w.repo_id=1 AND w.slug='reader''s-guide'",
            ),
            (
                "{ wikiPages(offset: 3, limit: 9) { slug } }",
                "FROM wiki_pages w WHERE w.repo_id=1 ORDER BY w.slug LIMIT 9 OFFSET 3",
            ),
        ];

        for (document, expected) in cases {
            let sql = executed_sql(document).await;
            assert!(sql.contains(expected), "missing {expected:?} in:\n{sql}");
        }
    }
}

mod multi_hop {
    use super::*;
    #[tokio::test]
    async fn issue_project_labels_compiles_nested_relations_into_one_statement() {
        let sql = executed_sql(
            r#"{
                issue(number: 1) {
                    number
                    project {
                        id
                        labels(limit: 10, offset: 2) { name group }
                    }
                }
            }"#,
        )
        .await;

        assert_contains_in_order(
            &sql,
            &[
                "FROM project_label_links pl4 JOIN project_labels l3",
                "WHERE pl4.repo_id=p1.repo_id AND pl4.project_id=p1.id",
                "ORDER BY l3.name LIMIT 10 OFFSET 2",
                "FROM issue_projects ip2 JOIN projects p1",
                "WHERE ip2.repo_id=i.repo_id AND ip2.issue_number=i.number",
                "FROM issues i",
            ],
        );
        assert!(!sql.contains("issue_labels"));
        assert!(!sql.contains("issue_states"));
        assert!(!sql.contains("project_milestones"));
    }

    #[tokio::test]
    async fn project_issues_labels_keeps_each_collection_correlated_and_paginated() {
        let sql = executed_sql(
            r#"{
                project(id: 1) {
                    id
                    issues(filter: { isTerminal: false }, limit: 5) {
                        number
                        labels(limit: 3) { name }
                    }
                }
            }"#,
        )
        .await;

        assert_contains_in_order(
            &sql,
            &[
                "FROM issue_labels il5 JOIN labels l4",
                "WHERE il5.repo_id=i1.repo_id AND il5.issue_number=i1.number",
                "ORDER BY l4.name LIMIT 3 OFFSET 0",
                "FROM issue_projects ip3 JOIN issues i1",
                "JOIN issue_states s2",
                "WHERE ip3.repo_id=p.repo_id AND ip3.project_id=p.id",
                "s2.is_terminal=0",
                "ORDER BY i1.number LIMIT 5 OFFSET 0",
                "FROM projects p",
            ],
        );
        assert_eq!(sql.matches("FROM issue_projects").count(), 1);
        assert_eq!(sql.matches("FROM issue_labels").count(), 1);
    }

    #[tokio::test]
    async fn project_milestones_issues_project_compiles_three_relation_hops() {
        let sql = executed_sql(
            r#"{
                project(id: 1) {
                    milestones(limit: 4) {
                        name
                        issues(limit: 6) {
                            number
                            project { id name }
                        }
                    }
                }
            }"#,
        )
        .await;

        assert_contains_in_order(
            &sql,
            &[
                "FROM issue_projects ip6 JOIN projects p5",
                "WHERE ip6.repo_id=i2.repo_id AND ip6.issue_number=i2.number",
                "FROM issue_milestones im4 JOIN issues i2",
                "WHERE im4.repo_id=m1.repo_id AND im4.project_id=m1.project_id AND im4.milestone_id=m1.id",
                "ORDER BY i2.number LIMIT 6 OFFSET 0",
                "FROM project_milestones m1",
                "WHERE m1.repo_id=p.repo_id AND m1.project_id=p.id",
                "ORDER BY m1.position,m1.id LIMIT 4 OFFSET 0",
                "FROM projects p",
            ],
        );
        assert!(!sql.contains("issue_states"));
        assert!(!sql.contains("issue_labels"));
    }
}
