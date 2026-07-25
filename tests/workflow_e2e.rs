//! One coherent acceptance journey for repository-local collaboration.
//!
//! The narrower tests in `cli.rs` diagnose individual primitives. This test
//! deliberately keeps one repository, one project, and one issue alive through
//! capture, dependency resolution, handoff, review, and completion so contract
//! drift between otherwise-correct primitives is caught.

use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_octa")
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(path: &Path) {
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "user.email", "workflow@example.com"]);
    git(path, &["config", "user.name", "workflow-test"]);
    git(path, &["commit", "-q", "--allow-empty", "-m", "init"]);
}

struct Env {
    repo: TempDir,
    xdg: TempDir,
}

impl Env {
    fn new() -> Self {
        let repo = TempDir::new().unwrap();
        init_repo(repo.path());
        Self {
            repo,
            xdg: TempDir::new().unwrap(),
        }
    }

    fn path(&self) -> &Path {
        self.repo.path()
    }

    fn command_in(&self, dir: &Path) -> Command {
        let mut command = Command::new(bin());
        command
            .current_dir(dir)
            .env("XDG_DATA_HOME", self.xdg.path());
        command
    }

    fn run_in(&self, dir: &Path, args: &[&str]) -> Output {
        self.command_in(dir)
            .args(args)
            .output()
            .expect("failed to spawn octa")
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_in(self.path(), args)
    }

    fn ok_in(&self, dir: &Path, args: &[&str]) -> String {
        let output = self.run_in(dir, args);
        assert!(
            output.status.success(),
            "octa {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        self.ok_in(self.path(), args)
    }

    fn json(&self, args: &[&str]) -> Value {
        serde_json::from_str(self.ok(args).trim()).unwrap()
    }
}

fn numbers_at(value: &Value, path: &[&str]) -> Vec<i64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            let mut current = item;
            for field in path {
                current = &current[*field];
            }
            current.as_i64().unwrap()
        })
        .collect()
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("failed to poll child") {
            return Some(status);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn repository_local_collaboration_runs_end_to_end_without_losing_context() {
    let env = Env::new();

    // Labels and their grouping are repository-local, user-defined data.
    env.ok(&["label", "group", "Workstream", "--selection", "single"]);
    for label in ["client", "documentation", "service"] {
        env.ok(&["label", "create", label, "--group", "Workstream"]);
    }
    let groups = env.json(&["label", "groups", "--json"]);
    assert_eq!(groups[0]["name"], "Workstream");
    assert_eq!(groups[0]["selection"], "single");
    let labels = env.json(&["label", "list", "--json"]);
    assert_eq!(
        labels
            .as_array()
            .unwrap()
            .iter()
            .map(|label| label["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["client", "documentation", "service"]
    );

    env.ok(&[
        "project",
        "create",
        "--name",
        "Workflow parity",
        "--summary",
        "Provide repository-local collaboration",
        "--description",
        "A finite CLI outcome.",
        "--priority",
        "2",
    ]);
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
        "project",
        "milestone",
        "create",
        "Workflow parity",
        "--name",
        "CLI beta",
        "--description",
        "First usable phase",
        "--status",
        "active",
        "--position",
        "1",
    ]);

    // #1 is ready foundation work. #2 is the deliberately rough Backlog
    // capture. #3 and #4 exercise related and unrelated follow-up data.
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Foundation",
        "--body",
        "Prepare the prerequisite.",
        "--state",
        "Todo",
        "--priority",
        "2",
        "--project",
        "Workflow parity",
        "--milestone",
        "CLI beta",
    ]);
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Read-only issue browser",
        "--body",
        "Rough capture",
        "--state",
        "Backlog",
        "--priority",
        "1",
        "--project",
        "Workflow parity",
        "--milestone",
        "CLI beta",
    ]);
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Related follow-up",
        "--state",
        "Todo",
        "--priority",
        "3",
        "--project",
        "Workflow parity",
        "--milestone",
        "CLI beta",
    ]);
    env.ok(&[
        "issue",
        "create",
        "--title",
        "Fallback follow-up",
        "--state",
        "Todo",
        "--priority",
        "1",
    ]);
    env.ok(&["issue", "dep", "add", "1", "2"]);
    env.ok(&["issue", "relate", "add", "2", "3"]);

    let unblocked_before = env.json(&[
        "issue",
        "list",
        "--state",
        "all",
        "--project",
        "Workflow parity",
        "--unblocked",
        "--json",
    ]);
    assert_eq!(numbers_at(&unblocked_before, &["number"]), vec![1, 3]);

    env.ok(&[
        "issue",
        "comment",
        "1",
        "--body",
        "Foundation shipped as planned.",
    ]);
    env.ok(&["issue", "set-state", "1", "Done"]);
    let unblocked_after = env.json(&[
        "issue",
        "list",
        "--state",
        "all",
        "--project",
        "Workflow parity",
        "--unblocked",
        "--json",
    ]);
    assert_eq!(numbers_at(&unblocked_after, &["number"]), vec![2, 3]);

    let groomed_body = "What & why: provide the read-only browser.\n\
Where: src/tui and issue CLI entry point.\n\
Acceptance: list and complete detail are visible; q exits cleanly.\n\
Constraints: view-only; no mutation keys.";
    env.ok(&[
        "issue",
        "edit",
        "2",
        "--body",
        groomed_body,
        "--priority",
        "1",
    ]);
    env.ok(&["issue", "label", "2", "client"]);
    env.ok(&["issue", "set-state", "2", "Todo"]);

    // The generic detail projection retains the complete issue context.
    let target = env.json(&["issue", "show", "2", "--json"]);
    assert_eq!(target["body"], groomed_body);
    assert_eq!(target["labels"], serde_json::json!(["client"]));
    assert_eq!(target["project"]["name"], "Workflow parity");
    assert_eq!(target["milestone"]["name"], "CLI beta");
    assert_eq!(target["blocked_by"], serde_json::json!([1]));

    env.ok(&["issue", "lock", "2", "--as", "agent-a"]);
    let contended = env.run(&["issue", "lock", "2", "--as", "agent-b"]);
    assert!(!contended.status.success());
    assert!(String::from_utf8_lossy(&contended.stderr).contains("agent-a"));
    env.ok(&["issue", "set-state", "2", "In Progress"]);

    // A handoff is append-only and must not change the active state or lock.
    env.ok(&[
        "issue",
        "comment",
        "2",
        "--body",
        "Handoff: rendering is wired; next run the PTY smoke test.",
    ]);
    let handed_off = env.json(&["issue", "show", "2", "--json"]);
    assert_eq!(handed_off["state"], "In Progress");
    assert_eq!(handed_off["locked_by"], "agent-a");
    assert_eq!(
        handed_off["comments"][0]["body"],
        "Handoff: rendering is wired; next run the PTY smoke test."
    );

    // Linked worktrees resolve to the same repository record and store.
    let worktree_parent = TempDir::new().unwrap();
    let worktree: PathBuf = worktree_parent.path().join("linked");
    git(
        env.path(),
        &[
            "worktree",
            "add",
            "-q",
            worktree.to_str().unwrap(),
            "-b",
            "workflow-e2e",
        ],
    );
    let worktree_view: Value = serde_json::from_str(
        env.ok_in(&worktree, &["issue", "show", "2", "--json"])
            .trim(),
    )
    .unwrap();
    assert_eq!(worktree_view["state"], "In Progress");
    assert_eq!(worktree_view["locked_by"], "agent-a");
    assert_eq!(worktree_view["comments"], handed_off["comments"]);

    // PR creation and link are one transaction, and an Issue may carry
    // multiple implementation PRs without replacing an earlier link.
    env.ok(&[
        "pr",
        "create",
        "--title",
        "Read-only issue browser",
        "--branch",
        "feat/issue-browser",
        "--body",
        "Implements the groomed deliverable.",
        "--issue",
        "2",
    ]);
    env.ok(&[
        "pr",
        "create",
        "--title",
        "Follow-up issue browser fixes",
        "--branch",
        "feat/issue-browser-follow-up",
        "--issue",
        "2",
    ]);
    let prs = env.json(&["pr", "list", "--state", "all", "--json"]);
    assert_eq!(prs.as_array().unwrap().len(), 2);
    assert_eq!(prs[0]["branch"], "feat/issue-browser");
    assert_eq!(prs[1]["branch"], "feat/issue-browser-follow-up");

    // Text and JSON expose the same complete resume projection.
    let detail_json = env.json(&["issue", "show", "2", "--json"]);
    assert_eq!(detail_json["milestone"]["name"], "CLI beta");
    assert_eq!(detail_json["blocked_by"], serde_json::json!([1]));
    assert_eq!(detail_json["related"], serde_json::json!([3]));
    assert_eq!(
        detail_json["pull_requests"][0]["branch"],
        "feat/issue-browser"
    );
    assert_eq!(
        detail_json["pull_requests"][1]["branch"],
        "feat/issue-browser-follow-up"
    );
    assert_eq!(detail_json["comments"].as_array().unwrap().len(), 1);
    assert_eq!(detail_json["locked_by"], "agent-a");
    let detail_text = env.ok(&["issue", "show", "2"]);
    for expected in [
        "milestone: CLI beta",
        "locked by: agent-a",
        "labels: client",
        "blocked by: #1",
        "related: #3",
        "pull request: #1 Read-only issue browser",
        "branch: feat/issue-browser",
        "pull request: #2 Follow-up issue browser fixes",
        "branch: feat/issue-browser-follow-up",
        "Handoff: rendering is wired",
    ] {
        assert!(
            detail_text.contains(expected),
            "text projection missing {expected:?}:\n{detail_text}"
        );
    }

    env.ok(&[
        "issue",
        "comment",
        "2",
        "--body",
        "Completion: followed the groomed plan without deviation; PR is ready.",
    ]);
    env.ok(&["issue", "set-state", "2", "In Review"]);
    let reviewing = env.json(&["issue", "show", "2", "--json"]);
    assert_eq!(reviewing["state"], "In Review");
    assert_eq!(reviewing["comments"].as_array().unwrap().len(), 2);
    env.ok(&["issue", "comment", "2", "--body", "Merged and shipped."]);
    env.ok(&["issue", "set-state", "2", "Done"]);

    let project = env.json(&["project", "show", "Workflow parity", "--json"]);
    assert_eq!(project["tally"]["completed"], 2);
    assert_eq!(project["tally"]["unstarted"], 1);
    assert_eq!(project["tally"]["total"], 3);

    env.ok(&["issue", "unlock", "2", "--as", "agent-a"]);
    assert!(env.json(&["issue", "show", "2", "--json"])["locked_by"].is_null());

    // A different git repository may use the same local issue number and the
    // same Project name without leaking either object across repository scope.
    let repo2 = TempDir::new().unwrap();
    init_repo(repo2.path());
    env.ok_in(
        repo2.path(),
        &["state", "add", "Backlog", "--type", "backlog"],
    );
    env.ok_in(
        repo2.path(),
        &["project", "create", "--name", "Workflow parity"],
    );
    env.ok_in(
        repo2.path(),
        &[
            "issue",
            "create",
            "--title",
            "Repository two issue one",
            "--state",
            "Backlog",
            "--project",
            "Workflow parity",
        ],
    );
    let repo2_issue: Value = serde_json::from_str(
        env.ok_in(repo2.path(), &["issue", "show", "1", "--json"])
            .trim(),
    )
    .unwrap();
    assert_eq!(repo2_issue["number"], 1);
    assert_eq!(repo2_issue["title"], "Repository two issue one");
    assert_eq!(repo2_issue["project"]["name"], "Workflow parity");
    assert_eq!(
        env.json(&["issue", "show", "1", "--json"])["title"],
        "Foundation"
    );
    assert_eq!(
        env.json(&["project", "show", "Workflow parity", "--json"])["tally"]["total"],
        3
    );
}

#[test]
fn tui_starts_and_quits_cleanly_in_a_real_pseudo_terminal() {
    let env = Env::new();
    env.ok(&["issue", "create", "--title", "PTY smoke"]);

    // macOS/BSD `script` supplies a real pseudo-terminal to crossterm. Keep a
    // hard deadline so a regression in key handling cannot hang the test run.
    let mut child = Command::new("/usr/bin/script")
        .current_dir(env.path())
        .env("XDG_DATA_HOME", env.xdg.path())
        .env("TERM", "xterm-256color")
        .args(["-q", "/dev/null", bin(), "issue", "tui"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn script pseudo-terminal");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"q")
        .expect("failed to send q to TUI");
    drop(child.stdin.take());

    let status = wait_with_timeout(&mut child, Duration::from_secs(10));
    if status.is_none() {
        child.kill().expect("failed to kill hung TUI");
        let _ = child.wait();
        panic!("TUI did not exit within 10 seconds after q");
    }
    let output = child
        .wait_with_output()
        .expect("failed to collect TUI output");
    assert!(
        output.status.success(),
        "PTY TUI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let screen = String::from_utf8_lossy(&output.stdout);
    assert!(
        screen.contains("\u{1b}[?1049h"),
        "alternate-screen entry missing from PTY transcript: {screen:?}"
    );
    assert!(
        screen.contains("\u{1b}[?1049l"),
        "alternate-screen restoration missing from PTY transcript: {screen:?}"
    );
}
