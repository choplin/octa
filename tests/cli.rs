//! Integration tests driving the built `octa` binary against throwaway git
//! repositories, each with an isolated global store (a per-test XDG data dir).
//! These exercise every primitive plus the worktree-sharing guarantee that is
//! octa's reason to exist.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
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
    leases: RefCell<HashMap<(PathBuf, String), String>>,
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
            leases: RefCell::new(HashMap::new()),
        }
    }

    fn path(&self) -> &Path {
        self.repo.path()
    }

    /// Run octa in `dir`, pointing its global store at this env's XDG dir.
    fn run_raw_in(&self, dir: &Path, args: &[&str]) -> Output {
        Command::new(bin())
            .current_dir(dir)
            .env("XDG_DATA_HOME", self.xdg.path())
            .args(args)
            .output()
            .expect("failed to spawn octa")
    }

    fn run_raw(&self, args: &[&str]) -> Output {
        self.run_raw_in(self.path(), args)
    }

    fn query_stdin(&self, document: &str, args: &[&str]) -> Output {
        let mut child = Command::new(bin())
            .current_dir(self.path())
            .env("XDG_DATA_HOME", self.xdg.path())
            .arg("query")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn octa query");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(document.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn run_in(&self, dir: &Path, args: &[&str]) -> Output {
        let Some(number) = protected_issue_number(args) else {
            return self.run_raw_in(dir, args);
        };
        if args.contains(&"--lease") {
            return self.run_raw_in(dir, args);
        }
        let key = (dir.to_path_buf(), number.to_string());
        let existing = self.leases.borrow().get(&key).cloned();
        let lease = match existing {
            Some(lease) => lease,
            None => {
                let acquired = self.run_raw_in(dir, &["issue", "lock", number]);
                if !acquired.status.success() {
                    return acquired;
                }
                let lease = String::from_utf8(acquired.stdout)
                    .unwrap()
                    .trim()
                    .to_string();
                self.leases.borrow_mut().insert(key, lease.clone());
                lease
            }
        };
        let mut leased_args = args.to_vec();
        leased_args.extend(["--lease", lease.as_str()]);
        self.run_raw_in(dir, &leased_args)
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

    fn lease(&self, number: &str) -> String {
        let lease = String::from_utf8(self.run_raw(&["issue", "lock", number]).stdout)
            .unwrap()
            .trim()
            .to_string();
        self.leases.borrow_mut().insert(
            (self.path().to_path_buf(), number.to_string()),
            lease.clone(),
        );
        lease
    }

    fn run_with_lease(&self, args: &[&str], lease: &str) -> Output {
        let mut leased_args = args.to_vec();
        leased_args.extend(["--lease", lease]);
        self.run(&leased_args)
    }

    fn ok_with_lease(&self, args: &[&str], lease: &str) -> String {
        let output = self.run_with_lease(args, lease);
        assert!(
            output.status.success(),
            "octa {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
}

fn protected_issue_number<'a>(args: &'a [&str]) -> Option<&'a str> {
    match args {
        ["issue", command @ ("set-state" | "set" | "unset" | "add" | "remove"), number, ..]
            if !command.is_empty() =>
        {
            Some(number)
        }
        ["pr", "create", rest @ ..] | ["pr", "add" | "remove", rest @ ..] => rest
            .windows(2)
            .find_map(|pair| (pair[0] == "--issue").then_some(pair[1])),
        _ => None,
    }
}

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s.trim()).unwrap()
}

fn issue_numbers(issues: &serde_json::Value) -> Vec<i64> {
    issues
        .as_array()
        .unwrap()
        .iter()
        .map(|issue| issue["number"].as_i64().unwrap())
        .collect()
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
    for command in [
        "tui", "create", "list", "show", "comment", "set", "unset", "add", "remove",
    ] {
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
fn piped_human_output_is_plain_and_json_stays_machine_readable() {
    let env = Env::new();

    let created = env.ok(&["issue", "create", "--title", "Styled"]);
    assert_eq!(created, "#1\n");
    assert!(!created.contains('\u{1b}'));

    let listed = env.ok(&["issue", "list"]);
    assert!(listed.contains("Styled"));
    assert!(listed.contains("│ Issue"));
    assert!(listed.contains("Title"));
    assert!(!listed.contains('\u{1b}'));

    let raw_json = env.ok(&["issue", "show", "1", "--json"]);
    assert_eq!(json(&raw_json)["title"], "Styled");
    assert!(!raw_json.contains('\u{1b}'));
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
fn issue_list_state_selectors_have_distinct_grammar() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "In Backlog state"]); // #1
    env.ok(&[
        "issue",
        "create",
        "--title",
        "In Todo state",
        "--state",
        "Todo",
    ]); // #2
    env.ok(&[
        "issue",
        "create",
        "--title",
        "In Done state",
        "--state",
        "Done",
    ]); // #3

    for args in [
        vec!["issue", "list", "--json"],
        vec!["issue", "list", "--open", "--json"],
    ] {
        let issues = json(&env.ok(&args));
        assert_eq!(issue_numbers(&issues), vec![1, 2]);
    }

    let closed = json(&env.ok(&["issue", "list", "--closed", "--json"]));
    assert_eq!(issue_numbers(&closed), vec![3]);

    let all = json(&env.ok(&["issue", "list", "--all", "--json"]));
    assert_eq!(issue_numbers(&all), vec![1, 2, 3]);

    let named_backlog = json(&env.ok(&["issue", "list", "--state", "Backlog", "--json"]));
    assert_eq!(issue_numbers(&named_backlog), vec![1]);

    let named_todo = json(&env.ok(&["issue", "list", "--state", "Todo", "--json"]));
    assert_eq!(issue_numbers(&named_todo), vec![2]);

    let selector_pairs = [
        ["--open", "--closed"],
        ["--open", "--all"],
        ["--open", "--state"],
        ["--closed", "--all"],
        ["--closed", "--state"],
        ["--all", "--state"],
    ];
    for [first, second] in selector_pairs {
        let mut args = vec!["issue", "list", first, second];
        if second == "--state" {
            args.push("Todo");
        }
        let output = env.run(&args);
        assert!(!output.status.success(), "{args:?} unexpectedly succeeded");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("cannot be used with"),
            "missing usage conflict for {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn edit_updates_title_and_body() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Old", "--body", "old body"]);
    let lease = env.lease("1");
    env.ok_with_lease(
        &["issue", "set", "1", "--title", "New", "--body", "new body"],
        &lease,
    );

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
    assert_eq!(arr[0]["state"], "Backlog");
    assert_eq!(arr[0]["status_type"], "backlog");
    assert_eq!(arr[0]["priority"], 0);

    let show = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(show["number"], 1);
    assert_eq!(show["title"], "JSON");
    assert_eq!(show["status_type"], "backlog");
    assert_eq!(show["priority"], 0);
    let comments = show["comments"].as_array().expect("comments not an array");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["body"], "a note");
}

#[test]
fn status_type_and_priority_filters_preserve_stable_issue_order() {
    let env = Env::new();
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

    env.ok(&["issue", "set", "1", "--priority", "1"]);
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
        .run(&["config", "state", "create", "Odd", "--type", "unknown"])
        .status
        .success());
}

#[test]
fn project_lifecycle_tally_and_issue_context_roundtrip() {
    let env = Env::new();
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
    assert!(all_text.contains("B/U/S/D/C"), "{all_text}");

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
        .run(&["issue", "set", "1", "--parent", "1"])
        .status
        .success());
    env.ok(&["issue", "set", "2", "--parent", "1"]);
    assert!(!env
        .run(&["issue", "set", "1", "--parent", "2"])
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
    env.ok(&["issue", "set", "2", "--parent", "1"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "2", "--json"]))["project"]["name"],
        "One",
        "a project-less child should retain initial inheritance"
    );

    env.ok(&["issue", "set", "1", "--project", "Two"]);
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

    env.ok(&["issue", "unset", "1", "--project"]);
    env.ok(&["issue", "set", "2", "--project", "Two"]);
    env.ok(&["issue", "unset", "2", "--project"]);

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

    let rejected = env.run(&["issue", "set", "1", "--parent", "2"]);
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
        .run(&["project", "set", "Case", "--name", "456"])
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
        "milestone",
        "create",
        "--project",
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
        "milestone",
        "create",
        "--project",
        "Launch",
        "--name",
        "Alpha",
        "--position",
        "1",
    ]);

    let milestones = json(&env.ok(&["milestone", "list", "--project", "Launch", "--json"]));
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
        "milestone",
        "set",
        "Beta",
        "--project",
        "Launch",
        "--status",
        "completed",
        "--target-date",
        "2026-09-15",
    ]);
    let edited = json(&env.ok(&["milestone", "show", "Beta", "--project", "Launch", "--json"]));
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
        vec!["issue", "set", "1", "--project", "Other"],
        vec!["issue", "unset", "1", "--project"],
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
    env.ok(&["issue", "unset", "1", "--milestone"]);
    env.ok(&["issue", "set", "1", "--project", "Other"]);
}

#[test]
fn milestone_requires_project_context_and_names_are_unambiguous() {
    let env = Env::new();
    env.ok(&["project", "create", "--name", "Launch"]);
    env.ok(&["project", "create", "--name", "Other"]);
    env.ok(&[
        "milestone",
        "create",
        "--project",
        "Launch",
        "--name",
        "Beta",
    ]);
    env.ok(&[
        "milestone",
        "create",
        "--project",
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
        .run(&[
            "milestone",
            "create",
            "--project",
            "Launch",
            "--name",
            "beta"
        ])
        .status
        .success());
    assert!(!env
        .run(&[
            "milestone",
            "create",
            "--project",
            "Launch",
            "--name",
            "123"
        ])
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
    let rejected = env.run(&["issue", "set", "1", "--milestone", "Beta"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("needs a project"));
}

// --- States, dependencies, lock ---------------------------------------------

#[test]
fn custom_state_and_set() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Task"]);

    // Default set is seeded.
    let states = json(&env.ok(&["config", "state", "list", "--json"]));
    let names: Vec<&str> = states
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    for expected in [
        "Backlog",
        "Todo",
        "In Progress",
        "In Review",
        "Done",
        "Canceled",
    ] {
        assert!(
            names.contains(&expected),
            "missing state {expected}: {names:?}"
        );
    }
    assert_eq!(
        names,
        vec![
            "Backlog",
            "Todo",
            "In Progress",
            "In Review",
            "Done",
            "Canceled"
        ],
        "new repositories must be seeded with the default workflow"
    );

    env.ok(&["issue", "set-state", "1", "In Review"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["status_type"],
        "started"
    );

    env.ok(&[
        "config",
        "state",
        "create",
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

    // A duplicate name is refused with an explanation, not a raw SQL error.
    let duplicate = env.run(&["config", "state", "create", "Done", "--type", "completed"]);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already exists"));
}

#[test]
fn seeded_repository_starts_new_issues_in_backlog() {
    let env = Env::new();
    let states = json(&env.ok(&["config", "state", "list", "--json"]));
    let starting: Vec<&str> = states
        .as_array()
        .unwrap()
        .iter()
        .filter(|state| state["is_starting"].as_bool().unwrap())
        .map(|state| state["name"].as_str().unwrap())
        .collect();
    assert_eq!(starting, vec!["Backlog"]);

    env.ok(&["issue", "create", "--title", "Captured"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "Backlog"
    );

    // States carry no stored ordinal; the listing order is derived from status
    // type, so a state added later still lands in its workflow group.
    env.ok(&[
        "config",
        "state",
        "create",
        "Blocked",
        "--type",
        "unstarted",
    ]);
    let states = json(&env.ok(&["config", "state", "list", "--json"]));
    let names: Vec<&str> = states
        .as_array()
        .unwrap()
        .iter()
        .map(|state| state["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "Backlog",
            "Blocked",
            "Todo",
            "In Progress",
            "In Review",
            "Done",
            "Canceled"
        ]
    );
    assert!(
        states.as_array().unwrap()[0].get("position").is_none(),
        "states must not expose a stored ordinal: {states}"
    );
}

#[test]
fn set_default_state_moves_the_starting_flag_and_stays_unique() {
    let env = Env::new();
    env.ok(&["config", "state", "set-default", "Todo"]);

    let states = json(&env.ok(&["config", "state", "list", "--json"]));
    let starting: Vec<&str> = states
        .as_array()
        .unwrap()
        .iter()
        .filter(|state| state["is_starting"].as_bool().unwrap())
        .map(|state| state["name"].as_str().unwrap())
        .collect();
    assert_eq!(starting, vec!["Todo"]);

    env.ok(&["issue", "create", "--title", "Groomed"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "Todo"
    );

    // Creating another starting state replaces the flag rather than adding one.
    env.ok(&[
        "config",
        "state",
        "create",
        "Triage",
        "--type",
        "backlog",
        "--starting",
    ]);
    let states = json(&env.ok(&["config", "state", "list", "--json"]));
    let starting: Vec<&str> = states
        .as_array()
        .unwrap()
        .iter()
        .filter(|state| state["is_starting"].as_bool().unwrap())
        .map(|state| state["name"].as_str().unwrap())
        .collect();
    assert_eq!(starting, vec!["Triage"]);

    // A terminal state cannot receive new issues.
    assert!(!env
        .run(&["config", "state", "set-default", "Done"])
        .status
        .success());
}

#[test]
fn renaming_a_state_carries_its_issues_and_rejects_collisions() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Carried", "--state", "Todo"]);

    env.ok(&["config", "state", "set", "Todo", "--name", "Ready"]);
    let shown = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(shown["state"], "Ready");
    assert_eq!(shown["status_type"], "unstarted");
    assert_eq!(
        issue_numbers(&json(
            &env.ok(&["issue", "list", "--state", "Ready", "--json"])
        )),
        vec![1]
    );

    let collision = env.run(&["config", "state", "set", "Ready", "--name", "Done"]);
    assert!(!collision.status.success());
    assert!(String::from_utf8_lossy(&collision.stderr).contains("already exists"));

    let empty = env.run(&["config", "state", "set", "Ready"]);
    assert!(!empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stderr).contains("nothing to update"));
}

#[test]
fn deleting_a_state_requires_somewhere_for_its_issues_to_go() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Stranded", "--state", "Todo"]);

    // An occupied state needs an explicit destination.
    let occupied = env.run(&["config", "state", "delete", "Todo"]);
    assert!(!occupied.status.success());
    assert!(String::from_utf8_lossy(&occupied.stderr).contains("--move-to"));

    // The starting state is protected until another one takes over.
    let starting = env.run(&["config", "state", "delete", "Backlog", "--move-to", "Todo"]);
    assert!(!starting.status.success());
    assert!(String::from_utf8_lossy(&starting.stderr).contains("set-default"));

    env.ok(&[
        "config",
        "state",
        "delete",
        "Todo",
        "--move-to",
        "In Progress",
    ]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "In Progress"
    );
    let names: Vec<String> = json(&env.ok(&["config", "state", "list", "--json"]))
        .as_array()
        .unwrap()
        .iter()
        .map(|state| state["name"].as_str().unwrap().to_string())
        .collect();
    assert!(!names.contains(&"Todo".to_string()), "{names:?}");

    // An unoccupied state deletes without a destination.
    env.ok(&["config", "state", "delete", "Canceled"]);
}

#[test]
fn dependencies_and_unblocked_query() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Foundation"]); // #1
    env.ok(&["issue", "create", "--title", "Feature"]); // #2
    env.ok(&["issue", "add", "1", "--blocks", "2"]); // #1 blocks #2

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
    env.ok(&["issue", "set-state", "1", "Done"]);
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
fn lease_guards_mutations_and_force_unlock_invalidates_the_old_token() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Contended"]);

    let lease = env.lease("1");
    let words = lease.split('-').collect::<Vec<_>>();
    assert_eq!(words.len(), 3);
    assert!(words.iter().all(
        |word| !word.is_empty() && word.chars().all(|character| character.is_ascii_lowercase())
    ));

    let shown = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(shown["leased"], true);
    assert!(!shown.to_string().contains(&lease));
    let listed = env.ok(&["issue", "list", "--all", "--json"]);
    assert!(!listed.contains(&lease));

    let out = env.run(&["issue", "lock", "1"]);
    assert!(!out.status.success(), "second lock unexpectedly succeeded");
    assert!(String::from_utf8_lossy(&out.stderr).contains("already leased"));

    let missing = env.run_raw(&["issue", "set-state", "1", "Done"]);
    assert!(!missing.status.success());
    let mismatch = env.run_with_lease(&["issue", "set-state", "1", "Done"], "incorrect-lease");
    assert!(!mismatch.status.success());
    env.ok_with_lease(&["issue", "set-state", "1", "Done"], &lease);

    env.ok(&["issue", "unlock", "1", "--force"]);
    let stale = env.run_with_lease(&["issue", "set-state", "1", "Todo"], &lease);
    assert!(!stale.status.success(), "force unlock left old lease valid");

    let replacement = env.lease("1");
    assert_ne!(replacement, lease);
    env.ok(&["issue", "unlock", "1", "--lease", &replacement]);
}

#[test]
fn every_issue_mutation_surface_rejects_missing_and_mismatched_leases() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Guarded"]);
    env.ok(&["issue", "create", "--title", "Peer"]);
    env.ok(&["config", "label", "create", "guarded", "--target", "issue"]);
    env.ok(&[
        "pr",
        "create",
        "--title",
        "Implementation",
        "--branch",
        "guarded",
    ]);
    let lease = env.lease("1");

    let commands = [
        vec!["issue", "set-state", "1", "Done"],
        vec!["issue", "set", "1", "--title", "Changed"],
        vec!["issue", "unset", "1", "--parent"],
        vec!["issue", "add", "1", "--label", "guarded"],
        vec!["issue", "remove", "1", "--label", "guarded"],
        vec!["issue", "add", "1", "--pr", "1"],
        vec!["issue", "remove", "1", "--pr", "1"],
        vec!["pr", "add", "1", "--issue", "1"],
        vec!["pr", "remove", "1", "--issue", "1"],
        vec![
            "pr", "create", "--title", "Linked", "--branch", "linked", "--issue", "1",
        ],
    ];
    for command in commands {
        let missing = env.run_raw(&command);
        assert!(
            !missing.status.success(),
            "missing lease unexpectedly accepted for {command:?}"
        );
        let mismatch = env.run_with_lease(&command, "incorrect-lease");
        assert!(
            !mismatch.status.success(),
            "mismatched lease unexpectedly accepted for {command:?}"
        );
    }

    env.ok_with_lease(&["issue", "set", "1", "--title", "Changed"], &lease);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["title"],
        "Changed"
    );
}

#[test]
fn concurrent_lease_acquisition_has_exactly_one_winner() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Contended"]);
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let barrier = Arc::clone(&barrier);
        let repo = env.path().to_path_buf();
        let xdg = env.xdg.path().to_path_buf();
        workers.push(thread::spawn(move || {
            barrier.wait();
            Command::new(bin())
                .current_dir(repo)
                .env("XDG_DATA_HOME", xdg)
                .args(["issue", "lock", "1"])
                .output()
                .unwrap()
        }));
    }
    barrier.wait();
    let outputs = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1
    );
    assert_eq!(
        outputs
            .iter()
            .filter(|output| !output.status.success())
            .count(),
        1
    );
}

// --- Labels -----------------------------------------------------------------

#[test]
fn single_select_group_is_mutually_exclusive() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Grouped"]);
    env.ok(&[
        "config",
        "label-group",
        "create",
        "delivery",
        "--target",
        "issue",
        "--selection",
        "single",
    ]);
    env.ok(&[
        "config", "label", "create", "alpha", "--target", "issue", "--group", "delivery",
    ]);
    env.ok(&[
        "config", "label", "create", "beta", "--target", "issue", "--group", "delivery",
    ]);

    env.ok(&["issue", "add", "1", "--label", "alpha"]);
    env.ok(&["issue", "add", "1", "--label", "beta"]); // replaces alpha (single group)

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
    env.ok(&[
        "config",
        "label-group",
        "create",
        "Type",
        "--target",
        "issue",
        "--selection",
        "multi",
    ]);
    env.ok(&[
        "config",
        "label",
        "create",
        "arbitrary",
        "--target",
        "issue",
        "--group",
        "Type",
    ]);
    for label in ["impl", "design", "research"] {
        env.ok(&["config", "label", "create", label, "--target", "issue"]);
        env.ok(&["issue", "add", "1", "--label", label]);
    }

    let show = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(
        show["labels"],
        serde_json::json!(["design", "impl", "research"])
    );
    let groups = json(&env.ok(&[
        "config",
        "label-group",
        "list",
        "--target",
        "issue",
        "--json",
    ]));
    assert_eq!(groups[0]["name"], "Type");
    assert_eq!(groups[0]["selection"], "multi");
}

#[test]
fn multi_select_group_labels_coexist() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "Multi"]);
    env.ok(&[
        "config",
        "label-group",
        "create",
        "area",
        "--target",
        "issue",
        "--selection",
        "multi",
    ]);
    env.ok(&[
        "config", "label", "create", "cli", "--target", "issue", "--group", "area",
    ]);
    env.ok(&[
        "config", "label", "create", "storage", "--target", "issue", "--group", "area",
    ]);

    env.ok(&["issue", "add", "1", "--label", "cli"]);
    env.ok(&["issue", "add", "1", "--label", "storage"]);

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
fn project_labels_are_separate_and_single_select_groups_replace_values() {
    let env = Env::new();
    env.ok(&["project", "create", "--name", "Launch"]);
    env.ok(&[
        "config",
        "label-group",
        "create",
        "horizon",
        "--target",
        "project",
        "--selection",
        "single",
    ]);
    for label in ["now", "next"] {
        env.ok(&[
            "config", "label", "create", label, "--target", "project", "--group", "horizon",
        ]);
    }
    env.ok(&["config", "label", "create", "now", "--target", "issue"]);

    env.ok(&["project", "add", "Launch", "--label", "now"]);
    env.ok(&["project", "add", "Launch", "--label", "next"]);

    let project = json(&env.ok(&["project", "show", "Launch", "--json"]));
    assert_eq!(project["labels"], serde_json::json!(["next"]));
    let labels = json(&env.ok(&["config", "label", "list", "--target", "project", "--json"]));
    assert_eq!(labels.as_array().unwrap().len(), 2);

    env.ok(&["project", "remove", "Launch", "--label", "next"]);
    let project = json(&env.ok(&["project", "show", "Launch", "--json"]));
    assert_eq!(project["labels"], serde_json::json!([]));
}

#[test]
fn related_issues_are_symmetric_idempotent_and_filterable() {
    let env = Env::new();
    for title in ["First", "Second", "Third"] {
        env.ok(&["issue", "create", "--title", title]);
    }

    env.ok(&["issue", "add", "2", "--related", "1"]);
    env.ok(&["issue", "add", "1", "--related", "2"]);

    let first = json(&env.ok(&["issue", "show", "1", "--json"]));
    let second = json(&env.ok(&["issue", "show", "2", "--json"]));
    assert_eq!(first["related"], serde_json::json!([2]));
    assert_eq!(second["related"], serde_json::json!([1]));
    assert!(env.ok(&["issue", "show", "1"]).contains("related: #2"));

    let filtered = json(&env.ok(&["issue", "list", "--related-to", "1", "--json"]));
    assert_eq!(filtered.as_array().unwrap().len(), 1);
    assert_eq!(filtered[0]["number"], 2);

    let self_relation = env.run(&["issue", "add", "1", "--related", "1"]);
    assert!(!self_relation.status.success());
    assert!(String::from_utf8_lossy(&self_relation.stderr).contains("itself"));

    env.ok(&["issue", "remove", "2", "--related", "1"]);
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

    env.ok(&["pr", "set-state", "1", "closed"]);
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
    env.ok(&["pr", "add", "2", "--issue", "2"]);
    env.ok(&["pr", "add", "2", "--issue", "1"]);
    env.ok(&["pr", "add", "1", "--issue", "2"]);
    env.ok(&["pr", "add", "1", "--issue", "1"]);

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
    env.ok(&["pr", "remove", "1", "--issue", "1"]);
    let first_issue = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(first_issue["pull_requests"].as_array().unwrap().len(), 1);
    assert_eq!(first_issue["pull_requests"][0]["number"], 2);
    assert_eq!(
        json(&env.ok(&["issue", "show", "2", "--json"]))["pull_requests"][0]["number"],
        1
    );

    let missing_pair = env.run(&["pr", "remove", "1", "--issue", "1"]);
    assert!(!missing_pair.status.success());
    assert!(String::from_utf8_lossy(&missing_pair.stderr).contains("is not linked"));
    env.ok(&["pr", "add", "1", "--issue", "1"]);
    env.ok(&["pr", "add", "1", "--issue", "1"]);
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
    let missing_in_repo_one = env.run(&["pr", "add", "3", "--issue", "1"]);
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
    env.ok(&["issue", "set-state", "1", "Done"]);

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
        &["issue", "list", "--all-repos", "--all", "--json"],
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
        &["issue", "list", "--all-repos", "--closed", "--json"],
    ));
    assert_eq!(closed.as_array().unwrap().len(), 1);
    assert_eq!(closed.as_array().unwrap()[0]["title"], "In first repo");

    for args in [
        vec!["issue", "list", "--all-repos", "--state", "In Progress"],
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
            "--state",
            "Todo",
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
    let out = env.run(&["pr", "set", "1"]);
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
    env.ok(&["wiki", "set", "home", "--body", "[[new]]"]);
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
    env.ok(&["wiki", "set", "home", "--title", "Renamed"]);
    assert_eq!(
        json(&env.ok(&["wiki", "show", "home", "--json"]))["links_to"],
        serde_json::json!(["new"])
    );
}

#[test]
fn label_errors_and_idempotent_operations_are_preserved() {
    let env = Env::new();
    assert!(!env
        .run(&[
            "config",
            "label-group",
            "create",
            "kind",
            "--target",
            "issue",
            "--selection",
            "bad"
        ])
        .status
        .success());
    assert!(!env
        .run(&["config", "label", "create", "x", "--target", "issue", "--group", "missing"])
        .status
        .success());
    env.ok(&["issue", "create", "--title", "Task"]);
    env.ok(&["config", "label", "create", "plain", "--target", "issue"]);
    env.ok(&["issue", "add", "1", "--label", "plain"]);
    env.ok(&["issue", "add", "1", "--label", "plain"]);
    env.ok(&["issue", "remove", "1", "--label", "missing"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["labels"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(!env
        .run(&["issue", "add", "99", "--label", "plain"])
        .status
        .success());
}

#[test]
fn graphql_query_traverses_entities_with_variables_filters_and_pagination() {
    let env = Env::new();
    env.ok(&["project", "create", "--name", "Outcome"]);
    env.ok(&["project", "create", "--name", "Other"]);
    env.ok(&[
        "milestone",
        "create",
        "--project",
        "Outcome",
        "--name",
        "Phase",
    ]);
    env.ok(&[
        "milestone",
        "create",
        "--project",
        "Other",
        "--name",
        "Other phase",
    ]);
    env.ok(&["config", "label", "create", "impl", "--target", "issue"]);
    env.ok(&["config", "label", "create", "docs", "--target", "issue"]);
    env.ok(&["config", "label", "create", "now", "--target", "project"]);
    env.ok(&["config", "label", "create", "next", "--target", "project"]);
    env.ok(&["project", "add", "Outcome", "--label", "now"]);
    env.ok(&["project", "add", "Other", "--label", "next"]);
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Root",
        "--project",
        "Outcome",
        "--milestone",
        "Phase",
    ]);
    env.ok(&["issue", "create", "--title", "Child", "--parent", "1"]);
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Third",
        "--project",
        "Outcome",
    ]);
    env.ok(&["issue", "add", "1", "--label", "impl"]);
    env.ok(&["issue", "add", "3", "--label", "docs"]);
    env.ok(&["issue", "add", "1", "--blocks", "2"]);
    env.ok(&["issue", "add", "1", "--related", "2"]);
    env.ok(&[
        "pr", "create", "--title", "Change", "--branch", "change", "--issue", "1",
    ]);
    env.ok(&["wiki", "create", "--title", "Home", "--body", "[[guide]]"]);
    env.ok(&["wiki", "create", "--title", "Guide", "--slug", "guide"]);

    let document = r#"
          query($number: Int!, $limit: Int!) {
          issue(number: $number) {
            number
            leased
            project { name milestones(limit: $limit) { name } }
            labels { name }
            blocks(limit: $limit) { number leased }
            related(limit: $limit) { number }
            pullRequests { number }
          }
          issues(filter: { projectId: 1, label: "impl" }, limit: $limit) { number leased }
          wikiPage(slug: "home") { linksTo { slug backlinks { slug } } }
        }
    "#;
    let out = env.query_stdin(document, &["--variables", r#"{"number":1,"limit":1}"#]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let response = json(&String::from_utf8(out.stdout).unwrap());
    assert_eq!(response["data"]["issue"]["leased"], true);
    assert_eq!(response["data"]["issue"]["project"]["name"], "Outcome");
    assert_eq!(response["data"]["issue"]["blocks"][0]["number"], 2);
    assert_eq!(response["data"]["issue"]["blocks"][0]["leased"], false);
    assert_eq!(response["data"]["issue"]["related"][0]["number"], 2);
    assert_eq!(response["data"]["issue"]["pullRequests"][0]["number"], 1);
    assert_eq!(response["data"]["issues"].as_array().unwrap().len(), 1);
    assert_eq!(response["data"]["wikiPage"]["linksTo"][0]["slug"], "guide");
    assert!(response["extensions"]["dbAccesses"].as_i64().unwrap() > 0);

    let joined = env.query_stdin(
        "{ issues(filter: { projectId: 1 }) { number project { name } labels { name } } }",
        &[],
    );
    let joined = json(&String::from_utf8(joined.stdout).unwrap());
    assert_eq!(joined["data"]["issues"].as_array().unwrap().len(), 3);
    assert_eq!(joined["extensions"]["dbAccesses"], 1);

    let project_projection = env.query_stdin(
        "{ projects { id issues { number } milestones { name project { id } } labels { name } } }",
        &[],
    );
    let project_projection = json(&String::from_utf8(project_projection.stdout).unwrap());
    assert_eq!(
        project_projection["data"]["projects"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(project_projection["extensions"]["dbAccesses"], 1);

    let relation_projection = env.query_stdin(
        "{ issues { number milestone { name } blocks { number } blockedBy { number } related { number } parent { number } subIssues { number } pullRequests { number } } }",
        &[],
    );
    let relation_projection = json(&String::from_utf8(relation_projection.stdout).unwrap());
    assert_eq!(
        relation_projection["data"]["issues"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(relation_projection["extensions"]["dbAccesses"], 1);

    let label_batch = env.query_stdin(
        "{ issueLabels: labels(target: ISSUE) { name issues { number } } projectLabels: labels(target: PROJECT) { name projects { id } } }",
        &[],
    );
    let label_batch = json(&String::from_utf8(label_batch.stdout).unwrap());
    assert_eq!(
        label_batch["data"]["issueLabels"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        label_batch["data"]["projectLabels"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(label_batch["extensions"]["dbAccesses"], 2);

    let merged = env.query_stdin(
        "query { issue(number: 1) { __typename project { id } project { name } ...IssuePart } } fragment IssuePart on IssueObject { project { summary } }",
        &[],
    );
    let merged = json(&String::from_utf8(merged.stdout).unwrap());
    assert_eq!(merged["data"]["issue"]["__typename"], "IssueObject");
    assert_eq!(merged["data"]["issue"]["project"]["id"], 1);
    assert_eq!(merged["data"]["issue"]["project"]["name"], "Outcome");
    assert!(merged["errors"].is_null());

    let merged_collection = env.query_stdin(
        "{ project(id: 1) { issues { number } issues { title } } }",
        &[],
    );
    let merged_collection = json(&String::from_utf8(merged_collection.stdout).unwrap());
    assert_eq!(
        merged_collection["data"]["project"]["issues"][0]["number"],
        1
    );
    assert_eq!(
        merged_collection["data"]["project"]["issues"][0]["title"],
        "Root"
    );
    assert!(merged_collection["errors"].is_null());

    let label_page = env.query_stdin("{ issue(number: 1) { labels(limit: 1) { name } } }", &[]);
    let label_page = json(&String::from_utf8(label_page.stdout).unwrap());
    assert_eq!(
        label_page["data"]["issue"]["labels"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let label_overflow =
        env.query_stdin("{ issue(number: 1) { labels(limit: 101) { name } } }", &[]);
    assert!(
        json(&String::from_utf8(label_overflow.stdout).unwrap())["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("limit must be between")
    );

    let aliases = (0..64)
        .map(|index| format!("field{index}: number"))
        .collect::<Vec<_>>()
        .join(" ");
    let aliases = env.query_stdin(&format!("{{ issue(number: 1) {{ {aliases} }} }}"), &[]);
    let aliases = json(&String::from_utf8(aliases.stdout).unwrap());
    assert_eq!(aliases["data"]["issue"]["field63"], 1);
    assert!(aliases["errors"].is_null());

    let nulls = env.query_stdin(
        "{ issue(number: 2) { number milestone { id } } milestone(projectId: 1, id: 1) { id startDate targetDate } }",
        &[],
    );
    let nulls = json(&String::from_utf8(nulls.stdout).unwrap());
    assert!(nulls["data"]["issue"]["milestone"].is_null());
    assert!(nulls["data"]["milestone"]["startDate"].is_null());
    assert!(nulls["data"]["milestone"]["targetDate"].is_null());
    assert!(nulls["errors"].is_null());

    env.ok(&["issue", "set", "1", "--title", "123"]);
    env.ok(&["project", "set", "Outcome", "--name", "null"]);
    env.ok(&[
        "milestone",
        "set",
        "Phase",
        "--project",
        "null",
        "--name",
        "true",
    ]);
    let json_like_text = env.query_stdin(
        "{ issue(number: 1) { title project { name milestones { name } } } }",
        &[],
    );
    let json_like_text = json(&String::from_utf8(json_like_text.stdout).unwrap());
    assert_eq!(json_like_text["data"]["issue"]["title"], "123");
    assert_eq!(json_like_text["data"]["issue"]["project"]["name"], "null");
    assert_eq!(
        json_like_text["data"]["issue"]["project"]["milestones"][0]["name"],
        "true"
    );
    assert!(json_like_text["errors"].is_null());

    let page = env.query_stdin("{ issues(offset: 1, limit: 1) { number } }", &[]);
    let page = json(&String::from_utf8(page.stdout).unwrap());
    assert_eq!(page["data"]["issues"], serde_json::json!([{ "number": 2 }]));

    let negative = env.query_stdin("{ issues(offset: -1, limit: 1) { number } }", &[]);
    let negative = json(&String::from_utf8(negative.stdout).unwrap());
    assert!(negative["errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("offset must be non-negative"));
}

#[test]
fn graphql_query_accepts_files_and_enforces_read_only_limits() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "One"]);

    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{{ issue(number: 1) {{ title }} }}").unwrap();
    let path = file.path().to_str().unwrap();
    let response = json(&env.ok(&["query", "--file", path]));
    assert_eq!(response["data"]["issue"]["title"], "One");

    let invalid = env.query_stdin("{ missingField }", &[]);
    assert!(invalid.status.success());
    assert!(!json(&String::from_utf8(invalid.stdout).unwrap())["errors"]
        .as_array()
        .unwrap()
        .is_empty());

    let over_limit = env.query_stdin("{ issues(limit: 101) { number } }", &[]);
    let over_limit = json(&String::from_utf8(over_limit.stdout).unwrap());
    assert!(over_limit["errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("limit must be between"));

    let too_deep = env.query_stdin(
        "{ issue(number: 1) { parent { parent { parent { parent { parent { parent { parent { parent { number } } } } } } } } } }",
        &[],
    );
    assert!(
        !json(&String::from_utf8(too_deep.stdout).unwrap())["errors"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let fields = (0..501)
        .map(|index| format!("field{index}: issues(limit: 1) {{ number }}"))
        .collect::<Vec<_>>()
        .join(" ");
    let too_complex = env.query_stdin(&format!("{{ {fields} }}"), &[]);
    assert!(
        !json(&String::from_utf8(too_complex.stdout).unwrap())["errors"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let schema = env.ok(&["query", "--schema"]);
    assert!(schema.contains("type QueryRoot"));
    assert!(!schema.contains("type Mutation"));
}
