//! Command line. `atk` alone opens the TUI; every subcommand works without one, for people
//! who prefer commands and for AI agents (add `--json` for machine-readable output).

use std::collections::BTreeMap;
use std::env;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};

use crate::config::Config;
use crate::engine;
use crate::headless::{self, RunRequest};
use crate::history::{self, LogLine, Outcome, RunRecord, Store};
use crate::mcp;
use crate::report;
use crate::runner::{LogKind, RunEvent, Status};
use crate::tui::{self, Start};
use crate::util::fmt_duration;
use crate::workflow::{self, plan::RunPlan, Source};

#[derive(Parser)]
#[command(
    name = "atk",
    version,
    about = "Actions Toolkit: run your CI/CD workflows locally in Docker",
    long_about = "Actions Toolkit (ATK) runs GitHub Actions-style workflows on your own machine, \
                  each job in its own Docker container. Run `atk` alone for the terminal UI, or \
                  use the commands below; add --json for machine-readable output."
)]
struct Cli {
    /// Project folder (defaults to the current directory)
    #[arg(short = 'C', long = "dir", value_name = "DIR", global = true)]
    dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Open settings and the guide in the terminal UI
    Settings,
    /// List this project's workflows and their jobs
    List {
        #[arg(long)]
        json: bool,
    },
    /// Check workflow files for errors without running them
    Validate {
        /// Workflow path, file name or name (default: every workflow)
        workflow: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Run a workflow, or one of its jobs, in Docker
    Run(RunArgs),
    /// Run a past run's workflow again
    Rerun {
        /// Run id, a unique start of one, or `last`
        #[arg(default_value = "last")]
        id: String,
        /// Start it in the background and print its id
        #[arg(short, long)]
        detach: bool,
        #[arg(long)]
        json: bool,
    },
    /// This project's runs, newest first, including ones still going
    Runs {
        /// How many to show
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// A run's jobs and steps, and what failed
    Status {
        /// Run id, a unique start of one, or `last`
        #[arg(default_value = "last")]
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Print a run's logs
    Logs {
        /// Run id, a unique start of one, or `last`
        #[arg(default_value = "last")]
        id: String,
        #[command(flatten)]
        filter: LogFilter,
        /// Only the last N lines
        #[arg(long, value_name = "N")]
        tail: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Follow a run's logs until it ends; exits with the run's result
    Watch {
        /// Run id, a unique start of one, or `last`
        #[arg(default_value = "last")]
        id: String,
        #[command(flatten)]
        filter: LogFilter,
        #[arg(long)]
        json: bool,
    },
    /// Stop a run that is still going
    Cancel {
        /// Run id, a unique start of one, or `last`
        #[arg(default_value = "last")]
        id: String,
    },
    /// Delete a run from the history
    Delete {
        /// Run id or a unique start of one
        id: String,
    },
    /// Delete every finished run of this project
    Clear {
        /// Confirm deleting them
        #[arg(long)]
        yes: bool,
    },
    /// Check that Docker is ready and show where ATK keeps its files
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Show or change settings
    Config {
        #[command(subcommand)]
        action: Option<ConfigCmd>,
        #[arg(long)]
        json: bool,
    },
    /// Show or change where workflows are searched
    Locations {
        #[command(subcommand)]
        action: Option<LocationCmd>,
        #[arg(long)]
        json: bool,
    },
    /// Serve ATK's tools to AI agents over the Model Context Protocol (stdio)
    Mcp,
    /// Print ATK's AI tool definitions for an agent framework
    Tools {
        #[arg(long, value_enum, default_value_t = mcp::Format::Mcp)]
        format: mcp::Format,
    },
    /// Call one AI tool with JSON arguments and print its JSON result
    Call {
        /// Tool name (see `atk tools`)
        tool: String,
        /// Arguments as a JSON object
        #[arg(default_value = "{}")]
        args: String,
    },
}

#[derive(Args)]
struct RunArgs {
    /// Workflow path, file name or name
    workflow: String,
    /// Run only this job (all its matrix variants), treating its `needs` as done
    #[arg(short, long)]
    job: Option<String>,
    /// Event to simulate (default: from settings, else push)
    #[arg(short, long)]
    event: Option<String>,
    /// A `workflow_dispatch` input
    #[arg(long = "input", value_name = "NAME=VALUE", value_parser = key_value)]
    inputs: Vec<(String, String)>,
    /// A value for `vars.NAME`
    #[arg(long = "var", value_name = "NAME=VALUE", value_parser = key_value)]
    vars: Vec<(String, String)>,
    /// Use IMAGE for jobs with `runs-on: LABEL`
    #[arg(long = "image", value_name = "LABEL=IMAGE", value_parser = key_value)]
    images: Vec<(String, String)>,
    /// Always pull images, even when present locally
    #[arg(long)]
    pull: bool,
    /// Mount the project into containers instead of copying it (steps can change your files)
    #[arg(long)]
    bind: bool,
    /// Start the run in the background and print its id
    #[arg(short, long)]
    detach: bool,
    /// Print events as JSON lines
    #[arg(long)]
    json: bool,
    /// Record the run under this id (used by --detach)
    #[arg(long, hide = true)]
    run_id: Option<String>,
}

#[derive(Args)]
struct LogFilter {
    /// Only this job: its number, name, or the start of its name
    #[arg(long)]
    job: Option<String>,
    /// Only this step: its number or name
    #[arg(long)]
    step: Option<String>,
}

#[derive(Subcommand)]
enum ConfigCmd {
    /// Set `event`, `parallel`, `image.LABEL` or `var.NAME`
    Set {
        key: String,
        value: String,
        /// Change your user settings instead of this project's .atk.toml
        #[arg(long)]
        global: bool,
    },
    /// Remove a setting
    Unset {
        key: String,
        /// Change your user settings instead of this project's .atk.toml
        #[arg(long)]
        global: bool,
    },
}

#[derive(Subcommand)]
enum LocationCmd {
    /// Search another folder or .yml file for workflows
    Add {
        path: String,
        /// For every project instead of just this one
        #[arg(long)]
        global: bool,
    },
    /// Stop searching a folder or file
    Remove {
        /// As listed by `atk locations`
        entry: String,
        /// From your user settings instead of this project's .atk.toml
        #[arg(long)]
        global: bool,
    },
}

fn key_value(s: &str) -> Result<(String, String), String> {
    match s.split_once('=') {
        Some((k, v)) if !k.trim().is_empty() => Ok((k.trim().to_string(), v.to_string())),
        _ => Err(format!("expected NAME=VALUE, got `{s}`")),
    }
}

pub fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let start = match &cli.dir {
        Some(d) => {
            crate::util::canonical(d).with_context(|| format!("{} does not exist", d.display()))?
        }
        None => env::current_dir()?,
    };
    let root = workflow::find_root(&start);
    let Some(command) = cli.command else {
        return open_tui(root, Start::Workflows);
    };
    match command {
        Cmd::Settings => open_tui(root, Start::Settings),
        Cmd::List { json } => list(&root, json),
        Cmd::Validate { workflow, json } => validate(&root, workflow.as_deref(), json),
        Cmd::Run(args) => run_workflow(&root, args),
        Cmd::Rerun { id, detach, json } => rerun(&root, &id, detach, json),
        Cmd::Runs { limit, json } => runs(&root, limit, json),
        Cmd::Status { id, json } => status(&root, &id, json),
        Cmd::Logs {
            id,
            filter,
            tail,
            json,
        } => logs(&root, &id, &filter, tail, json),
        Cmd::Watch { id, filter, json } => watch(&root, &id, &filter, json),
        Cmd::Cancel { id } => cancel(&root, &id),
        Cmd::Delete { id } => delete(&root, &id),
        Cmd::Clear { yes } => clear(&root, yes),
        Cmd::Doctor { json } => doctor(&root, json),
        Cmd::Config { action, json } => config(&root, action, json),
        Cmd::Locations { action, json } => locations(&root, action, json),
        Cmd::Mcp => mcp::serve(root).map(|_| ExitCode::SUCCESS),
        Cmd::Tools { format } => {
            print_json(&mcp::export(format));
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Call { tool, args } => {
            let args: Value = serde_json::from_str(&args).context("arguments must be JSON")?;
            match mcp::call(&root, &tool, &args) {
                Ok(v) => {
                    print_json(&v);
                    Ok(ExitCode::SUCCESS)
                }
                Err(e) => {
                    print_json(&json!({ "error": format!("{e:#}") }));
                    Ok(ExitCode::FAILURE)
                }
            }
        }
    }
}

fn open_tui(root: PathBuf, start: Start) -> Result<ExitCode> {
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        bail!("the ATK terminal UI needs an interactive terminal; see `atk --help` for commands");
    }
    tui::open(root, start)?;
    Ok(ExitCode::SUCCESS)
}

fn store() -> Result<Store> {
    Store::open_default().context("this system has no user data folder for the run history")
}

fn print_json(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn print_json_line(v: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

/// The exit code a run's outcome maps to: 0 success, 130 cancelled, 1 anything else.
fn exit_code(outcome: Outcome) -> ExitCode {
    match outcome {
        Outcome::Success => ExitCode::SUCCESS,
        Outcome::Cancelled => ExitCode::from(130),
        _ => ExitCode::FAILURE,
    }
}

fn icon(status: Status) -> &'static str {
    match status {
        Status::Pending => "○",
        Status::Running => "●",
        Status::Success => "✓",
        Status::Failure => "✗",
        Status::Skipped => "-",
        Status::Cancelled => "■",
    }
}

fn outcome_icon(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Running => "●",
        Outcome::Success => "✓",
        Outcome::Failure => "✗",
        Outcome::Cancelled => "■",
        Outcome::Interrupted => "!",
    }
}

fn millis(ms: Option<u64>) -> String {
    ms.map(|m| fmt_duration(Duration::from_millis(m)))
        .unwrap_or_default()
}

/// A log line as text, prefixed with its job.
fn log_text(job: &str, kind: LogKind, text: &str) -> String {
    let mark = match kind {
        LogKind::Output => "",
        LogKind::Command => "│ ",
        LogKind::System => "· ",
        LogKind::Header => "━━ ",
        LogKind::Group => "▸ ",
        LogKind::Notice => "ℹ ",
        LogKind::Warning => "⚠ ",
        LogKind::Error => "✗ ",
    };
    format!("[{job}] {mark}{text}")
}

// ---------------------------------------------------------------------------------------------
// Workflows

fn list(root: &Path, json: bool) -> Result<ExitCode> {
    let all = report::workflows(root);
    if json {
        print_json(&all);
        return Ok(ExitCode::SUCCESS);
    }
    let items = all.as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        println!(
            "No workflows found in {} (searched {} and any locations from `atk locations`).",
            root.display(),
            workflow::AUTO_DIRS.join(", ")
        );
    }
    for wf in items {
        let path = wf["path"].as_str().unwrap_or_default();
        let name = wf["name"].as_str().unwrap_or_default();
        if let Some(err) = wf["error"].as_str() {
            println!("✗ {path}  invalid: {}", err.lines().next().unwrap_or(""));
            continue;
        }
        let on: Vec<&str> = wf["on"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        println!("{name}  {path}  on: {}", on.join(", "));
        for job in wf["jobs"].as_array().into_iter().flatten() {
            let matrix = job["matrix"].as_u64().unwrap_or(1);
            let runs_on: Vec<&str> = job["runs_on"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let times = if matrix > 1 {
                format!("  ×{matrix}")
            } else {
                String::new()
            };
            println!(
                "  {}{times}  {}",
                job["id"].as_str().unwrap_or_default(),
                runs_on.join(",")
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn validate(root: &Path, which: Option<&str>, json: bool) -> Result<ExitCode> {
    let only = which
        .map(|w| workflow::find(root, w))
        .transpose()
        .map_err(anyhow::Error::msg)?;
    let (results, ok) = report::validate(root, only.as_deref());
    if json {
        print_json(&json!({ "valid": ok, "workflows": results }));
    } else {
        for r in results.as_array().into_iter().flatten() {
            let path = r["path"].as_str().unwrap_or_default();
            match r["error"].as_str() {
                Some(e) => println!("✗ {path}\n    {}", e.replace('\n', "\n    ")),
                None => println!("✓ {path}  {} jobs", r["jobs"]),
            }
        }
    }
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

// ---------------------------------------------------------------------------------------------
// Running

fn run_workflow(root: &Path, args: RunArgs) -> Result<ExitCode> {
    let req = RunRequest {
        workflow: workflow::find(root, &args.workflow).map_err(anyhow::Error::msg)?,
        job: args.job,
        event: args.event,
        inputs: args.inputs.into_iter().collect(),
        vars: args.vars.into_iter().collect(),
        images: args.images.into_iter().collect(),
        pull: args.pull,
        bind: args.bind,
    };
    if args.detach {
        return detach(root, &req, args.json);
    }
    let id = args.run_id.unwrap_or_else(history::new_id);
    foreground(root, &req, id, args.json)
}

fn detach(root: &Path, req: &RunRequest, json: bool) -> Result<ExitCode> {
    let store = store()?;
    let id = headless::spawn_detached(root, &store, req)?;
    if json {
        print_json(&json!({ "id": id, "outcome": "running" }));
    } else {
        println!("Started run {id}");
        println!("  atk watch {id}    follow it");
        println!("  atk status {id}   check on it");
        println!("  atk cancel {id}   stop it");
    }
    Ok(ExitCode::SUCCESS)
}

fn foreground(root: &Path, req: &RunRequest, id: String, json: bool) -> Result<ExitCode> {
    let store = Store::open_default();
    if json {
        print_json_line(&json!({ "type": "run", "id": id }));
    } else {
        println!("Run {id}");
    }
    let mut on_event = |plan: &RunPlan, ev: &RunEvent| print_event(plan, ev, json);
    let outcome = headless::execute(root, store.clone(), req, id.clone(), &mut on_event)?;
    match store.as_ref().and_then(|s| s.get(&id).map(|r| (s, r))) {
        Some((store, record)) if json => {
            let mut done = report::review(store, &record);
            done["type"] = json!("finished");
            print_json_line(&done);
        }
        Some((store, record)) => print_summary(store, &record),
        None if json => {
            print_json_line(&json!({ "type": "finished", "outcome": outcome.as_str() }))
        }
        None => println!("{} {}", outcome_icon(outcome), outcome.as_str()),
    }
    Ok(exit_code(outcome))
}

fn print_event(plan: &RunPlan, ev: &RunEvent, json: bool) {
    let job_name = |j: usize| plan.jobs.get(j).map(|p| p.label.as_str()).unwrap_or("");
    let step_name = |j: usize, s: usize| {
        plan.jobs
            .get(j)
            .and_then(|p| p.step_names.get(s))
            .map(String::as_str)
            .unwrap_or("")
    };
    if json {
        let v = match ev {
            RunEvent::Job { job, status } => {
                json!({ "type": "job", "job": job_name(*job), "status": status })
            }
            RunEvent::Step { job, step, status } => json!({
                "type": "step",
                "job": job_name(*job),
                "step": step_name(*job, *step),
                "status": status,
            }),
            RunEvent::Log {
                job,
                step,
                kind,
                text,
            } => json!({
                "type": "log",
                "job": job_name(*job),
                "step": step_name(*job, *step),
                "kind": kind,
                "text": text,
            }),
            RunEvent::Finished { .. } => return,
        };
        print_json_line(&v);
        return;
    }
    match ev {
        RunEvent::Job { job, status } if status.is_done() => {
            println!("[{}] {} {}", job_name(*job), icon(*status), status.as_str());
        }
        RunEvent::Log {
            job, kind, text, ..
        } => println!("{}", log_text(job_name(*job), *kind, text)),
        _ => {}
    }
}

fn print_summary(store: &Store, record: &RunRecord) {
    let review = report::review(store, record);
    let took = fmt_duration(Duration::from_millis(record.millis));
    println!();
    match record.outcome {
        Outcome::Success => println!("✓ {} succeeded in {took}", record.title),
        Outcome::Running => println!("● {} is still running ({took})", record.title),
        other => println!(
            "{} {} {} after {took}",
            outcome_icon(other),
            record.title,
            match other {
                Outcome::Cancelled => "was cancelled",
                Outcome::Interrupted => "was interrupted",
                _ => "failed",
            }
        ),
    }
    for f in review["failures"].as_array().into_iter().flatten() {
        println!(
            "  ✗ {} › {}",
            f["job"].as_str().unwrap_or_default(),
            f["step"].as_str().unwrap_or_default()
        );
    }
    if record.outcome != Outcome::Success {
        println!("  atk status {} for details", record.id);
    }
}

fn rerun(root: &Path, id: &str, detach_it: bool, json: bool) -> Result<ExitCode> {
    let record = store()?.find(root, id)?;
    let req = RunRequest {
        workflow: record.workflow.clone(),
        job: record.job.clone(),
        event: Some(record.event.clone()),
        ..Default::default()
    };
    if detach_it {
        detach(&record.project, &req, json)
    } else {
        foreground(&record.project, &req, history::new_id(), json)
    }
}

// ---------------------------------------------------------------------------------------------
// History

fn runs(root: &Path, limit: usize, json: bool) -> Result<ExitCode> {
    let runs: Vec<RunRecord> = store()?.list(root).into_iter().take(limit).collect();
    if json {
        print_json(&Value::Array(runs.iter().map(report::run).collect()));
        return Ok(ExitCode::SUCCESS);
    }
    if runs.is_empty() {
        println!("No runs yet for {}.", root.display());
    }
    for r in &runs {
        let job = r
            .job
            .as_ref()
            .map(|j| format!(" › {j}"))
            .unwrap_or_default();
        println!(
            "{} {}  {}{job}  {} · {} · {}",
            outcome_icon(r.outcome),
            r.id,
            r.title,
            r.event,
            fmt_duration(Duration::from_millis(r.millis)),
            history::ago(r.started)
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn status(root: &Path, id: &str, json: bool) -> Result<ExitCode> {
    let store = store()?;
    let record = store.find(root, id)?;
    let review = report::review(&store, &record);
    if json {
        print_json(&review);
        return Ok(ExitCode::SUCCESS);
    }
    println!(
        "{} {}  {}  {} · {} · {}",
        outcome_icon(record.outcome),
        record.title,
        record.outcome.as_str(),
        record.event,
        record.engine,
        history::ago(record.started)
    );
    println!(
        "id {}  workflow {}{}",
        record.id,
        report::rel(&record.project, &record.workflow),
        record
            .job
            .as_ref()
            .map(|j| format!(" › {j}"))
            .unwrap_or_default()
    );
    for job in &record.jobs {
        println!(
            "  {} {}  {}",
            icon(job.status),
            job.name,
            millis(job.millis)
        );
        if job.status != Status::Success {
            for step in &job.steps {
                println!(
                    "      {} {}  {}",
                    icon(step.status),
                    step.name,
                    millis(step.millis)
                );
            }
        }
    }
    let artifacts = store.artifacts(&record.id);
    if !artifacts.is_empty() {
        println!(
            "
Artifacts"
        );
    }
    for a in &artifacts {
        let files = if a.files == 1 { "file" } else { "files" };
        println!(
            "  {}  {} {files} · {}  {}",
            a.name,
            a.files,
            fmt_size(a.bytes),
            a.path.display()
        );
    }
    let failures = review["failures"].as_array().cloned().unwrap_or_default();
    if !failures.is_empty() {
        println!("\nFailures");
    }
    for f in failures {
        println!(
            "  {} › {}",
            f["job"].as_str().unwrap_or_default(),
            f["step"].as_str().unwrap_or_default()
        );
        for line in f["output_tail"].as_array().into_iter().flatten() {
            println!("    {}", line.as_str().unwrap_or_default());
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn fmt_size(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{:.1} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

fn print_lines(record: &RunRecord, lines: &[LogLine], json: bool) {
    for line in lines {
        if json {
            print_json_line(&report::log_line(record, line));
        } else {
            let job = record
                .jobs
                .get(line.job)
                .map(|j| j.name.as_str())
                .unwrap_or("");
            println!("{}", log_text(job, line.kind, &line.text));
        }
    }
}

fn logs(
    root: &Path,
    id: &str,
    filter: &LogFilter,
    tail: Option<usize>,
    json: bool,
) -> Result<ExitCode> {
    let store = store()?;
    let record = store.find(root, id)?;
    let lines = report::filter_logs(
        &record,
        store.logs(&record.id),
        filter.job.as_deref(),
        filter.step.as_deref(),
    )?;
    let skip = tail.map_or(0, |n| lines.len().saturating_sub(n));
    print_lines(&record, &lines[skip..], json);
    Ok(ExitCode::SUCCESS)
}

fn watch(root: &Path, id: &str, filter: &LogFilter, json: bool) -> Result<ExitCode> {
    let store = store()?;
    let mut record = store.find(root, id)?;
    let mut offset = 0;
    loop {
        let (lines, next) = store.logs_from(&record.id, offset);
        offset = next;
        let lines = report::filter_logs(
            &record,
            lines,
            filter.job.as_deref(),
            filter.step.as_deref(),
        )?;
        print_lines(&record, &lines, json);
        if record.outcome.is_done() {
            break;
        }
        thread::sleep(Duration::from_millis(300));
        record = store.get(&record.id).context("the run was deleted")?;
    }
    // Lines written between the last read and the end of the run.
    let (rest, _) = store.logs_from(&record.id, offset);
    let rest = report::filter_logs(&record, rest, filter.job.as_deref(), filter.step.as_deref())?;
    print_lines(&record, &rest, json);
    if json {
        let mut done = report::review(&store, &record);
        done["type"] = json!("finished");
        print_json_line(&done);
    } else {
        print_summary(&store, &record);
    }
    Ok(exit_code(record.outcome))
}

fn cancel(root: &Path, id: &str) -> Result<ExitCode> {
    let store = store()?;
    let record = store.find(root, id)?;
    if record.outcome.is_done() {
        println!(
            "Run {} already ended ({})",
            record.id,
            record.outcome.as_str()
        );
        return Ok(ExitCode::SUCCESS);
    }
    store.request_cancel(&record.id)?;
    println!("Asked run {} to stop", record.id);
    Ok(ExitCode::SUCCESS)
}

fn delete(root: &Path, id: &str) -> Result<ExitCode> {
    let store = store()?;
    let record = store.find(root, id)?;
    if !record.outcome.is_done() {
        bail!("run {} is still going; `atk cancel` it first", record.id);
    }
    store.delete(&record.id)?;
    println!("Deleted run {}", record.id);
    Ok(ExitCode::SUCCESS)
}

fn clear(root: &Path, yes: bool) -> Result<ExitCode> {
    if !yes {
        bail!("this deletes every finished run of this project; pass --yes to confirm");
    }
    let n = store()?.clear(root)?;
    println!("Deleted {n} run{}", if n == 1 { "" } else { "s" });
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------------------------
// Setup

fn doctor(root: &Path, json: bool) -> Result<ExitCode> {
    let docker = engine::detect();
    let workflows = workflow::discover(root);
    let info = json!({
        "docker": {
            "status": docker.status_label(),
            "ready": docker.is_ready(),
            "version": docker.version,
            "note": docker.note,
        },
        "project": root,
        "workflows": workflows.len(),
        "invalid_workflows": workflows.iter().filter(|w| w.workflow().is_none()).count(),
        "user_config": Config::user_path(),
        "project_config": Config::project_path(root),
        "history": Store::open_default().map(|s| s.dir().to_path_buf()),
    });
    if json {
        print_json(&info);
    } else {
        let version = docker
            .version
            .as_ref()
            .map(|v| format!(" {v}"))
            .unwrap_or_default();
        let note = docker
            .note
            .as_ref()
            .map(|n| format!(": {n}"))
            .unwrap_or_default();
        let mark = if docker.is_ready() { "✓" } else { "✗" };
        println!("{mark} Docker{version} is {}{note}", docker.status_label());
        println!("  project         {}", root.display());
        println!("  workflows       {}", workflows.len());
        let path = |v: &Value| v.as_str().unwrap_or("unavailable").to_string();
        println!("  user config     {}", path(&info["user_config"]));
        println!("  project config  {}", path(&info["project_config"]));
        println!("  run history     {}", path(&info["history"]));
    }
    Ok(if docker.is_ready() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn config(root: &Path, action: Option<ConfigCmd>, json: bool) -> Result<ExitCode> {
    let (key, value, global) = match action {
        None => {
            let merged = Config::load(root)?;
            if json {
                print_json(&json!({
                    "config": merged,
                    "user_config": Config::user_path(),
                    "project_config": Config::project_path(root),
                }));
            } else {
                print!("{}", toml::to_string_pretty(&merged)?);
            }
            return Ok(ExitCode::SUCCESS);
        }
        Some(ConfigCmd::Set { key, value, global }) => (key, Some(value), global),
        Some(ConfigCmd::Unset { key, global }) => (key, None, global),
    };
    let mut cfg = if global {
        Config::load_user()?
    } else {
        Config::load_project(root)?
    };
    set_key(&mut cfg, &key, value.as_deref())?;
    let path = if global {
        cfg.save_user()?
    } else {
        cfg.save_project(root)?
    };
    println!("Saved {}", path.display());
    Ok(ExitCode::SUCCESS)
}

fn set_key(cfg: &mut Config, key: &str, value: Option<&str>) -> Result<()> {
    fn put(map: &mut BTreeMap<String, String>, name: &str, value: Option<&str>) {
        match value {
            Some(v) => map.insert(name.to_string(), v.to_string()),
            None => map.remove(name),
        };
    }
    match key.split_once('.') {
        None if key == "event" => cfg.event = value.map(String::from),
        None if key == "parallel" => {
            cfg.parallel = match value {
                None | Some("auto") => None,
                Some(v) => match v.parse::<usize>() {
                    Ok(n) if n >= 1 => Some(n),
                    _ => bail!("parallel must be a whole number of 1 or more, or `auto`"),
                },
            }
        }
        Some(("image", label)) => put(&mut cfg.images, label, value),
        Some(("var", name)) => put(&mut cfg.vars, name, value),
        _ => bail!("unknown setting `{key}`; use event, parallel, image.LABEL or var.NAME"),
    }
    Ok(())
}

fn locations(root: &Path, action: Option<LocationCmd>, json: bool) -> Result<ExitCode> {
    let (entry, add, global) = match action {
        None => {
            let all: Vec<Value> = workflow::locations(root)
                .into_iter()
                .map(|l| {
                    let found = l.path.exists().then(|| workflow::scan(&l).len());
                    json!({
                        "source": l.source.label(),
                        "entry": l.entry,
                        "path": l.path,
                        "workflows": found,
                    })
                })
                .collect();
            if json {
                print_json(&Value::Array(all));
            } else {
                for l in all {
                    let found = match l["workflows"].as_u64() {
                        Some(n) => format!("{n} found"),
                        None => "missing".into(),
                    };
                    println!(
                        "{:<8} {}  {found}",
                        l["source"].as_str().unwrap_or_default(),
                        l["entry"].as_str().unwrap_or_default()
                    );
                }
            }
            return Ok(ExitCode::SUCCESS);
        }
        Some(LocationCmd::Add { path, global }) => (path, true, global),
        Some(LocationCmd::Remove { entry, global }) => (entry, false, global),
    };
    let source = if global {
        Source::Global
    } else {
        Source::Project
    };
    let mut cfg = if global {
        Config::load_user()?
    } else {
        Config::load_project(root)?
    };
    if add {
        let base = match source {
            Source::Global => dirs::home_dir().context("could not find the home directory")?,
            _ => root.to_path_buf(),
        };
        let path = workflow::resolve(&entry, &base);
        if !path.exists() {
            bail!("{} does not exist", path.display());
        }
        // Project entries are stored relative to the root so the config stays portable.
        let stored = match (source, path.strip_prefix(root)) {
            (Source::Project, Ok(rel)) if rel.as_os_str().is_empty() => ".".to_string(),
            (Source::Project, Ok(rel)) => rel.to_string_lossy().into_owned(),
            _ => entry.trim_end_matches(['/', '\\']).to_string(),
        };
        if cfg.workflows.contains(&stored) {
            bail!("{stored} is already listed");
        }
        cfg.workflows.push(stored);
    } else {
        let before = cfg.workflows.len();
        cfg.workflows.retain(|e| *e != entry);
        if cfg.workflows.len() == before {
            bail!("{entry} is not listed (see `atk locations`)");
        }
    }
    let saved = if global {
        cfg.save_user()?
    } else {
        cfg.save_project(root)?
    };
    println!("Saved {}", saved.display());
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        Cli::try_parse_from(["atk", "run", "ci", "--job", "test", "--input", "a=b", "-d"]).unwrap();
        Cli::try_parse_from(["atk", "-C", ".", "watch", "last", "--json"]).unwrap();
        assert!(Cli::try_parse_from(["atk", "run", "ci", "--input", "novalue"]).is_err());
    }

    #[test]
    fn config_keys() {
        let mut cfg = Config::default();
        set_key(&mut cfg, "image.ubuntu-latest", Some("node:20")).unwrap();
        set_key(&mut cfg, "parallel", Some("3")).unwrap();
        assert_eq!(cfg.images["ubuntu-latest"], "node:20");
        assert_eq!(cfg.parallel, Some(3));
        set_key(&mut cfg, "image.ubuntu-latest", None).unwrap();
        assert!(cfg.images.is_empty());
        assert!(set_key(&mut cfg, "parallel", Some("0")).is_err());
        assert!(set_key(&mut cfg, "engine", Some("x")).is_err());
    }
}
