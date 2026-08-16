//! One coherent acceptance journey for repository-local collaboration.
//!
//! The narrower tests in `cli.rs` diagnose individual primitives. This test
//! deliberately keeps one repository, one project, and one issue alive through
//! capture, dependency resolution, handoff, review, and completion so contract
//! drift between otherwise-correct primitives is caught.

use serde_json::Value;
use std::fs;
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

    fn query(&self, document: &str) -> Output {
        let mut child = self
            .command_in(self.path())
            .arg("query")
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
            .expect("failed to write GraphQL document");
        child
            .wait_with_output()
            .expect("failed to collect octa query output")
    }

    fn ok_with_lease(&self, args: &[&str], lease: &str) -> String {
        let mut leased_args = args.to_vec();
        leased_args.extend(["--lease", lease]);
        self.ok(&leased_args)
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

fn assert_command_fails_at(stage: &str, output: &Output, expected_error: &str) {
    assert!(
        !output.status.success(),
        "{stage}: command unexpectedly succeeded"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected_error),
        "{stage}: expected error {expected_error:?}, got:\n{stderr}"
    );
}

struct AgentLifecycle {
    env: Env,
}

impl AgentLifecycle {
    fn new() -> Self {
        let env = Env::new();

        env.ok(&[
            "project",
            "create",
            "--name",
            "Agent lifecycle",
            "--summary",
            "Ship work safely across agent handoffs",
        ]);
        env.ok(&[
            "issue",
            "create",
            "--title",
            "Implement lifecycle scenario",
            "--state",
            "Backlog",
            "--project",
            "Agent lifecycle",
        ]);
        env.ok(&[
            "issue",
            "create",
            "--title",
            "Run the follow-up",
            "--state",
            "Backlog",
            "--project",
            "Agent lifecycle",
        ]);

        // Arrange the dependency, then release the setup lease so every
        // scenario starts with an unowned Backlog issue.
        let setup_lease = env.ok(&["issue", "lock", "1"]).trim().to_string();
        env.ok_with_lease(&["issue", "add", "1", "--blocks", "2"], &setup_lease);
        env.ok(&["issue", "unlock", "1", "--lease", &setup_lease]);

        Self { env }
    }

    fn ready_backlog(&self) -> Value {
        self.env.json(&[
            "issue",
            "list",
            "--state",
            "Backlog",
            "--unblocked",
            "--json",
        ])
    }

    fn start_as_agent_a(&self) -> String {
        let lease = self.env.ok(&["issue", "lock", "1"]).trim().to_string();
        self.env
            .ok_with_lease(&["issue", "set-state", "1", "In Progress"], &lease);
        lease
    }

    fn commit_implementation_and_open_pr(&self) {
        git(
            self.env.path(),
            &["switch", "-q", "-c", "agent-lifecycle-change"],
        );
        fs::write(
            self.env.path().join("lifecycle.txt"),
            "implemented by agent A\n",
        )
        .expect("agent A failed to write the implementation artifact");
        git(self.env.path(), &["add", "lifecycle.txt"]);
        git(
            self.env.path(),
            &["commit", "-q", "-m", "implement lifecycle change"],
        );
        git(self.env.path(), &["switch", "-q", "main"]);
        self.env.ok(&[
            "pr",
            "create",
            "--title",
            "Agent lifecycle change",
            "--branch",
            "agent-lifecycle-change",
        ]);
    }

    fn handoff_to_agent_b(&self, agent_a_lease: &str) -> String {
        self.env.ok(&[
            "issue",
            "comment",
            "1",
            "--body",
            "Handoff: implementation is committed; link PR #1 and review it.",
        ]);
        self.env
            .ok(&["issue", "unlock", "1", "--lease", agent_a_lease]);
        self.env.ok(&["issue", "lock", "1"]).trim().to_string()
    }
}

#[test]
fn leased_issue_rejects_a_competing_agent_and_unleased_mutation() {
    // This scenario isolates the ownership boundary: agent A selects ready
    // Backlog work and locks it, then agent B must fail both to take the lease
    // and to mutate the issue without one. Handoff behavior is tested below.
    let scenario = AgentLifecycle::new();
    assert_eq!(
        numbers_at(&scenario.ready_backlog(), &["number"]),
        vec![1],
        "discovery: only the unblocked Backlog issue should be ready"
    );

    let agent_a_lease = scenario.start_as_agent_a();
    let competing_lock = scenario.env.run(&["issue", "lock", "1"]);
    assert_command_fails_at("agent B competing lease", &competing_lock, "already leased");

    let unleased_write = scenario.env.run(&["issue", "set-state", "1", "In Review"]);
    assert_command_fails_at(
        "agent B write without lease",
        &unleased_write,
        "valid lease required",
    );
    assert!(
        !agent_a_lease.is_empty(),
        "agent A should receive an opaque lease"
    );
}

#[test]
fn handoff_gives_the_next_agent_fresh_ownership_and_complete_context() {
    // This scenario treats a handoff as a recoverability contract. Agent B must
    // see the Issue, Project, dependency, PR, and handoff note, obtain a fresh
    // lease, and prove that agent A's released lease can no longer mutate work.
    let scenario = AgentLifecycle::new();
    let agent_a_lease = scenario.start_as_agent_a();
    scenario.commit_implementation_and_open_pr();

    let context = scenario.env.query(
        r#"{
            issue(number: 1) {
                number
                state
                project { name issues { number state } }
                blocks { number title state }
            }
            pullRequests { number title branch state }
        }"#,
    );
    assert!(
        context.status.success(),
        "context query: GraphQL command failed:\n{}",
        String::from_utf8_lossy(&context.stderr)
    );
    let context: Value = serde_json::from_slice(&context.stdout).unwrap();
    assert_eq!(context["data"]["issue"]["number"], 1);
    assert_eq!(
        context["data"]["issue"]["project"]["name"],
        "Agent lifecycle"
    );
    assert_eq!(context["data"]["issue"]["blocks"][0]["number"], 2);
    assert_eq!(
        context["data"]["pullRequests"][0]["branch"],
        "agent-lifecycle-change"
    );
    assert!(
        context["errors"].is_null(),
        "context query: unexpected GraphQL errors: {}",
        context["errors"]
    );

    let agent_b_lease = scenario.handoff_to_agent_b(&agent_a_lease);
    assert_ne!(
        agent_b_lease, agent_a_lease,
        "handoff: agent B must receive a fresh opaque lease"
    );
    let stale_agent_a_write = scenario.env.run(&[
        "issue",
        "set-state",
        "1",
        "In Review",
        "--lease",
        &agent_a_lease,
    ]);
    assert_command_fails_at(
        "handoff stale agent A lease",
        &stale_agent_a_write,
        "valid lease required",
    );

    scenario
        .env
        .ok_with_lease(&["pr", "add", "1", "--issue", "1"], &agent_b_lease);
    let resumed = scenario.env.json(&["issue", "show", "1", "--json"]);
    assert_eq!(
        resumed["comments"][0]["body"],
        "Handoff: implementation is committed; link PR #1 and review it.",
        "handoff: agent B could not recover agent A's pickup context"
    );
    assert_eq!(
        resumed["pull_requests"][0]["number"], 1,
        "handoff: agent B did not link the implementation PR"
    );
}

#[test]
fn integrated_pr_completes_the_issue_and_reveals_unblocked_work() {
    // This scenario ties workflow completion to an observable Git outcome. The
    // linked PR passes review, its branch is fast-forwarded into main, and only
    // then does Done reveal the dependent issue through the normal read paths.
    let scenario = AgentLifecycle::new();
    let agent_a_lease = scenario.start_as_agent_a();
    scenario.commit_implementation_and_open_pr();
    let agent_b_lease = scenario.handoff_to_agent_b(&agent_a_lease);
    scenario
        .env
        .ok_with_lease(&["pr", "add", "1", "--issue", "1"], &agent_b_lease);

    scenario.env.ok(&["pr", "set-state", "1", "review"]);
    scenario
        .env
        .ok_with_lease(&["issue", "set-state", "1", "In Review"], &agent_b_lease);
    let review = scenario.env.json(&["issue", "show", "1", "--json"]);
    assert_eq!(review["state"], "In Review", "review: issue state drifted");
    assert_eq!(
        review["pull_requests"][0]["state"], "review",
        "review: linked PR state was not visible from the issue"
    );

    git(
        scenario.env.path(),
        &["merge", "-q", "--ff-only", "agent-lifecycle-change"],
    );
    assert!(
        scenario.env.path().join("lifecycle.txt").is_file(),
        "integration: target branch does not contain the implementation artifact"
    );
    scenario.env.ok(&["pr", "set-state", "1", "closed"]);
    scenario
        .env
        .ok_with_lease(&["issue", "set-state", "1", "Done"], &agent_b_lease);
    scenario
        .env
        .ok(&["issue", "unlock", "1", "--lease", &agent_b_lease]);

    let next_open = scenario
        .env
        .json(&["issue", "list", "--unblocked", "--json"]);
    assert_eq!(
        numbers_at(&next_open, &["number"]),
        vec![2],
        "next-work discovery: default open list should reveal the newly unblocked issue"
    );

    let next_from_query = scenario.env.query(
        r#"{
            issue(number: 1) {
                state
                blocks { number title state isClosed }
                pullRequests { number state }
            }
        }"#,
    );
    assert!(
        next_from_query.status.success(),
        "next-work query: GraphQL command failed:\n{}",
        String::from_utf8_lossy(&next_from_query.stderr)
    );
    let next_from_query: Value = serde_json::from_slice(&next_from_query.stdout).unwrap();
    assert_eq!(next_from_query["data"]["issue"]["state"], "Done");
    assert_eq!(
        next_from_query["data"]["issue"]["blocks"][0]["number"], 2,
        "next-work query: completed issue should expose the work it unblocked"
    );
    assert_eq!(
        next_from_query["data"]["issue"]["pullRequests"][0]["state"], "closed",
        "next-work query: integrated PR state should remain observable"
    );
}

#[test]
fn repository_local_collaboration_runs_end_to_end_without_losing_context() {
    let env = Env::new();

    // Labels and their grouping are repository-local, user-defined data.
    env.ok(&[
        "config",
        "label-group",
        "create",
        "Workstream",
        "--target",
        "issue",
        "--selection",
        "single",
    ]);
    for label in ["client", "documentation", "service"] {
        env.ok(&[
            "config",
            "label",
            "create",
            label,
            "--target",
            "issue",
            "--group",
            "Workstream",
        ]);
    }
    let groups = env.json(&[
        "config",
        "label-group",
        "list",
        "--target",
        "issue",
        "--json",
    ]);
    assert_eq!(groups[0]["name"], "Workstream");
    assert_eq!(groups[0]["selection"], "single");
    let labels = env.json(&["config", "label", "list", "--target", "issue", "--json"]);
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
    ]);
    env.ok(&[
        "milestone",
        "create",
        "--project",
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
    ]);
    let foundation_lease = env.ok(&["issue", "lock", "1"]).trim().to_string();
    let target_lease = env.ok(&["issue", "lock", "2"]).trim().to_string();
    env.ok_with_lease(&["issue", "add", "1", "--blocks", "2"], &foundation_lease);
    env.ok_with_lease(&["issue", "add", "2", "--related", "3"], &target_lease);

    let unblocked_before = env.json(&[
        "issue",
        "list",
        "--all",
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
    env.ok_with_lease(&["issue", "set-state", "1", "Done"], &foundation_lease);
    let unblocked_after = env.json(&[
        "issue",
        "list",
        "--all",
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
    env.ok_with_lease(
        &["issue", "set", "2", "--body", groomed_body],
        &target_lease,
    );
    env.ok_with_lease(&["issue", "add", "2", "--label", "client"], &target_lease);
    env.ok_with_lease(&["issue", "set-state", "2", "Todo"], &target_lease);

    // The generic detail projection retains the complete issue context.
    let target = env.json(&["issue", "show", "2", "--json"]);
    assert_eq!(target["body"], groomed_body);
    assert_eq!(target["labels"], serde_json::json!(["client"]));
    assert_eq!(target["project"]["name"], "Workflow parity");
    assert_eq!(target["milestone"]["name"], "CLI beta");
    assert_eq!(target["blocked_by"], serde_json::json!([1]));

    let contended = env.run(&["issue", "lock", "2"]);
    assert!(!contended.status.success());
    assert!(String::from_utf8_lossy(&contended.stderr).contains("already leased"));
    env.ok_with_lease(&["issue", "set-state", "2", "In Progress"], &target_lease);

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
    assert_eq!(handed_off["leased"], true);
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
    assert_eq!(worktree_view["leased"], true);
    assert_eq!(worktree_view["comments"], handed_off["comments"]);

    // PR creation and link are one transaction, and an Issue may carry
    // multiple implementation PRs without replacing an earlier link.
    env.ok_with_lease(
        &[
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
        ],
        &target_lease,
    );
    env.ok_with_lease(
        &[
            "pr",
            "create",
            "--title",
            "Follow-up issue browser fixes",
            "--branch",
            "feat/issue-browser-follow-up",
            "--issue",
            "2",
        ],
        &target_lease,
    );
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
    assert_eq!(detail_json["leased"], true);
    let detail_text = env.ok(&["issue", "show", "2"]);
    for expected in [
        "milestone: CLI beta",
        "leased: yes",
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
    env.ok_with_lease(&["issue", "set-state", "2", "In Review"], &target_lease);
    let reviewing = env.json(&["issue", "show", "2", "--json"]);
    assert_eq!(reviewing["state"], "In Review");
    assert_eq!(reviewing["comments"].as_array().unwrap().len(), 2);
    env.ok(&["issue", "comment", "2", "--body", "Merged and shipped."]);
    env.ok_with_lease(&["issue", "set-state", "2", "Done"], &target_lease);

    let project = env.json(&["project", "show", "Workflow parity", "--json"]);
    assert_eq!(project["tally"]["closed"], 2);
    assert_eq!(project["tally"]["open"], 1);
    assert_eq!(project["tally"]["total"], 3);

    env.ok(&["issue", "unlock", "2", "--lease", &target_lease]);
    assert_eq!(env.json(&["issue", "show", "2", "--json"])["leased"], false);

    // A different git repository may use the same local issue number and the
    // same Project name without leaking either object across repository scope.
    let repo2 = TempDir::new().unwrap();
    init_repo(repo2.path());
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
