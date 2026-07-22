//! Integration tests driving the built `octa` binary against throwaway git
//! repositories, each with an isolated global store (a per-test XDG data dir).
//! These exercise every primitive plus the worktree-sharing guarantee that is
//! octa's reason to exist.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_octa")
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .expect("failed to spawn git");
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

/// A throwaway repository plus its own isolated global store.
struct Env {
    repo: TempDir,
    xdg: TempDir,
}

impl Env {
    fn new() -> Self {
        let repo = TempDir::new().unwrap();
        git(repo.path(), &["init", "-q", "-b", "main"]);
        git(repo.path(), &["config", "user.email", "test@example.com"]);
        git(repo.path(), &["config", "user.name", "test"]);
        git(
            repo.path(),
            &["commit", "-q", "--allow-empty", "-m", "init"],
        );
        Env {
            repo,
            xdg: TempDir::new().unwrap(),
        }
    }

    fn path(&self) -> &Path {
        self.repo.path()
    }

    /// Run octa in `dir`, pointing its global store at this env's XDG dir.
    fn run_in(&self, dir: &Path, args: &[&str]) -> Output {
        Command::new(bin())
            .current_dir(dir)
            .env("XDG_DATA_HOME", self.xdg.path())
            .args(args)
            .output()
            .expect("failed to spawn octa")
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_in(self.path(), args)
    }

    fn ok(&self, args: &[&str]) -> String {
        self.ok_in(self.path(), args)
    }

    fn ok_in(&self, dir: &Path, args: &[&str]) -> String {
        let out = self.run_in(dir, args);
        assert!(
            out.status.success(),
            "octa {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s.trim()).unwrap()
}

// --- Issue lifecycle --------------------------------------------------------

#[test]
fn create_list_show_roundtrip() {
    let env = Env::new();
    assert_eq!(
        env.ok(&["issue", "create", "--title", "First", "--body", "hello"])
            .trim(),
        "#1"
    );
    // Per-repo sequential numbering.
    assert_eq!(
        env.ok(&["issue", "create", "--title", "Second"]).trim(),
        "#2"
    );

    let listed = env.ok(&["issue", "list"]);
    assert!(listed.contains("First"), "list missing First: {listed}");
    assert!(listed.contains("Second"), "list missing Second: {listed}");

    let shown = env.ok(&["issue", "show", "1"]);
    assert!(shown.contains("First"), "show missing title: {shown}");
    assert!(shown.contains("hello"), "show missing body: {shown}");
}

#[test]
fn comment_appears_in_thread() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Discuss"]);
    env.ok(&["issue", "comment", "1", "--body", "first reply"]);
    env.ok(&["issue", "comment", "1", "--body", "second reply"]);

    let shown = env.ok(&["issue", "show", "1"]);
    let first = shown.find("first reply").expect("first reply missing");
    let second = shown.find("second reply").expect("second reply missing");
    assert!(first < second, "comments out of order: {shown}");
}

#[test]
fn close_reopen_and_state_filter() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Bug"]);
    env.ok(&["issue", "close", "1"]);

    let open_list = env.ok(&["issue", "list"]);
    assert!(
        !open_list.contains("Bug"),
        "closed issue in open list: {open_list}"
    );

    let closed_list = env.ok(&["issue", "list", "--state", "closed"]);
    assert!(
        closed_list.contains("Bug"),
        "missing from closed list: {closed_list}"
    );

    let all_list = env.ok(&["issue", "list", "--state", "all"]);
    assert!(
        all_list.contains("Bug"),
        "missing from all list: {all_list}"
    );

    env.ok(&["issue", "reopen", "1"]);
    assert!(
        env.ok(&["issue", "list"]).contains("Bug"),
        "reopened issue missing"
    );
}

#[test]
fn edit_updates_title_and_body() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Old", "--body", "old body"]);
    env.ok(&["issue", "edit", "1", "--title", "New", "--body", "new body"]);

    let shown = env.ok(&["issue", "show", "1"]);
    assert!(shown.contains("New"), "edited title missing: {shown}");
    assert!(shown.contains("new body"), "edited body missing: {shown}");
    assert!(!shown.contains("old body"), "stale body present: {shown}");
}

#[test]
fn json_outputs_have_expected_fields() {
    let env = Env::new();
    let created = json(&env.ok(&["issue", "create", "--title", "JSON", "--json"]));
    assert_eq!(created["number"], 1);

    env.ok(&["issue", "comment", "1", "--body", "a note"]);

    let list = json(&env.ok(&["issue", "list", "--json"]));
    let arr = list.as_array().expect("list --json not an array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["number"], 1);
    assert_eq!(arr[0]["title"], "JSON");
    assert_eq!(arr[0]["state"], "open");

    let show = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(show["number"], 1);
    assert_eq!(show["title"], "JSON");
    let comments = show["comments"].as_array().expect("comments not an array");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["body"], "a note");
}

// --- States, dependencies, lock ---------------------------------------------

#[test]
fn custom_state_and_set() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Task"]);

    // Default set is seeded.
    let states = json(&env.ok(&["state", "list", "--json"]));
    let names: Vec<&str> = states
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"open") && names.contains(&"in_progress") && names.contains(&"closed"));

    env.ok(&["state", "add", "blocked", "--starting"]);
    env.ok(&["issue", "set-state", "1", "blocked"]);
    let shown = env.ok(&["issue", "show", "1"]);
    assert!(shown.contains("blocked"), "state not applied: {shown}");

    // Unknown state is rejected.
    assert!(!env
        .run(&["issue", "set-state", "1", "nonsense"])
        .status
        .success());
}

#[test]
fn dependencies_and_unblocked_query() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Foundation"]); // #1
    env.ok(&["issue", "create", "--title", "Feature"]); // #2
    env.ok(&["issue", "dep", "add", "1", "2"]); // #1 blocks #2

    // #2 is blocked while #1 is open; only #1 is unblocked.
    let unblocked = json(&env.ok(&["issue", "list", "--unblocked", "--json"]));
    let nums: Vec<i64> = unblocked
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["number"].as_i64().unwrap())
        .collect();
    assert!(
        nums.contains(&1),
        "foundation should be unblocked: {nums:?}"
    );
    assert!(!nums.contains(&2), "feature should be blocked: {nums:?}");

    // Completing #1 unblocks #2.
    env.ok(&["issue", "close", "1"]);
    let unblocked = json(&env.ok(&["issue", "list", "--unblocked", "--json"]));
    let nums: Vec<i64> = unblocked
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["number"].as_i64().unwrap())
        .collect();
    assert!(
        nums.contains(&2),
        "feature should now be unblocked: {nums:?}"
    );

    // The edge is visible from both sides.
    let shown = env.ok(&["issue", "show", "2"]);
    assert!(shown.contains("blocked by"), "blocked-by missing: {shown}");
}

#[test]
fn atomic_lock_prevents_double_acquire() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Contended"]);

    env.ok(&["issue", "lock", "1", "--as", "agent-a"]);
    // A second holder cannot acquire it.
    let out = env.run(&["issue", "lock", "1", "--as", "agent-b"]);
    assert!(!out.status.success(), "second lock unexpectedly succeeded");
    assert!(String::from_utf8_lossy(&out.stderr).contains("agent-a"));

    // The holder can release, then another may take it.
    env.ok(&["issue", "unlock", "1", "--as", "agent-a"]);
    env.ok(&["issue", "lock", "1", "--as", "agent-b"]);
}

// --- Labels -----------------------------------------------------------------

#[test]
fn single_select_group_is_mutually_exclusive() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Typed"]);
    env.ok(&["label", "group", "type", "--selection", "single"]);
    env.ok(&["label", "create", "impl", "--group", "type"]);
    env.ok(&["label", "create", "design", "--group", "type"]);

    env.ok(&["issue", "label", "1", "impl"]);
    env.ok(&["issue", "label", "1", "design"]); // replaces impl (single group)

    let show = json(&env.ok(&["issue", "show", "1", "--json"]));
    let labels: Vec<&str> = show["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        vec!["design"],
        "single group not exclusive: {labels:?}"
    );

    // Filter by label.
    let listed = json(&env.ok(&["issue", "list", "--label", "design", "--json"]));
    assert_eq!(listed.as_array().unwrap().len(), 1);
}

#[test]
fn multi_select_group_labels_coexist() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Multi"]);
    env.ok(&["label", "group", "area", "--selection", "multi"]);
    env.ok(&["label", "create", "cli", "--group", "area"]);
    env.ok(&["label", "create", "storage", "--group", "area"]);

    env.ok(&["issue", "label", "1", "cli"]);
    env.ok(&["issue", "label", "1", "storage"]);

    let show = json(&env.ok(&["issue", "show", "1", "--json"]));
    let mut labels: Vec<&str> = show["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    labels.sort();
    assert_eq!(labels, vec!["cli", "storage"], "multi group should coexist");
}

// --- Pull requests ----------------------------------------------------------

#[test]
fn pr_lifecycle_tracks_branch_and_comments() {
    let env = Env::new();
    let created = env.ok(&[
        "pr",
        "create",
        "--title",
        "Add feature",
        "--branch",
        "feat/x",
    ]);
    assert_eq!(created.trim(), "#1");

    env.ok(&["pr", "comment", "1", "--body", "looks good"]);
    let show = json(&env.ok(&["pr", "show", "1", "--json"]));
    assert_eq!(show["branch"], "feat/x");
    assert_eq!(show["comments"].as_array().unwrap().len(), 1);
    let text_show = env.ok(&["pr", "show", "1"]);
    assert!(text_show.contains("--- comments ---"));
    assert!(text_show.contains("looks good"));

    env.ok(&["pr", "close", "1"]);
    let open = env.ok(&["pr", "list"]);
    assert!(
        !open.contains("Add feature"),
        "closed PR still open: {open}"
    );
    let all = env.ok(&["pr", "list", "--state", "all"]);
    assert!(all.contains("Add feature"), "PR missing from all: {all}");
}

// --- Wiki -------------------------------------------------------------------

#[test]
fn wiki_pages_link_and_backlink() {
    let env = Env::new();
    env.ok(&[
        "wiki",
        "create",
        "--title",
        "Home",
        "--body",
        "see [[design]]",
    ]);
    env.ok(&[
        "wiki",
        "create",
        "--title",
        "Design",
        "--slug",
        "design",
        "--body",
        "the design",
    ]);

    let home = json(&env.ok(&["wiki", "show", "home", "--json"]));
    let links: Vec<&str> = home["links_to"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    assert!(
        links.contains(&"design"),
        "home should link to design: {links:?}"
    );

    let design = json(&env.ok(&["wiki", "show", "design", "--json"]));
    let backlinks: Vec<&str> = design["backlinks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    assert!(
        backlinks.contains(&"home"),
        "design should be backlinked by home: {backlinks:?}"
    );
    let home_text = env.ok(&["wiki", "show", "home"]);
    assert!(home_text.contains("links to: design"));
    let design_text = env.ok(&["wiki", "show", "design"]);
    assert!(design_text.contains("backlinks: home"));
}

// --- Cross-worktree and cross-repo scope ------------------------------------

#[test]
fn issues_are_shared_across_worktrees() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Shared", "--json"]);

    let wt_parent = TempDir::new().unwrap();
    let wt: PathBuf = wt_parent.path().join("linked");
    git(
        env.path(),
        &[
            "worktree",
            "add",
            "-q",
            wt.to_str().unwrap(),
            "-b",
            "feature",
        ],
    );

    let list = json(&env.ok_in(&wt, &["issue", "list", "--json"]));
    let arr = list.as_array().expect("list --json not an array");
    assert_eq!(
        arr.len(),
        1,
        "worktree did not see the shared issue: {list}"
    );
    assert_eq!(arr[0]["title"], "Shared");

    env.ok_in(&wt, &["issue", "comment", "1", "--body", "from worktree"]);
    let shown = env.ok(&["issue", "show", "1"]);
    assert!(
        shown.contains("from worktree"),
        "cross-worktree comment missing: {shown}"
    );
}

#[test]
fn all_repos_aggregates_across_repositories() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "In first repo"]);
    env.ok(&["issue", "close", "1"]);

    // A second repository sharing the same global store.
    let repo2 = TempDir::new().unwrap();
    git(repo2.path(), &["init", "-q", "-b", "main"]);
    git(repo2.path(), &["config", "user.email", "t@e.com"]);
    git(repo2.path(), &["config", "user.name", "t"]);
    git(
        repo2.path(),
        &["commit", "-q", "--allow-empty", "-m", "init"],
    );
    env.ok_in(
        repo2.path(),
        &["issue", "create", "--title", "In second repo"],
    );

    // Each repo numbers from 1 independently.
    let second = json(&env.ok_in(repo2.path(), &["issue", "list", "--json"]));
    assert_eq!(second.as_array().unwrap()[0]["number"], 1);

    // --all-repos sees both.
    let all = json(&env.ok_in(
        repo2.path(),
        &["issue", "list", "--all-repos", "--state", "all", "--json"],
    ));
    let titles: Vec<&str> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    assert!(
        titles.contains(&"In first repo"),
        "cross-repo view missing first: {titles:?}"
    );
    assert!(
        titles.contains(&"In second repo"),
        "cross-repo view missing second: {titles:?}"
    );

    let closed = json(&env.ok_in(
        repo2.path(),
        &[
            "issue",
            "list",
            "--all-repos",
            "--state",
            "closed",
            "--json",
        ],
    ));
    assert_eq!(closed.as_array().unwrap().len(), 1);
    assert_eq!(closed.as_array().unwrap()[0]["title"], "In first repo");

    for args in [
        vec!["issue", "list", "--all-repos", "--state", "in_progress"],
        vec!["issue", "list", "--all-repos", "--label", "missing"],
        vec!["issue", "list", "--all-repos", "--unblocked"],
    ] {
        let out = env.run_in(repo2.path(), &args);
        assert!(!out.status.success(), "{args:?} unexpectedly succeeded");
    }
}

#[test]
fn pr_state_filter_and_edit_validation_are_preserved() {
    let env = Env::new();
    env.ok(&["pr", "create", "--title", "One", "--branch", "one"]);
    env.ok(&["pr", "set-state", "1", "waiting"]);
    assert!(env.ok(&["pr", "list", "--state", "closed"]).contains("One"));
    assert!(!env.ok(&["pr", "list"]).contains("One"));
    let out = env.run(&["pr", "edit", "1"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nothing to update"));
}

#[test]
fn wiki_body_edit_replaces_links_and_title_edit_preserves_them() {
    let env = Env::new();
    env.ok(&[
        "wiki",
        "create",
        "--title",
        "Home",
        "--body",
        "[[old]] [[old]] [[home]]",
    ]);
    env.ok(&["wiki", "create", "--title", "Old", "--slug", "old"]);
    env.ok(&["wiki", "create", "--title", "New", "--slug", "new"]);
    env.ok(&["wiki", "edit", "home", "--body", "[[new]]"]);
    assert_eq!(
        json(&env.ok(&["wiki", "show", "home", "--json"]))["links_to"],
        serde_json::json!(["new"])
    );
    assert!(
        json(&env.ok(&["wiki", "show", "old", "--json"]))["backlinks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    env.ok(&["wiki", "edit", "home", "--title", "Renamed"]);
    assert_eq!(
        json(&env.ok(&["wiki", "show", "home", "--json"]))["links_to"],
        serde_json::json!(["new"])
    );
}

#[test]
fn label_errors_and_idempotent_operations_are_preserved() {
    let env = Env::new();
    assert!(!env
        .run(&["label", "group", "kind", "--selection", "bad"])
        .status
        .success());
    assert!(!env
        .run(&["label", "create", "x", "--group", "missing"])
        .status
        .success());
    env.ok(&["issue", "create", "--title", "Task"]);
    env.ok(&["label", "create", "plain"]);
    env.ok(&["issue", "label", "1", "plain"]);
    env.ok(&["issue", "label", "1", "plain"]);
    env.ok(&["issue", "unlabel", "1", "missing"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["labels"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(!env.run(&["issue", "label", "99", "plain"]).status.success());
}
