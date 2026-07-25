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

#[test]
fn issue_help_advertises_tui_without_hiding_existing_commands() {
    let output = Command::new(bin())
        .args(["issue", "--help"])
        .output()
        .expect("failed to run issue help");
    assert!(
        output.status.success(),
        "issue help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    for command in ["tui", "create", "list", "show", "comment", "dep", "relate"] {
        assert!(
            stdout
                .lines()
                .any(|line| line.trim_start().starts_with(command)),
            "issue help missing {command:?}:\n{stdout}"
        );
    }
    assert!(
        !stdout
            .lines()
            .any(|line| line.trim_start().starts_with("transition")),
        "issue help still advertises removed transition command:\n{stdout}"
    );
    assert!(stdout.contains("read-only terminal interface"));
}

#[test]
fn issue_tui_rejects_ambiguous_all_repo_details_before_terminal_mode() {
    let env = Env::new();
    let output = env.run(&["issue", "tui", "--all-repos"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("single repository"));
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
    assert_eq!(arr[0]["status_type"], "unstarted");
    assert_eq!(arr[0]["priority"], 0);

    let show = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(show["number"], 1);
    assert_eq!(show["title"], "JSON");
    assert_eq!(show["status_type"], "unstarted");
    assert_eq!(show["priority"], 0);
    let comments = show["comments"].as_array().expect("comments not an array");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["body"], "a note");
}

#[test]
fn status_type_and_priority_filters_preserve_stable_issue_order() {
    let env = Env::new();
    for (name, status_type) in [
        ("Backlog", "backlog"),
        ("Todo", "unstarted"),
        ("In Progress", "started"),
        ("In Review", "started"),
        ("Done", "completed"),
    ] {
        env.ok(&["state", "add", name, "--type", status_type]);
    }
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Backlog low",
        "--state",
        "Backlog",
        "--priority",
        "4",
    ]); // #1
    env.ok(&["issue", "create", "--title", "Todo none", "--state", "Todo"]); // #2
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Backlog urgent",
        "--state",
        "Backlog",
        "--priority",
        "1",
    ]); // #3
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Todo urgent",
        "--state",
        "Todo",
        "--priority",
        "1",
    ]); // #4
    env.ok(&[
        "issue",
        "create",
        "--title",
        "In flight",
        "--state",
        "In Progress",
        "--priority",
        "4",
    ]); // #5
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Awaiting review",
        "--state",
        "In Review",
        "--priority",
        "1",
    ]); // #6
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Already done",
        "--state",
        "Done",
        "--priority",
        "1",
    ]); // #7

    let listed = json(&env.ok(&["issue", "list", "--json"]));
    let numbers: Vec<i64> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|issue| issue["number"].as_i64().unwrap())
        .collect();
    // The general list and TUI keep stable issue-number order.
    assert_eq!(numbers, vec![1, 2, 3, 4, 5, 6]);

    let backlog = json(&env.ok(&[
        "issue",
        "list",
        "--status-type",
        "backlog",
        "--priority",
        "1",
        "--json",
    ]));
    assert_eq!(backlog.as_array().unwrap().len(), 1);
    assert_eq!(backlog[0]["number"], 3);
}

#[test]
fn priority_and_status_type_validation_and_edit_roundtrip() {
    let env = Env::new();
    env.ok(&["state", "add", "Todo", "--type", "unstarted"]);
    let created = env.ok(&[
        "issue",
        "create",
        "--title",
        "Prioritized",
        "--state",
        "Todo",
        "--priority",
        "2",
    ]);
    assert_eq!(created.trim(), "#1");
    let show = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(show["state"], "Todo");
    assert_eq!(show["status_type"], "unstarted");
    assert_eq!(show["priority"], 2);

    env.ok(&["issue", "edit", "1", "--priority", "1"]);
    let show = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(show["priority"], 1);

    assert!(!env
        .run(&["issue", "create", "--title", "Bad", "--priority", "5"])
        .status
        .success());
    assert!(!env
        .run(&["issue", "list", "--status-type", "unknown"])
        .status
        .success());
    assert!(!env
        .run(&["state", "add", "Odd", "--type", "unknown"])
        .status
        .success());
}

#[test]
fn project_lifecycle_tally_and_issue_context_roundtrip() {
    let env = Env::new();
    for (name, status_type) in [
        ("Todo", "unstarted"),
        ("In Progress", "started"),
        ("Canceled", "canceled"),
    ] {
        env.ok(&["state", "add", name, "--type", status_type]);
    }
    let created = json(&env.ok(&[
        "project",
        "create",
        "--name",
        "Ship CLI",
        "--summary",
        "Finite outcome",
        "--description",
        "Deliver the CLI.",
        "--priority",
        "2",
        "--json",
    ]));
    assert_eq!(created["id"], 1);

    env.ok(&[
        "issue",
        "create",
        "--title",
        "Parent",
        "--state",
        "Todo",
        "--project",
        "Ship CLI",
    ]);
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Child",
        "--state",
        "In Progress",
        "--parent",
        "1",
    ]);
    env.ok(&["issue", "create", "--title", "Standalone"]);
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Canceled work",
        "--state",
        "Canceled",
        "--project",
        "Ship CLI",
    ]);

    let child = json(&env.ok(&["issue", "show", "2", "--json"]));
    assert_eq!(child["project"]["name"], "Ship CLI");
    assert_eq!(child["parent"]["number"], 1);
    let parent = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(parent["sub_issues"][0]["number"], 2);
    assert!(json(&env.ok(&["issue", "show", "3", "--json"]))["project"].is_null());

    let filtered = json(&env.ok(&["issue", "list", "--project", "Ship CLI", "--json"]));
    assert_eq!(filtered.as_array().unwrap().len(), 2);
    let project = json(&env.ok(&["project", "show", "1", "--json"]));
    assert_eq!(project["tally"]["unstarted"], 1);
    assert_eq!(project["tally"]["started"], 1);
    assert_eq!(project["tally"]["canceled"], 1);
    assert_eq!(project["tally"]["total"], 3);
    let overview = json(&env.ok(&["project", "list", "--json"]));
    assert_eq!(overview[0]["tally"]["unstarted"], 1);
    assert_eq!(overview[0]["tally"]["started"], 1);
    assert_eq!(overview[0]["tally"]["completed"], 0);
    assert_eq!(overview[0]["tally"]["canceled"], 1);
    assert_eq!(overview[0]["tally"]["total"], 3);

    env.ok(&[
        "project",
        "set-state",
        "1",
        "shipped",
        "--type",
        "completed",
    ]);
    assert_eq!(
        json(&env.ok(&["project", "list", "--json"]))[0]["status_type"],
        "completed"
    );
    assert!(json(&env.ok(&["project", "list", "--active", "--json"]))
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn project_list_orders_priorities_with_none_last_and_filters_active_explicitly() {
    let env = Env::new();
    for (name, priority) in [
        ("None", "0"),
        ("Low", "4"),
        ("Urgent", "1"),
        ("Medium", "3"),
        ("High", "2"),
    ] {
        env.ok(&["project", "create", "--name", name, "--priority", priority]);
    }
    env.ok(&[
        "project",
        "set-state",
        "Urgent",
        "canceled",
        "--type",
        "canceled",
    ]);

    let all = json(&env.ok(&["project", "list", "--json"]));
    assert_eq!(
        all.as_array()
            .unwrap()
            .iter()
            .map(|project| project["priority"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 0]
    );
    assert_eq!(all[0]["status_type"], "canceled");
    let all_text = env.ok(&["project", "list"]);
    assert!(all_text.contains("Urgent"), "{all_text}");
    assert!(all_text.contains("issues B/U/S/D/C"), "{all_text}");

    let active = json(&env.ok(&["project", "list", "--active", "--json"]));
    assert_eq!(
        active
            .as_array()
            .unwrap()
            .iter()
            .map(|project| project["priority"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 0]
    );
}

#[test]
fn parent_self_and_cycles_are_rejected_while_projects_are_independent() {
    let env = Env::new();
    env.ok(&["project", "create", "--name", "One"]);
    env.ok(&["project", "create", "--name", "Two"]);
    env.ok(&["issue", "create", "--title", "A", "--project", "One"]);
    env.ok(&["issue", "create", "--title", "C", "--project", "Two"]);
    assert!(!env
        .run(&["issue", "parent", "set", "1", "1"])
        .status
        .success());
    env.ok(&["issue", "parent", "set", "2", "1"]);
    assert!(!env
        .run(&["issue", "parent", "set", "1", "2"])
        .status
        .success());

    let child = json(&env.ok(&["issue", "show", "2", "--json"]));
    assert_eq!(child["project"]["name"], "Two");
    assert_eq!(child["parent"]["number"], 1);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["sub_issues"][0]["number"],
        2
    );
}

#[test]
fn parent_child_project_mutations_are_independent_and_inheritance_is_initial_only() {
    let env = Env::new();
    env.ok(&["project", "create", "--name", "One"]);
    env.ok(&["project", "create", "--name", "Two"]);
    env.ok(&["issue", "create", "--title", "Parent", "--project", "One"]);
    env.ok(&["issue", "create", "--title", "Child"]);
    env.ok(&["issue", "parent", "set", "2", "1"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "2", "--json"]))["project"]["name"],
        "One",
        "a project-less child should retain initial inheritance"
    );

    env.ok(&["issue", "project", "set", "1", "Two"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["project"]["name"],
        "Two"
    );
    assert_eq!(
        json(&env.ok(&["issue", "show", "2", "--json"]))["project"]["name"],
        "One",
        "later parent Project changes must not propagate to the child"
    );
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Explicit different Project",
        "--parent",
        "1",
        "--project",
        "One",
    ]);
    let different = json(&env.ok(&["issue", "show", "3", "--json"]));
    assert_eq!(different["project"]["name"], "One");
    assert_eq!(different["parent"]["number"], 1);

    env.ok(&["issue", "project", "clear", "1"]);
    env.ok(&["issue", "project", "set", "2", "Two"]);
    env.ok(&["issue", "project", "clear", "2"]);

    let parent = json(&env.ok(&["issue", "show", "1", "--json"]));
    let child = json(&env.ok(&["issue", "show", "2", "--json"]));
    assert!(parent["project"].is_null());
    assert!(child["project"].is_null());
    assert_eq!(child["parent"]["number"], 1);
    assert_eq!(parent["sub_issues"][0]["number"], 2);
    let child_text = env.ok(&["issue", "show", "2"]);
    assert!(child_text.contains("parent: #1 Parent"), "{child_text}");

    env.ok(&[
        "issue",
        "create",
        "--title",
        "Only child has a Project",
        "--parent",
        "1",
        "--project",
        "One",
    ]);
    let explicit = json(&env.ok(&["issue", "show", "4", "--json"]));
    assert_eq!(explicit["project"]["name"], "One");
    assert_eq!(explicit["parent"]["number"], 1);
}

#[test]
fn parent_references_are_repository_scoped() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Repo one child"]);

    let repo2 = TempDir::new().unwrap();
    git(repo2.path(), &["init", "-q", "-b", "main"]);
    git(repo2.path(), &["config", "user.email", "test@example.com"]);
    git(repo2.path(), &["config", "user.name", "test"]);
    git(
        repo2.path(),
        &["commit", "-q", "--allow-empty", "-m", "init"],
    );
    env.ok_in(
        repo2.path(),
        &["issue", "create", "--title", "Repo two one"],
    );
    env.ok_in(
        repo2.path(),
        &["issue", "create", "--title", "Repo two two"],
    );

    let rejected = env.run(&["issue", "parent", "set", "1", "2"]);
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("issue #2 not found"),
        "{}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    assert!(json(&env.ok(&["issue", "show", "1", "--json"]))["parent"].is_null());
}

#[test]
fn project_names_are_unambiguous() {
    let env = Env::new();
    env.ok(&["project", "create", "--name", "Case"]);
    let duplicate = env.run(&["project", "create", "--name", "case"]);
    assert!(!duplicate.status.success());
    assert!(!env
        .run(&["project", "create", "--name", "123"])
        .status
        .success());
    assert!(!env
        .run(&["project", "edit", "Case", "--name", "456"])
        .status
        .success());
    assert_eq!(
        json(&env.ok(&["project", "show", "cAsE", "--json"]))["name"],
        "Case"
    );
}

#[test]
fn project_milestones_roundtrip_filter_and_reject_inconsistent_project_changes() {
    let env = Env::new();
    env.ok(&["project", "create", "--name", "Launch"]);
    env.ok(&["project", "create", "--name", "Other"]);
    env.ok(&[
        "project",
        "milestone",
        "create",
        "Launch",
        "--name",
        "Beta",
        "--description",
        "Public beta phase",
        "--status",
        "active",
        "--position",
        "2",
        "--target-date",
        "2026-09-01",
    ]);
    env.ok(&[
        "project",
        "milestone",
        "create",
        "Launch",
        "--name",
        "Alpha",
        "--position",
        "1",
    ]);

    let milestones = json(&env.ok(&["project", "milestone", "list", "Launch", "--json"]));
    assert_eq!(milestones[0]["name"], "Alpha");
    assert_eq!(milestones[1]["name"], "Beta");
    assert_eq!(milestones[1]["target_date"], "2026-09-01");

    env.ok(&[
        "issue",
        "create",
        "--title",
        "Invite users",
        "--project",
        "Launch",
        "--milestone",
        "bEtA",
    ]);
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Unphased",
        "--project",
        "Launch",
    ]);
    let issue = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(issue["project"]["name"], "Launch");
    assert_eq!(issue["milestone"]["name"], "Beta");
    let issue_text = env.ok(&["issue", "show", "1"]);
    assert!(issue_text.contains("project: Launch"));
    assert!(issue_text.contains("milestone: Beta"));

    env.ok(&[
        "project",
        "milestone",
        "edit",
        "Launch",
        "Beta",
        "--status",
        "completed",
        "--target-date",
        "2026-09-15",
    ]);
    let edited = json(&env.ok(&["project", "milestone", "show", "Launch", "Beta", "--json"]));
    assert_eq!(edited["status"], "completed");
    assert_eq!(edited["target_date"], "2026-09-15");

    let filtered = json(&env.ok(&[
        "issue",
        "list",
        "--project",
        "Launch",
        "--milestone",
        "Beta",
        "--json",
    ]));
    assert_eq!(filtered.as_array().unwrap().len(), 1);
    assert_eq!(filtered[0]["number"], 1);

    for args in [
        vec!["issue", "project", "set", "1", "Other"],
        vec!["issue", "project", "clear", "1"],
    ] {
        let rejected = env.run(&args);
        assert!(
            !rejected.status.success(),
            "{args:?} unexpectedly succeeded"
        );
        assert!(
            String::from_utf8_lossy(&rejected.stderr).contains("clear the milestone first"),
            "{}",
            String::from_utf8_lossy(&rejected.stderr)
        );
    }
    env.ok(&["issue", "milestone", "clear", "1"]);
    env.ok(&["issue", "project", "set", "1", "Other"]);
}

#[test]
fn milestone_requires_project_context_and_names_are_unambiguous() {
    let env = Env::new();
    env.ok(&["project", "create", "--name", "Launch"]);
    env.ok(&["project", "create", "--name", "Other"]);
    env.ok(&["project", "milestone", "create", "Launch", "--name", "Beta"]);
    env.ok(&[
        "project",
        "milestone",
        "create",
        "Other",
        "--name",
        "Other only",
    ]);

    assert!(!env
        .run(&[
            "issue",
            "create",
            "--title",
            "No project",
            "--milestone",
            "Beta"
        ])
        .status
        .success());
    assert!(!env
        .run(&["project", "milestone", "create", "Launch", "--name", "beta"])
        .status
        .success());
    assert!(!env
        .run(&["project", "milestone", "create", "Launch", "--name", "123"])
        .status
        .success());
    let wrong_project = env.run(&[
        "issue",
        "create",
        "--title",
        "Wrong project milestone",
        "--project",
        "Launch",
        "--milestone",
        "Other only",
    ]);
    assert!(!wrong_project.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_project.stderr).contains("not found in project"),
        "{}",
        String::from_utf8_lossy(&wrong_project.stderr)
    );
    assert_eq!(
        json(&env.ok(&["issue", "list", "--json"]))
            .as_array()
            .unwrap()
            .len(),
        0,
        "failed create must not leave an issue behind"
    );
    env.ok(&["issue", "create", "--title", "Standalone"]);
    let rejected = env.run(&["issue", "milestone", "set", "1", "Beta"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("needs a project"));
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
    for expected in ["open", "in_progress", "closed"] {
        assert!(
            names.contains(&expected),
            "missing state {expected}: {names:?}"
        );
    }
    assert_eq!(
        names,
        vec!["open", "in_progress", "closed"],
        "new repositories must not receive local workflow defaults"
    );

    env.ok(&["state", "add", "In Review", "--type", "started"]);
    env.ok(&["issue", "set-state", "1", "In Review"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["status_type"],
        "started"
    );

    env.ok(&[
        "state",
        "add",
        "blocked",
        "--type",
        "unstarted",
        "--starting",
    ]);
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
    env.ok(&["issue", "create", "--title", "Grouped"]);
    env.ok(&["label", "group", "delivery", "--selection", "single"]);
    env.ok(&["label", "create", "alpha", "--group", "delivery"]);
    env.ok(&["label", "create", "beta", "--group", "delivery"]);

    env.ok(&["issue", "label", "1", "alpha"]);
    env.ok(&["issue", "label", "1", "beta"]); // replaces alpha (single group)

    let show = json(&env.ok(&["issue", "show", "1", "--json"]));
    let labels: Vec<&str> = show["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        vec!["beta"],
        "single group not exclusive: {labels:?}"
    );

    // Filter by label.
    let listed = json(&env.ok(&["issue", "list", "--label", "beta", "--json"]));
    assert_eq!(listed.as_array().unwrap().len(), 1);
}

#[test]
fn taxonomy_like_names_are_ordinary_label_data() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Opaque labels"]);
    env.ok(&["label", "group", "Type", "--selection", "multi"]);
    env.ok(&["label", "create", "arbitrary", "--group", "Type"]);
    for label in ["impl", "design", "research"] {
        env.ok(&["label", "create", label]);
        env.ok(&["issue", "label", "1", label]);
    }

    let show = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(
        show["labels"],
        serde_json::json!(["design", "impl", "research"])
    );
    let groups = json(&env.ok(&["label", "groups", "--json"]));
    assert_eq!(groups[0]["name"], "Type");
    assert_eq!(groups[0]["selection"], "multi");
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

#[test]
fn related_issues_are_symmetric_idempotent_and_filterable() {
    let env = Env::new();
    for title in ["First", "Second", "Third"] {
        env.ok(&["issue", "create", "--title", title]);
    }

    env.ok(&["issue", "relate", "add", "2", "1"]);
    env.ok(&["issue", "relate", "add", "1", "2"]);

    let first = json(&env.ok(&["issue", "show", "1", "--json"]));
    let second = json(&env.ok(&["issue", "show", "2", "--json"]));
    assert_eq!(first["related"], serde_json::json!([2]));
    assert_eq!(second["related"], serde_json::json!([1]));
    assert!(env.ok(&["issue", "show", "1"]).contains("related: #2"));

    let filtered = json(&env.ok(&["issue", "list", "--related-to", "1", "--json"]));
    assert_eq!(filtered.as_array().unwrap().len(), 1);
    assert_eq!(filtered[0]["number"], 2);

    let self_relation = env.run(&["issue", "relate", "add", "1", "1"]);
    assert!(!self_relation.status.success());
    assert!(String::from_utf8_lossy(&self_relation.stderr).contains("itself"));

    env.ok(&["issue", "relate", "rm", "2", "1"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["related"],
        serde_json::json!([])
    );
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

#[test]
fn issue_pr_links_are_many_to_many_idempotent_and_unlink_exact_pairs() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Implement"]);
    env.ok(&["issue", "create", "--title", "Other"]);

    env.ok(&[
        "pr",
        "create",
        "--title",
        "Implementation",
        "--branch",
        "feat/implementation",
        "--issue",
        "1",
    ]);
    let issue = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(issue["pull_requests"][0]["number"], 1);
    assert_eq!(issue["pull_requests"][0]["title"], "Implementation");
    assert_eq!(issue["pull_requests"][0]["branch"], "feat/implementation");
    assert_eq!(issue["pull_requests"][0]["state"], "open");
    let text = env.ok(&["issue", "show", "1"]);
    assert!(text.contains("pull request: #1 Implementation"));
    assert!(text.contains("branch: feat/implementation"));

    let second_pr = env.ok(&[
        "pr",
        "create",
        "--title",
        "Existing compatible PR",
        "--branch",
        "feat/existing",
    ]);
    assert_eq!(second_pr.trim(), "#2");
    env.ok(&["pr", "link", "2", "2"]);
    env.ok(&["pr", "link", "1", "2"]);
    env.ok(&["pr", "link", "2", "1"]);
    env.ok(&["pr", "link", "1", "1"]);

    let first_issue = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(
        first_issue["pull_requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pr| pr["number"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    let second_issue = json(&env.ok(&["issue", "show", "2", "--json"]));
    assert_eq!(
        second_issue["pull_requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pr| pr["number"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );

    // Unlink removes only the requested pair, leaving both other cardinality
    // directions intact.
    env.ok(&["pr", "unlink", "1", "1"]);
    let first_issue = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(first_issue["pull_requests"].as_array().unwrap().len(), 1);
    assert_eq!(first_issue["pull_requests"][0]["number"], 2);
    assert_eq!(
        json(&env.ok(&["issue", "show", "2", "--json"]))["pull_requests"][0]["number"],
        1
    );

    let missing_pair = env.run(&["pr", "unlink", "1", "1"]);
    assert!(!missing_pair.status.success());
    assert!(String::from_utf8_lossy(&missing_pair.stderr).contains("is not linked"));
    env.ok(&["pr", "link", "1", "1"]);
    env.ok(&["pr", "link", "1", "1"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["pull_requests"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "an identical pair was stored more than once"
    );
}

#[test]
fn linked_pr_create_failure_leaves_no_orphan_and_links_are_repo_local() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Owner"]);
    env.ok(&[
        "pr", "create", "--title", "Owned", "--branch", "owned", "--issue", "1",
    ]);

    let second = env.ok(&[
        "pr",
        "create",
        "--title",
        "Second linked PR",
        "--branch",
        "second",
        "--issue",
        "1",
    ]);
    assert_eq!(second.trim(), "#2");

    let missing_issue = env.run(&[
        "pr",
        "create",
        "--title",
        "Must not exist",
        "--branch",
        "orphan",
        "--issue",
        "999",
    ]);
    assert!(!missing_issue.status.success());
    let prs = json(&env.ok(&["pr", "list", "--state", "all", "--json"]));
    assert_eq!(prs.as_array().unwrap().len(), 2);
    assert_eq!(prs[0]["branch"], "owned");
    assert_eq!(prs[1]["branch"], "second");

    let repo2 = TempDir::new().unwrap();
    git(repo2.path(), &["init", "-q", "-b", "main"]);
    git(repo2.path(), &["config", "user.email", "test@example.com"]);
    git(repo2.path(), &["config", "user.name", "test"]);
    git(
        repo2.path(),
        &["commit", "-q", "--allow-empty", "-m", "init"],
    );
    env.ok_in(
        repo2.path(),
        &[
            "pr", "create", "--title", "Repo two", "--branch", "repo-two",
        ],
    );
    let missing_in_repo_one = env.run(&["pr", "link", "1", "3"]);
    assert!(!missing_in_repo_one.status.success());
    assert!(String::from_utf8_lossy(&missing_in_repo_one.stderr).contains("PR #3 not found"));
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
fn project_overview_aggregates_tallies_across_repositories() {
    let env = Env::new();
    env.ok(&["state", "add", "In Progress", "--type", "started"]);
    env.ok(&["project", "create", "--name", "First outcome"]);
    env.ok(&[
        "issue",
        "create",
        "--title",
        "First task",
        "--state",
        "In Progress",
        "--project",
        "First outcome",
    ]);

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
        &["project", "create", "--name", "Second outcome"],
    );
    env.ok_in(
        repo2.path(),
        &[
            "issue",
            "create",
            "--title",
            "Second task",
            "--project",
            "Second outcome",
        ],
    );

    let all = json(&env.ok_in(repo2.path(), &["project", "list", "--all-repos", "--json"]));
    let projects = all.as_array().unwrap();
    assert_eq!(projects.len(), 2);
    let first = projects
        .iter()
        .find(|project| project["name"] == "First outcome")
        .unwrap();
    let second = projects
        .iter()
        .find(|project| project["name"] == "Second outcome")
        .unwrap();
    assert_eq!(first["tally"]["started"], 1);
    assert_eq!(second["tally"]["unstarted"], 1);
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

#[test]
fn reconstructed_parent_project_sameness_is_enforced() {
    let env = Env::new();
    env.ok(&["project", "create", "--name", "One"]);
    env.ok(&["project", "create", "--name", "Two"]);
    env.ok(&["issue", "create", "--title", "Parent", "--project", "One"]);
    env.ok(&["issue", "create", "--title", "Child", "--project", "Two"]);
    assert!(!env
        .run(&["issue", "parent", "set", "2", "1"])
        .status
        .success());
}

#[test]
fn reconstructed_issue_pr_projection_is_singular() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Owner"]);
    env.ok(&[
        "pr", "create", "--title", "Only", "--branch", "only", "--issue", "1",
    ]);
    let issue = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(issue["pull_request"]["number"], 1);
    assert!(issue.get("pull_requests").is_none());
}
