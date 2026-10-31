//! Release quality tests: drive the built `atk` binary the way people and agents do, so a
//! release can't ship with a broken command. CI runs them on every PR and before every
//! release is published. Tests that start containers skip themselves without Docker.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const DEMO: &str = "tests/fixtures/demo";
const NODE: &str = "tests/fixtures/node";

/// `atk` with its run history and settings isolated in `home`.
fn atk(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_atk"));
    cmd.current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CONFIG_HOME", home.join("config"));
    cmd
}

fn run(home: &Path, args: &[&str]) -> Output {
    atk(home).args(args).output().expect("atk runs")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", text(out)))
}

fn home() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// A throwaway project holding one workflow.
fn project(workflow: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let wf = dir.path().join(".atk/workflows");
    std::fs::create_dir_all(&wf).unwrap();
    std::fs::write(wf.join("ci.yml"), workflow).unwrap();
    dir
}

/// Whether Docker can run jobs. Linux CI always has it, so there a missing Docker is a
/// failure rather than a skip: release tests must never pass by not running.
fn docker(home: &Path) -> bool {
    let doctor = run(home, &["doctor"]);
    let ready = doctor.status.success();
    if !ready && cfg!(target_os = "linux") && std::env::var_os("CI").is_some() {
        panic!("Docker should be ready on Linux CI: {}", text(&doctor));
    }
    if !ready {
        eprintln!("skipping: Docker is not available");
    }
    ready
}

#[test]
fn reports_its_version() {
    let h = home();
    let out = run(h.path(), &["--version"]);
    assert!(out.status.success());
    assert!(
        text(&out).contains(env!("CARGO_PKG_VERSION")),
        "{}",
        text(&out)
    );
}

#[test]
fn lists_workflows_and_jobs() {
    let h = home();
    let out = run(h.path(), &["-C", DEMO, "list", "--json"]);
    assert!(out.status.success(), "{}", text(&out));
    let list = json(&out);
    let demo = &list[0];
    assert_eq!(demo["name"], "Demo");
    let jobs: Vec<&str> = demo["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j["id"].as_str().unwrap())
        .collect();
    assert_eq!(jobs, ["build", "test", "report"]);
    assert_eq!(demo["jobs"][1]["matrix"], 2);
}

#[test]
fn validates_good_and_bad_workflows() {
    let h = home();
    let ok = run(h.path(), &["-C", DEMO, "validate", "--json"]);
    assert!(ok.status.success(), "{}", text(&ok));
    assert_eq!(json(&ok)["valid"], true);

    let bad = project("on: push\njobs:\n  a:\n    needs: missing\n    runs-on: ubuntu-latest\n    steps: [{ run: echo }]\n");
    let dir = bad.path().to_str().unwrap();
    let out = run(h.path(), &["-C", dir, "validate", "--json"]);
    assert!(
        !out.status.success(),
        "an invalid workflow must fail validation"
    );
    assert_eq!(json(&out)["valid"], false);
}

#[test]
fn prints_tools_for_every_agent_format() {
    let h = home();
    for format in ["mcp", "openai", "anthropic"] {
        let out = run(h.path(), &["tools", "--format", format]);
        assert!(out.status.success(), "{}", text(&out));
        assert_eq!(json(&out).as_array().unwrap().len(), 10, "{format}");
    }
}

#[test]
fn calls_tools_and_reports_errors() {
    let h = home();
    let out = run(h.path(), &["-C", DEMO, "call", "list_workflows"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(json(&out)["workflows"][0]["name"], "Demo");

    let out = run(h.path(), &["call", "no_such_tool"]);
    assert!(!out.status.success());
    assert!(json(&out)["error"]
        .as_str()
        .unwrap()
        .contains("unknown tool"));
}

#[test]
fn serves_mcp_over_stdio() {
    let h = home();
    let mut child = atk(h.path())
        .args(["-C", DEMO, "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"validate_workflows","arguments":{}}}"#,
    ];
    let mut stdin = child.stdin.take().unwrap();
    for r in requests {
        writeln!(stdin, "{r}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let replies: Vec<Value> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(replies.len(), 3, "the notification gets no reply");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "atk");
    assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), 10);
    assert_eq!(replies[2]["result"]["isError"], false);
}

#[test]
fn saves_project_settings() {
    let h = home();
    let p = project("on: push\njobs:\n  a: { runs-on: ubuntu-latest, steps: [{ run: echo }] }\n");
    let dir = p.path().to_str().unwrap();
    let out = run(
        h.path(),
        &["-C", dir, "config", "set", "image.ubuntu-latest", "node:20"],
    );
    assert!(out.status.success(), "{}", text(&out));
    let saved = std::fs::read_to_string(p.path().join(".atk.toml")).unwrap();
    assert!(saved.contains("node:20"), "{saved}");
    assert!(!run(h.path(), &["-C", dir, "config", "set", "nope", "x"])
        .status
        .success());
}

#[test]
fn runs_a_job_in_docker_and_reviews_it() {
    let h = home();
    if !docker(h.path()) {
        return;
    }
    let out = run(
        h.path(),
        &["-C", DEMO, "run", "demo", "--job", "build", "--json"],
    );
    assert!(out.status.success(), "{}", text(&out));
    let last = String::from_utf8_lossy(&out.stdout)
        .lines()
        .last()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .unwrap();
    assert_eq!(last["type"], "finished");
    assert_eq!(last["outcome"], "success");

    let status = json(&run(h.path(), &["-C", DEMO, "status", "last", "--json"]));
    assert_eq!(status["outcome"], "success");
    assert!(status["failures"].as_array().unwrap().is_empty());

    let logs = text(&run(h.path(), &["-C", DEMO, "logs", "last"]));
    assert!(logs.contains("custom-tool says hi"), "{logs}");
}

#[test]
fn reports_a_failing_step() {
    let h = home();
    if !docker(h.path()) {
        return;
    }
    let p = project(
        "on: push\njobs:\n  broken:\n    runs-on: ubuntu-latest\n    steps:\n      - name: Fail loudly\n        run: |\n          echo about to fail\n          exit 4\n",
    );
    let dir = p.path().to_str().unwrap();
    let out = run(h.path(), &["-C", dir, "run", "ci"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));

    let status = json(&run(h.path(), &["-C", dir, "status", "last", "--json"]));
    assert_eq!(status["outcome"], "failure");
    let failure = &status["failures"][0];
    assert_eq!(failure["step"], "Fail loudly");
    assert!(
        failure["output_tail"].to_string().contains("about to fail"),
        "{failure}"
    );
}

#[test]
fn an_action_that_cannot_run_fails_the_job() {
    let h = home();
    if !docker(h.path()) {
        return;
    }
    let p = project(
        "on: push\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: owner/missing-action@v1\n",
    );
    let dir = p.path().to_str().unwrap();
    let out = run(h.path(), &["-C", dir, "run", "ci"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("no local copy"), "{}", text(&out));
}

#[test]
fn node_actions_write_step_files() {
    let h = home();
    if !docker(h.path()) {
        return;
    }
    let out = run(
        h.path(),
        &["-C", NODE, "run", "ci", "--image", "ubuntu-latest=node:20"],
    );
    assert!(out.status.success(), "{}", text(&out));

    let logs = text(&run(h.path(), &["-C", NODE, "logs", "last"]));
    assert!(logs.contains("node-tool is on PATH"), "{logs}");
}

#[test]
fn detached_runs_can_be_watched() {
    let h = home();
    if !docker(h.path()) {
        return;
    }
    let out = run(
        h.path(),
        &[
            "-C", DEMO, "run", "demo", "--job", "build", "--detach", "--json",
        ],
    );
    assert!(out.status.success(), "{}", text(&out));
    let id = json(&out)["id"].as_str().unwrap().to_string();

    let watched = run(h.path(), &["-C", DEMO, "watch", &id]);
    assert!(watched.status.success(), "{}", text(&watched));
    assert!(text(&watched).contains("succeeded"), "{}", text(&watched));

    let runs = json(&run(h.path(), &["-C", DEMO, "runs", "--json"]));
    assert!(runs
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == id.as_str()));
}
