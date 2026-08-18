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
        ["issue", command @ ("start" | "close" | "reopen" | "set" | "unset" | "add" | "remove"), number, ..]
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
        env.ok(&["issue", "open", "--title", "First", "--body", "hello"])
            .trim(),
        "#1"
    );
    // Per-repo sequential numbering.
    assert_eq!(env.ok(&["issue", "open", "--title", "Second"]).trim(), "#2");

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

    let created = env.ok(&["issue", "open", "--title", "Styled"]);
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
    env.ok(&["issue", "open", "--title", "Discuss"]);
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
    env.ok(&["issue", "open", "--title", "In open state"]); // #1
    env.ok(&["issue", "open", "--title", "In progress state"]); // #2
    env.ok(&["issue", "start", "2"]);
    env.ok(&["issue", "open", "--title", "In closed state"]); // #3
    env.ok(&["issue", "close", "3"]);
    env.ok(&["issue", "open", "--title", "In not planned state"]); // #4
    env.ok(&["issue", "close", "4", "--as", "not planned"]);

    // Omitting a selector hides closed work; it is exactly the non-closed types.
    let default = json(&env.ok(&["issue", "list", "--json"]));
    assert_eq!(issue_numbers(&default), vec![1, 2]);
    let non_closed = json(&env.ok(&[
        "issue",
        "list",
        "--state-type",
        "open,in progress",
        "--json",
    ]));
    assert_eq!(issue_numbers(&non_closed), vec![1, 2]);

    let all = json(&env.ok(&["issue", "list", "--all", "--json"]));
    assert_eq!(issue_numbers(&all), vec![1, 2, 3, 4]);

    // One closed type covers both ways an issue ends.
    let closed = json(&env.ok(&["issue", "list", "--state-type", "closed", "--json"]));
    assert_eq!(issue_numbers(&closed), vec![3, 4]);

    let named_open = json(&env.ok(&["issue", "list", "--state", "open", "--json"]));
    assert_eq!(issue_numbers(&named_open), vec![1]);

    let named_pair = json(&env.ok(&["issue", "list", "--state", "open,not planned", "--json"]));
    assert_eq!(issue_numbers(&named_pair), vec![1, 4]);

    let unknown_type = env.run(&["issue", "list", "--state-type", "waiting"]);
    assert!(!unknown_type.status.success());
    assert!(String::from_utf8_lossy(&unknown_type.stderr).contains("unknown state type"));

    let selector_pairs = [
        ["--state", "--state-type"],
        ["--state", "--all"],
        ["--state-type", "--all"],
    ];
    for [first, second] in selector_pairs {
        let mut args = vec!["issue", "list", first];
        if first != "--all" {
            args.push("open");
        }
        args.push(second);
        if second != "--all" {
            args.push("open");
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
    env.ok(&["issue", "open", "--title", "Old", "--body", "old body"]);
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
    let created = json(&env.ok(&["issue", "open", "--title", "JSON", "--json"]));
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

#[test]
fn state_filters_preserve_stable_issue_order() {
    let env = Env::new();
    env.ok(&["issue", "open", "--title", "Fresh"]); // #1
    env.ok(&["issue", "open", "--title", "Also fresh"]); // #2
    env.ok(&["issue", "open", "--title", "Picked up"]); // #3
    env.ok(&["issue", "start", "3"]);
    env.ok(&["issue", "open", "--title", "Still going"]); // #4
    env.ok(&["issue", "start", "4"]);
    env.ok(&["issue", "open", "--title", "Finished"]); // #5
    env.ok(&["issue", "close", "5"]);
    env.ok(&["issue", "open", "--title", "Dropped"]); // #6
    env.ok(&["issue", "close", "6", "--as", "not planned"]);

    let listed = json(&env.ok(&["issue", "list", "--json"]));
    let numbers: Vec<i64> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|issue| issue["number"].as_i64().unwrap())
        .collect();
    // Issue-number order survives filtering; the selector never reorders.
    assert_eq!(numbers, vec![1, 2, 3, 4]);

    let open = json(&env.ok(&["issue", "list", "--state-type", "open", "--json"]));
    assert_eq!(open.as_array().unwrap().len(), 2);
    assert_eq!(open[0]["number"], 1);
    assert_eq!(open[1]["number"], 2);

    let closed = json(&env.ok(&["issue", "list", "--state-type", "closed", "--json"]));
    assert_eq!(closed[0]["number"], 5);
    assert_eq!(closed[1]["number"], 6);
}

#[test]
fn withdrawn_options_and_flag_conflicts_are_rejected() {
    let env = Env::new();
    env.ok(&["issue", "open", "--title", "Plain"]);
    env.ok(&["project", "create", "--name", "Ship"]);

    // Priority left the data model; every surface that used to accept it must
    // now reject the option outright rather than silently ignore it.
    for withdrawn in [
        vec!["issue", "create", "--title", "Bad", "--priority", "2"],
        vec!["issue", "set", "1", "--priority", "1"],
        vec!["issue", "list", "--priority", "1"],
        vec!["project", "create", "--name", "Bad", "--priority", "2"],
        vec!["project", "set", "Ship", "--priority", "2"],
    ] {
        assert!(
            !env.run(&withdrawn).status.success(),
            "{withdrawn:?} should be rejected"
        );
    }

    assert!(!env
        .run(&["issue", "list", "--status-type", "backlog"])
        .status
        .success());
    // The two-flag classification is gone; `--type` replaces both flags and
    // `set-state` folded into `issue set --as`.
    for withdrawn in [
        vec!["config", "issue", "state", "create", "Odd", "--starting"],
        vec!["config", "issue", "state", "create", "Odd", "--closed"],
        vec![
            "config", "issue", "state", "set", "open", "--closed", "true",
        ],
        vec!["issue", "set-state", "1", "closed"],
        vec!["issue", "list", "--open"],
        vec!["issue", "list", "--closed"],
        vec!["issue", "start", "1", "--as", "in progress"],
    ] {
        assert!(
            !env.run(&withdrawn).status.success(),
            "{withdrawn:?} should be rejected"
        );
    }
    // Configuration is split by the record it configures, so `--target` and the
    // target-less spellings it used to disambiguate are gone.
    for withdrawn in [
        vec!["config", "label", "create", "docs", "--target", "issue"],
        vec!["config", "label", "list", "--target", "issue"],
        vec![
            "config",
            "label-group",
            "create",
            "priority",
            "--target",
            "issue",
            "--selection",
            "single",
        ],
        vec!["config", "label-group", "list", "--target", "project"],
        vec!["config", "state", "list"],
        vec![
            "config", "issue", "label", "create", "docs", "--target", "issue",
        ],
    ] {
        assert!(
            !env.run(&withdrawn).status.success(),
            "{withdrawn:?} should be rejected"
        );
    }
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
        "--json",
    ]));
    assert_eq!(created["id"], 1);

    env.ok(&[
        "issue",
        "open",
        "--title",
        "Parent",
        "--project",
        "Ship CLI",
    ]);
    env.ok(&["issue", "open", "--title", "Child", "--parent", "1"]);
    env.ok(&["issue", "start", "2"]);
    env.ok(&["issue", "open", "--title", "Standalone"]);
    env.ok(&[
        "issue",
        "open",
        "--title",
        "Dropped work",
        "--project",
        "Ship CLI",
    ]);
    env.ok(&["issue", "close", "4", "--as", "not planned"]);

    let child = json(&env.ok(&["issue", "show", "2", "--json"]));
    assert_eq!(child["project"]["name"], "Ship CLI");
    assert_eq!(child["parent"]["number"], 1);
    let parent = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(parent["sub_issues"][0]["number"], 2);
    assert!(json(&env.ok(&["issue", "show", "3", "--json"]))["project"].is_null());

    let filtered = json(&env.ok(&["issue", "list", "--project", "Ship CLI", "--json"]));
    assert_eq!(filtered.as_array().unwrap().len(), 2);
    let project = json(&env.ok(&["project", "show", "1", "--json"]));
    // open and in progress are not closed; not planned is.
    assert_eq!(project["tally"]["open"], 2);
    assert_eq!(project["tally"]["closed"], 1);
    assert_eq!(project["tally"]["total"], 3);
    let overview = json(&env.ok(&["project", "list", "--json"]));
    assert_eq!(overview[0]["tally"]["open"], 2);
    assert_eq!(overview[0]["tally"]["closed"], 1);
    assert_eq!(overview[0]["tally"]["total"], 3);

    env.ok(&[
        "config", "project", "state", "create", "shipped", "--type", "closed",
    ]);
    env.ok(&["project", "close", "1", "--as", "shipped"]);
    let listed = json(&env.ok(&["project", "list", "--json"]))[0].clone();
    assert_eq!(listed["state"], "shipped");
    assert_eq!(listed["state_type"], "closed");
    assert!(json(&env.ok(&["project", "list", "--active", "--json"]))
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn project_list_orders_by_id_and_filters_active_explicitly() {
    let env = Env::new();
    for name in ["First", "Second", "Third", "Fourth", "Fifth"] {
        env.ok(&["project", "create", "--name", name]);
    }
    env.ok(&["project", "close", "Second"]);

    // Without priority the default order is creation order within a repository.
    let all = json(&env.ok(&["project", "list", "--json"]));
    assert_eq!(
        all.as_array()
            .unwrap()
            .iter()
            .map(|project| project["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        vec!["First", "Second", "Third", "Fourth", "Fifth"]
    );
    assert_eq!(all[1]["state"], "closed");
    assert_eq!(all[1]["state_type"], "closed");
    let all_text = env.ok(&["project", "list"]);
    assert!(all_text.contains("Second"), "{all_text}");
    assert!(all_text.contains("Open/Closed"), "{all_text}");
    assert!(!all_text.contains("Priority"), "{all_text}");

    let active = json(&env.ok(&["project", "list", "--active", "--json"]));
    assert_eq!(
        active
            .as_array()
            .unwrap()
            .iter()
            .map(|project| project["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        vec!["First", "Third", "Fourth", "Fifth"]
    );
}

#[test]
fn parent_self_and_cycles_are_rejected_while_projects_are_independent() {
    let env = Env::new();
    env.ok(&["project", "create", "--name", "One"]);
    env.ok(&["project", "create", "--name", "Two"]);
    env.ok(&["issue", "open", "--title", "A", "--project", "One"]);
    env.ok(&["issue", "open", "--title", "C", "--project", "Two"]);
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
    env.ok(&["issue", "open", "--title", "Parent", "--project", "One"]);
    env.ok(&["issue", "open", "--title", "Child"]);
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
        "open",
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
        "open",
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
    env.ok(&["issue", "open", "--title", "Repo one child"]);

    let repo2 = TempDir::new().unwrap();
    git(repo2.path(), &["init", "-q", "-b", "main"]);
    git(repo2.path(), &["config", "user.email", "test@example.com"]);
    git(repo2.path(), &["config", "user.name", "test"]);
    git(
        repo2.path(),
        &["commit", "-q", "--allow-empty", "-m", "init"],
    );
    env.ok_in(repo2.path(), &["issue", "open", "--title", "Repo two one"]);
    env.ok_in(repo2.path(), &["issue", "open", "--title", "Repo two two"]);

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
        "open",
        "--title",
        "Invite users",
        "--project",
        "Launch",
        "--milestone",
        "bEtA",
    ]);
    env.ok(&[
        "issue",
        "open",
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
        "open",
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
    env.ok(&["issue", "open", "--title", "Standalone"]);
    let rejected = env.run(&["issue", "set", "1", "--milestone", "Beta"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("needs a project"));
}

// --- States, dependencies, lock ---------------------------------------------

#[test]
fn seeded_states_carry_a_type_and_a_default_per_type() {
    let env = Env::new();

    let states = json(&env.ok(&["config", "issue", "state", "list", "--json"]));
    let rows: Vec<(&str, &str, bool)> = states
        .as_array()
        .unwrap()
        .iter()
        .map(|state| {
            (
                state["name"].as_str().unwrap(),
                state["type"].as_str().unwrap(),
                state["is_default"].as_bool().unwrap(),
            )
        })
        .collect();
    // Listing order is derived, not stored: by type in lifecycle order, then
    // the type's default, then by name. `not planned` is a closing *reason*,
    // so it shares the closed type rather than extending the axis.
    assert_eq!(
        rows,
        vec![
            ("open", "open", true),
            ("in progress", "in progress", true),
            ("closed", "closed", true),
            ("not planned", "closed", false),
        ],
        "new repositories must be seeded with the default state set"
    );
    assert!(
        states.as_array().unwrap()[0].get("position").is_none(),
        "states must not expose a stored ordinal: {states}"
    );

    let rendered = env.ok(&["config", "issue", "state", "list"]);
    for expected in ["Type", "Default", "in progress"] {
        assert!(
            rendered.contains(expected),
            "missing {expected}:\n{rendered}"
        );
    }
    assert!(
        !rendered.contains("starting"),
        "the listing must not reintroduce the withdrawn starting flag:\n{rendered}"
    );

    // A state added later lands in its type's group without any reordering.
    env.ok(&[
        "config", "issue", "state", "create", "blocked", "--type", "open",
    ]);
    let names: Vec<String> = json(&env.ok(&["config", "issue", "state", "list", "--json"]))
        .as_array()
        .unwrap()
        .iter()
        .map(|state| state["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["open", "blocked", "in progress", "closed", "not planned"]
    );

    let duplicate = env.run(&[
        "config", "issue", "state", "create", "closed", "--type", "closed",
    ]);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already exists"));

    let unknown_type = env.run(&[
        "config", "issue", "state", "create", "odd", "--type", "waiting",
    ]);
    assert!(!unknown_type.status.success());
    assert!(String::from_utf8_lossy(&unknown_type.stderr).contains("unknown state type"));
}

#[test]
fn issue_verbs_move_to_their_type_default_and_reject_the_wrong_type() {
    let env = Env::new();
    env.ok(&["issue", "open", "--title", "Task"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "open",
        "a new issue lands in the open type's default"
    );
    let lease = env.lease("1");

    env.ok_with_lease(&["issue", "start", "1"], &lease);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "in progress"
    );

    env.ok_with_lease(&["issue", "close", "1"], &lease);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "closed"
    );

    env.ok_with_lease(&["issue", "reopen", "1"], &lease);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "open"
    );

    // `--as` reaches the type's other states.
    env.ok_with_lease(&["issue", "close", "1", "--as", "not planned"], &lease);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "not planned"
    );

    // ...and only those. A verb naming the outcome is not a back door.
    let wrong_type = env.run_with_lease(&["issue", "reopen", "1", "--as", "in progress"], &lease);
    assert!(!wrong_type.status.success());
    let message = String::from_utf8_lossy(&wrong_type.stderr);
    assert!(message.contains("not \"open\""), "{message}");
    assert!(message.contains("available: open"), "{message}");

    // `issue set --as` is the unconstrained move, and reaches any state.
    env.ok(&[
        "config", "issue", "state", "create", "blocked", "--type", "open",
    ]);
    env.ok_with_lease(&["issue", "set", "1", "--as", "blocked"], &lease);
    assert!(env.ok(&["issue", "show", "1"]).contains("blocked"));

    assert!(!env
        .run_with_lease(&["issue", "set", "1", "--as", "nonsense"], &lease)
        .status
        .success());
}

#[test]
fn create_is_an_alias_of_open_and_shares_its_narrowing() {
    let env = Env::new();
    env.ok(&[
        "config", "issue", "state", "create", "triage", "--type", "open",
    ]);

    env.ok(&["issue", "create", "--title", "Captured", "--as", "triage"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "triage"
    );
    env.ok(&["issue", "open", "--title", "Also captured"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "2", "--json"]))["state"],
        "open"
    );

    // `open --as` is narrowed to the open type like every other verb. Capturing
    // work that is already underway is `open` then `start`.
    for verb in ["create", "open"] {
        let wrong_type = env.run(&["issue", verb, "--title", "Done already", "--as", "closed"]);
        assert!(
            !wrong_type.status.success(),
            "{verb} --as accepted a closed state"
        );
        assert!(String::from_utf8_lossy(&wrong_type.stderr).contains("not \"open\""));
    }
}

#[test]
fn set_default_state_moves_the_flag_within_one_type_only() {
    let env = Env::new();
    env.ok(&[
        "config", "issue", "state", "create", "triage", "--type", "open",
    ]);
    env.ok(&["config", "issue", "state", "set", "triage", "--default"]);

    let defaults: Vec<(String, String)> =
        json(&env.ok(&["config", "issue", "state", "list", "--json"]))
            .as_array()
            .unwrap()
            .iter()
            .filter(|state| state["is_default"].as_bool().unwrap())
            .map(|state| {
                (
                    state["name"].as_str().unwrap().to_string(),
                    state["type"].as_str().unwrap().to_string(),
                )
            })
            .collect();
    // Moving the open default leaves the other types' defaults alone.
    assert_eq!(
        defaults,
        vec![
            ("triage".to_string(), "open".to_string()),
            ("in progress".to_string(), "in progress".to_string()),
            ("closed".to_string(), "closed".to_string()),
        ]
    );

    env.ok(&["issue", "open", "--title", "Captured"]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "triage"
    );

    // Creating a state as its type's default replaces the flag, never adds one.
    env.ok(&[
        "config",
        "issue",
        "state",
        "create",
        "done",
        "--type",
        "closed",
        "--default",
    ]);
    let closed_defaults: Vec<String> =
        json(&env.ok(&["config", "issue", "state", "list", "--json"]))
            .as_array()
            .unwrap()
            .iter()
            .filter(|state| state["type"] == "closed" && state["is_default"].as_bool().unwrap())
            .map(|state| state["name"].as_str().unwrap().to_string())
            .collect();
    assert_eq!(closed_defaults, vec!["done".to_string()]);
    let lease = env.lease("1");
    env.ok_with_lease(&["issue", "close", "1"], &lease);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "done"
    );
}

#[test]
fn a_populated_type_always_has_exactly_one_default() {
    let env = Env::new();

    // A second state of a populated type does not take the flag unasked.
    env.ok(&[
        "config", "issue", "state", "create", "triage", "--type", "open",
    ]);
    let defaults = |env: &Env, state_type: &str| -> Vec<String> {
        json(&env.ok(&["config", "issue", "state", "list", "--json"]))
            .as_array()
            .unwrap()
            .iter()
            .filter(|state| state["type"] == state_type && state["is_default"] == true)
            .map(|state| state["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(defaults(&env, "open"), vec!["open".to_string()]);

    // Moving a state into an empty type makes it that type's default, the same
    // way creating one there does.
    env.ok(&["config", "issue", "state", "delete", "in progress"]);
    assert!(defaults(&env, "in progress").is_empty());
    let updated = env.ok(&[
        "config",
        "issue",
        "state",
        "set",
        "triage",
        "--type",
        "in progress",
    ]);
    assert!(
        updated.contains("now that type's default"),
        "the promotion must not be silent: {updated}"
    );
    assert_eq!(defaults(&env, "in progress"), vec!["triage".to_string()]);
    assert_eq!(defaults(&env, "open"), vec!["open".to_string()]);
}

#[test]
fn required_types_cannot_be_emptied_and_defaults_cannot_be_dropped() {
    let env = Env::new();

    // open and closed must stay populated; every issue has to be able to start
    // and to end.
    let only_open = env.run(&["config", "issue", "state", "delete", "open"]);
    assert!(!only_open.status.success());
    assert!(String::from_utf8_lossy(&only_open.stderr).contains("only \"open\" state"));

    // With a second open state, the default flag becomes the thing in the way.
    env.ok(&[
        "config", "issue", "state", "create", "triage", "--type", "open",
    ]);
    let default_delete = env.run(&["config", "issue", "state", "delete", "open"]);
    assert!(!default_delete.status.success());
    assert!(String::from_utf8_lossy(&default_delete.stderr).contains("--default"));

    env.ok(&["config", "issue", "state", "set", "triage", "--default"]);
    env.ok(&["config", "issue", "state", "delete", "open"]);

    // Retyping is guarded the same way: closed cannot be emptied either.
    env.ok(&["config", "issue", "state", "delete", "not planned"]);
    let only_closed = env.run(&[
        "config", "issue", "state", "set", "closed", "--type", "open",
    ]);
    assert!(!only_closed.status.success());

    // in progress may be emptied: a workflow that never distinguishes
    // picked-up work is coherent.
    env.ok(&["config", "issue", "state", "delete", "in progress"]);
    let states = json(&env.ok(&["config", "issue", "state", "list", "--json"]));
    assert!(!states
        .as_array()
        .unwrap()
        .iter()
        .any(|state| state["type"] == "in progress"));
    // ...but then `start` has nowhere to go, and says so.
    env.ok(&["issue", "open", "--title", "Task"]);
    let lease = env.lease("1");
    let empty_type = env.run_with_lease(&["issue", "start", "1"], &lease);
    assert!(!empty_type.status.success());
    let message = String::from_utf8_lossy(&empty_type.stderr);
    assert!(
        message.contains("no \"in progress\" state is configured"),
        "{message}"
    );

    // Refilling the type restores `start` without a second command: the first
    // state of an empty type is that type's default.
    let created = env.ok(&[
        "config",
        "issue",
        "state",
        "create",
        "wip",
        "--type",
        "in progress",
    ]);
    assert!(
        created.contains("now that type's default"),
        "the promotion must not be silent: {created}"
    );
    env.ok_with_lease(&["issue", "start", "1"], &lease);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "wip"
    );
}

#[test]
fn renaming_a_state_carries_its_issues_and_rejects_collisions() {
    let env = Env::new();
    env.ok(&["issue", "open", "--title", "Carried"]);

    env.ok(&["config", "issue", "state", "set", "open", "--name", "Ready"]);
    let shown = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(shown["state"], "Ready");
    assert_eq!(
        issue_numbers(&json(
            &env.ok(&["issue", "list", "--state", "Ready", "--json"])
        )),
        vec![1]
    );

    let collision = env.run(&[
        "config", "issue", "state", "set", "Ready", "--name", "closed",
    ]);
    assert!(!collision.status.success());
    assert!(String::from_utf8_lossy(&collision.stderr).contains("already exists"));

    let empty = env.run(&["config", "issue", "state", "set", "Ready"]);
    assert!(!empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stderr).contains("nothing to update"));

    // Retyping a state moves the issues in it across the axis with it.
    env.ok(&[
        "config", "issue", "state", "create", "triage", "--type", "open",
    ]);
    env.ok(&["config", "issue", "state", "set", "triage", "--default"]);
    env.ok(&[
        "config",
        "issue",
        "state",
        "set",
        "Ready",
        "--type",
        "in progress",
    ]);
    assert_eq!(
        issue_numbers(&json(&env.ok(&[
            "issue",
            "list",
            "--state-type",
            "in progress",
            "--json"
        ]))),
        vec![1]
    );
}

#[test]
fn deleting_a_state_requires_somewhere_for_its_issues_to_go() {
    let env = Env::new();
    env.ok(&["issue", "open", "--title", "Stranded"]);
    env.ok(&["issue", "close", "1", "--as", "not planned"]);

    // An occupied state needs an explicit destination.
    let occupied = env.run(&["config", "issue", "state", "delete", "not planned"]);
    assert!(!occupied.status.success());
    assert!(String::from_utf8_lossy(&occupied.stderr).contains("--move-to"));

    // A type's default is protected while that type has somewhere else to
    // point, so the guard fires before the occupancy question is even asked.
    let default = env.run(&[
        "config",
        "issue",
        "state",
        "delete",
        "closed",
        "--move-to",
        "not planned",
    ]);
    assert!(!default.status.success());
    assert!(String::from_utf8_lossy(&default.stderr).contains("--default"));

    env.ok(&[
        "config",
        "issue",
        "state",
        "delete",
        "not planned",
        "--move-to",
        "in progress",
    ]);
    assert_eq!(
        json(&env.ok(&["issue", "show", "1", "--json"]))["state"],
        "in progress"
    );
    let names: Vec<String> = json(&env.ok(&["config", "issue", "state", "list", "--json"]))
        .as_array()
        .unwrap()
        .iter()
        .map(|state| state["name"].as_str().unwrap().to_string())
        .collect();
    assert!(!names.contains(&"not planned".to_string()), "{names:?}");

    // An unoccupied non-default state deletes without a destination.
    env.ok(&[
        "config", "issue", "state", "create", "blocked", "--type", "open",
    ]);
    env.ok(&["config", "issue", "state", "delete", "blocked"]);
}

#[test]
fn dependencies_and_unblocked_query() {
    let env = Env::new();
    env.ok(&["issue", "open", "--title", "Foundation"]); // #1
    env.ok(&["issue", "open", "--title", "Feature"]); // #2
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
fn lease_guards_mutations_and_force_unlock_invalidates_the_old_token() {
    let env = Env::new();
    env.ok(&["issue", "open", "--title", "Contended"]);

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

    let missing = env.run_raw(&["issue", "close", "1"]);
    assert!(!missing.status.success());
    let mismatch = env.run_with_lease(&["issue", "close", "1"], "incorrect-lease");
    assert!(!mismatch.status.success());
    env.ok_with_lease(&["issue", "close", "1"], &lease);

    env.ok(&["issue", "unlock", "1", "--force"]);
    let stale = env.run_with_lease(&["issue", "reopen", "1"], &lease);
    assert!(!stale.status.success(), "force unlock left old lease valid");

    let replacement = env.lease("1");
    assert_ne!(replacement, lease);
    env.ok(&["issue", "unlock", "1", "--lease", &replacement]);
}

#[test]
fn every_issue_mutation_surface_rejects_missing_and_mismatched_leases() {
    let env = Env::new();
    env.ok(&["issue", "open", "--title", "Guarded"]);
    env.ok(&["issue", "open", "--title", "Peer"]);
    env.ok(&["config", "issue", "label", "create", "guarded"]);
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
        vec!["issue", "close", "1"],
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
    env.ok(&["issue", "open", "--title", "Contended"]);
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
    env.ok(&["issue", "open", "--title", "Grouped"]);
    env.ok(&[
        "config",
        "issue",
        "label-group",
        "create",
        "delivery",
        "--selection",
        "single",
    ]);
    env.ok(&[
        "config", "issue", "label", "create", "alpha", "--group", "delivery",
    ]);
    env.ok(&[
        "config", "issue", "label", "create", "beta", "--group", "delivery",
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
    env.ok(&["issue", "open", "--title", "Opaque labels"]);
    env.ok(&[
        "config",
        "issue",
        "label-group",
        "create",
        "Type",
        "--selection",
        "multi",
    ]);
    env.ok(&[
        "config",
        "issue",
        "label",
        "create",
        "arbitrary",
        "--group",
        "Type",
    ]);
    for label in ["impl", "design", "research"] {
        env.ok(&["config", "issue", "label", "create", label]);
        env.ok(&["issue", "add", "1", "--label", label]);
    }

    let show = json(&env.ok(&["issue", "show", "1", "--json"]));
    assert_eq!(
        show["labels"],
        serde_json::json!(["design", "impl", "research"])
    );
    let groups = json(&env.ok(&["config", "issue", "label-group", "list", "--json"]));
    assert_eq!(groups[0]["name"], "Type");
    assert_eq!(groups[0]["selection"], "multi");
}

#[test]
fn multi_select_group_labels_coexist() {
    let env = Env::new();
    env.ok(&["issue", "open", "--title", "Multi"]);
    env.ok(&[
        "config",
        "issue",
        "label-group",
        "create",
        "area",
        "--selection",
        "multi",
    ]);
    env.ok(&[
        "config", "issue", "label", "create", "cli", "--group", "area",
    ]);
    env.ok(&[
        "config", "issue", "label", "create", "storage", "--group", "area",
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
        "project",
        "label-group",
        "create",
        "horizon",
        "--selection",
        "single",
    ]);
    for label in ["now", "next"] {
        env.ok(&[
            "config", "project", "label", "create", label, "--group", "horizon",
        ]);
    }
    env.ok(&["config", "issue", "label", "create", "now"]);

    env.ok(&["project", "add", "Launch", "--label", "now"]);
    env.ok(&["project", "add", "Launch", "--label", "next"]);

    let project = json(&env.ok(&["project", "show", "Launch", "--json"]));
    assert_eq!(project["labels"], serde_json::json!(["next"]));
    let labels = json(&env.ok(&["config", "project", "label", "list", "--json"]));
    assert_eq!(labels.as_array().unwrap().len(), 2);

    env.ok(&["project", "remove", "Launch", "--label", "next"]);
    let project = json(&env.ok(&["project", "show", "Launch", "--json"]));
    assert_eq!(project["labels"], serde_json::json!([]));
}

#[test]
fn related_issues_are_symmetric_idempotent_and_filterable() {
    let env = Env::new();
    for title in ["First", "Second", "Third"] {
        env.ok(&["issue", "open", "--title", title]);
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
    env.ok(&["issue", "open", "--title", "Implement"]);
    env.ok(&["issue", "open", "--title", "Other"]);

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
    env.ok(&["issue", "open", "--title", "Owner"]);
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
    env.ok(&["issue", "open", "--title", "Shared", "--json"]);

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
    env.ok(&["issue", "open", "--title", "In first repo"]);
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
        &["issue", "open", "--title", "In second repo"],
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

    // States are global configuration, so both state selectors work across
    // repositories; only the repository-scoped filters need a single repo.
    let closed = json(&env.ok_in(
        repo2.path(),
        &[
            "issue",
            "list",
            "--all-repos",
            "--state-type",
            "closed",
            "--json",
        ],
    ));
    assert_eq!(closed.as_array().unwrap().len(), 1);
    assert_eq!(closed.as_array().unwrap()[0]["title"], "In first repo");

    let named = json(&env.ok_in(
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
    assert_eq!(named.as_array().unwrap().len(), 1);

    for args in [
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
        "open",
        "--title",
        "First task",
        "--project",
        "First outcome",
    ]);
    env.ok(&["issue", "start", "1"]);

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
            "open",
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
    assert_eq!(first["tally"]["open"], 1);
    assert_eq!(second["tally"]["open"], 1);
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
            "issue",
            "label-group",
            "create",
            "kind",
            "--selection",
            "bad"
        ])
        .status
        .success());
    let missing_group = env.run(&[
        "config", "issue", "label", "create", "x", "--group", "missing",
    ]);
    assert!(!missing_group.status.success());
    // Error hints name commands that exist. A hint is the only place a withdrawn
    // spelling can survive, since no compiler or command parse reaches it.
    let hint = String::from_utf8_lossy(&missing_group.stderr).to_string();
    assert!(
        hint.contains("octa config issue label-group create"),
        "hint does not name the current command: {hint}"
    );
    env.ok(&["issue", "open", "--title", "Task"]);
    env.ok(&["config", "issue", "label", "create", "plain"]);
    env.ok(&["issue", "add", "1", "--label", "plain"]);
    env.ok(&["issue", "add", "1", "--label", "plain"]);
    let unknown_label = env.run(&["issue", "add", "1", "--label", "absent"]);
    assert!(!unknown_label.status.success());
    let hint = String::from_utf8_lossy(&unknown_label.stderr).to_string();
    assert!(
        hint.contains("octa config issue label create"),
        "hint does not name the current command: {hint}"
    );
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
    env.ok(&["config", "issue", "label", "create", "impl"]);
    env.ok(&["config", "issue", "label", "create", "docs"]);
    env.ok(&["config", "project", "label", "create", "now"]);
    env.ok(&["config", "project", "label", "create", "next"]);
    env.ok(&["project", "add", "Outcome", "--label", "now"]);
    env.ok(&["project", "add", "Other", "--label", "next"]);
    env.ok(&[
        "issue",
        "open",
        "--title",
        "Root",
        "--project",
        "Outcome",
        "--milestone",
        "Phase",
    ]);
    env.ok(&["issue", "open", "--title", "Child", "--parent", "1"]);
    env.ok(&["issue", "open", "--title", "Third", "--project", "Outcome"]);
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
    env.ok(&["issue", "open", "--title", "One"]);

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
