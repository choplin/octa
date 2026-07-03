//! Integration tests driving the built `octa` binary against throwaway git
//! repositories. These exercise the issue lifecycle and the worktree-sharing
//! guarantee that is octa's reason to exist.

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

/// A fresh git repository with an initial commit (so `git worktree add` works).
fn init_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "test"]);
    git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "init"]);
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(bin())
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to spawn octa")
}

fn run_ok(dir: &Path, args: &[&str]) -> String {
    let out = run(dir, args);
    assert!(
        out.status.success(),
        "octa {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn create_list_show_roundtrip() {
    let repo = init_repo();
    let dir = repo.path();

    let created = run_ok(
        dir,
        &["issue", "create", "--title", "First", "--body", "hello"],
    );
    assert_eq!(created.trim(), "#1");

    // A second issue gets the next sequential number.
    let created2 = run_ok(dir, &["issue", "create", "--title", "Second"]);
    assert_eq!(created2.trim(), "#2");

    let listed = run_ok(dir, &["issue", "list"]);
    assert!(listed.contains("First"), "list missing First: {listed}");
    assert!(listed.contains("Second"), "list missing Second: {listed}");

    let shown = run_ok(dir, &["issue", "show", "1"]);
    assert!(shown.contains("First"), "show missing title: {shown}");
    assert!(shown.contains("hello"), "show missing body: {shown}");
}

#[test]
fn comment_appears_in_thread() {
    let repo = init_repo();
    let dir = repo.path();

    run_ok(dir, &["issue", "create", "--title", "Discuss"]);
    run_ok(dir, &["issue", "comment", "1", "--body", "first reply"]);
    run_ok(dir, &["issue", "comment", "1", "--body", "second reply"]);

    let shown = run_ok(dir, &["issue", "show", "1"]);
    let first = shown.find("first reply").expect("first reply missing");
    let second = shown.find("second reply").expect("second reply missing");
    assert!(
        first < second,
        "comments out of chronological order: {shown}"
    );
}

#[test]
fn close_reopen_and_state_filter() {
    let repo = init_repo();
    let dir = repo.path();

    run_ok(dir, &["issue", "create", "--title", "Bug"]);
    run_ok(dir, &["issue", "close", "1"]);

    // Default list is open-only: the closed issue is hidden.
    let open_list = run_ok(dir, &["issue", "list"]);
    assert!(
        !open_list.contains("Bug"),
        "closed issue still in open list: {open_list}"
    );

    let closed_list = run_ok(dir, &["issue", "list", "--state", "closed"]);
    assert!(
        closed_list.contains("Bug"),
        "closed issue missing from closed list: {closed_list}"
    );

    let all_list = run_ok(dir, &["issue", "list", "--state", "all"]);
    assert!(
        all_list.contains("Bug"),
        "closed issue missing from all list: {all_list}"
    );

    run_ok(dir, &["issue", "reopen", "1"]);
    let reopened = run_ok(dir, &["issue", "list"]);
    assert!(
        reopened.contains("Bug"),
        "reopened issue missing from open list: {reopened}"
    );
}

#[test]
fn edit_updates_title_and_body() {
    let repo = init_repo();
    let dir = repo.path();

    run_ok(
        dir,
        &["issue", "create", "--title", "Old", "--body", "old body"],
    );
    run_ok(
        dir,
        &["issue", "edit", "1", "--title", "New", "--body", "new body"],
    );

    let shown = run_ok(dir, &["issue", "show", "1"]);
    assert!(shown.contains("New"), "edited title missing: {shown}");
    assert!(shown.contains("new body"), "edited body missing: {shown}");
    assert!(!shown.contains("old body"), "stale body present: {shown}");
}

#[test]
fn json_outputs_have_expected_fields() {
    let repo = init_repo();
    let dir = repo.path();

    let created = run_ok(dir, &["issue", "create", "--title", "JSON", "--json"]);
    let created_json: serde_json::Value = serde_json::from_str(created.trim()).unwrap();
    assert_eq!(created_json["number"], 1);

    run_ok(dir, &["issue", "comment", "1", "--body", "a note"]);

    let list = run_ok(dir, &["issue", "list", "--json"]);
    let list_json: serde_json::Value = serde_json::from_str(list.trim()).unwrap();
    let arr = list_json.as_array().expect("list --json is not an array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["number"], 1);
    assert_eq!(arr[0]["title"], "JSON");
    assert_eq!(arr[0]["state"], "open");

    let show = run_ok(dir, &["issue", "show", "1", "--json"]);
    let show_json: serde_json::Value = serde_json::from_str(show.trim()).unwrap();
    assert_eq!(show_json["number"], 1);
    assert_eq!(show_json["title"], "JSON");
    let comments = show_json["comments"]
        .as_array()
        .expect("comments not an array");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["body"], "a note");
}

#[test]
fn issues_are_shared_across_worktrees() {
    let repo = init_repo();
    let main_dir = repo.path();

    // Create an issue in the main worktree.
    run_ok(
        main_dir,
        &["issue", "create", "--title", "Shared", "--json"],
    );

    // Add a linked worktree in a separate location.
    let wt_parent = TempDir::new().unwrap();
    let wt: PathBuf = wt_parent.path().join("linked");
    git(
        main_dir,
        &[
            "worktree",
            "add",
            "-q",
            wt.to_str().unwrap(),
            "-b",
            "feature",
        ],
    );

    // The linked worktree must see the same issue.
    let list = run_ok(&wt, &["issue", "list", "--json"]);
    let list_json: serde_json::Value = serde_json::from_str(list.trim()).unwrap();
    let arr = list_json.as_array().expect("list --json is not an array");
    assert_eq!(
        arr.len(),
        1,
        "worktree did not see the shared issue: {list}"
    );
    assert_eq!(arr[0]["title"], "Shared");

    // And a comment added from the worktree is visible in the main worktree.
    run_ok(&wt, &["issue", "comment", "1", "--body", "from worktree"]);
    let shown = run_ok(main_dir, &["issue", "show", "1"]);
    assert!(
        shown.contains("from worktree"),
        "cross-worktree comment missing: {shown}"
    );
}
