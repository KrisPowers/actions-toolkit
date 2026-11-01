//! Executes a single job: provisions a session, runs each step, collects outputs, tears down.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use indexmap::IndexMap;
use serde_json::{json, Map, Value};
use serde_yaml::Value as Yaml;

use super::{resolve_image, LogKind, RunEvent, RunOptions, Status};
use crate::engine::{ActionImage, Engine, ExecRequest, Session, SessionRequest};
use crate::util::{self, Cancel};
use crate::workflow::action::{self, ActionRef, CacheMode};
use crate::workflow::expr::{self, Ctx, JobState};
use crate::workflow::model::{ActionDef, Step};
use crate::workflow::plan::{step_title, PlannedJob, RunPlan};

pub(crate) struct JobContext {
    pub index: usize,
    pub plan: Arc<RunPlan>,
    pub opts: Arc<RunOptions>,
    pub engine: Arc<dyn Engine>,
    pub tx: Sender<RunEvent>,
    pub cancel: Cancel,
    pub needs: Value,
}

pub(crate) struct JobResult {
    pub status: Status,
    pub outputs: Map<String, Value>,
}

/// Sends events for one job, masking secrets in every log line.
struct Emitter {
    tx: Sender<RunEvent>,
    job: usize,
    masks: RefCell<Vec<String>>,
}

impl Emitter {
    fn log(&self, step: usize, kind: LogKind, text: impl Into<String>) {
        let mut text = text.into();
        for m in self.masks.borrow().iter() {
            if text.contains(m.as_str()) {
                text = text.replace(m.as_str(), "***");
            }
        }
        let _ = self.tx.send(RunEvent::Log {
            job: self.job,
            step,
            kind,
            text,
        });
    }

    fn add_mask(&self, value: &str) {
        let v = value.trim();
        if !v.is_empty() {
            self.masks.borrow_mut().push(v.to_string());
        }
    }

    fn step(&self, step: usize, status: Status) {
        let _ = self.tx.send(RunEvent::Step {
            job: self.job,
            step,
            status,
        });
    }

    fn job(&self, status: Status) {
        let _ = self.tx.send(RunEvent::Job {
            job: self.job,
            status,
        });
    }
}

pub(crate) fn run(cx: JobContext) -> JobResult {
    let pj = &cx.plan.jobs[cx.index];
    let em = Emitter {
        tx: cx.tx.clone(),
        job: cx.index,
        masks: RefCell::new(
            cx.opts
                .secrets
                .values()
                .filter(|v| !v.trim().is_empty())
                .cloned()
                .collect(),
        ),
    };
    let last = pj.step_names.len() - 1;
    em.job(Status::Running);
    em.step(0, Status::Running);

    let session = match setup(&cx, pj, &em) {
        Ok(s) => s,
        Err(e) => {
            let cancelled = cx.cancel.is_cancelled();
            if !cancelled {
                em.log(0, LogKind::Error, format!("{e:#}"));
            }
            let status = if cancelled {
                Status::Cancelled
            } else {
                Status::Failure
            };
            em.step(0, status);
            for i in 1..=last {
                em.step(i, Status::Skipped);
            }
            em.job(status);
            return JobResult {
                status,
                outputs: Map::new(),
            };
        }
    };
    em.step(0, Status::Success);

    let mut job = JobRun::new(&cx, pj, em, session);
    let status = job.execute();
    let outputs = job.outputs();

    job.em.step(last, Status::Running);
    if status == Status::Success {
        job.save_caches(last);
    }
    {
        let JobRun { session, em, .. } = &mut job;
        session.teardown(&mut |l| em.log(last, LogKind::System, l));
    }
    job.em.step(last, Status::Success);
    job.em.job(status);
    JobResult { status, outputs }
}

fn setup(cx: &JobContext, pj: &PlannedJob, em: &Emitter) -> Result<Box<dyn Session>> {
    if let Some(uses) = &pj.job.uses {
        bail!("reusable workflows (`uses: {uses}`) are not supported yet");
    }
    if pj.job.services.is_some() {
        em.log(
            0,
            LogKind::Warning,
            "Service containers (`services:`) are not supported yet; skipping them",
        );
    }
    let engine = &cx.engine;
    em.log(0, LogKind::System, format!("Engine: {}", engine.name()));
    if !pj.runs_on.is_empty() {
        em.log(
            0,
            LogKind::System,
            format!("runs-on: {}", pj.runs_on.join(", ")),
        );
    }
    let event = json!({ "event_name": cx.opts.event });
    let data = json!({ "matrix": pj.matrix, "atk": event, "github": event });
    let ctx = Ctx::new(&data, JobState::default());
    let spec = pj.job.container.as_ref().map(|c| c.spec());

    let mut req = SessionRequest {
        id: format!("{}-{}-{}", cx.opts.run_id, cx.index, util::slug(&pj.label)),
        source: &cx.opts.root,
        image: None,
        container_env: Vec::new(),
        volumes: Vec::new(),
        options: None,
        bind: cx.opts.bind,
        pull: cx.opts.pull,
        cache: cache_store(&cx.opts.root),
        artifacts: cx.opts.artifacts.clone(),
        cancel: cx.cancel.clone(),
    };
    let image = match &spec {
        Some(s) => expr::interpolate(&s.image, &ctx).map_err(|e| anyhow!(e))?,
        None => resolve_image(&cx.opts, &pj.runs_on, &mut |l| {
            em.log(0, LogKind::Warning, l)
        })?,
    };
    em.log(0, LogKind::System, format!("Image: {image}"));
    req.image = Some(image);
    if let Some(s) = &spec {
        for (k, v) in &s.env {
            req.container_env.push((
                k.clone(),
                expr::interpolate_yaml(v, &ctx).map_err(|e| anyhow!(e))?,
            ));
        }
        req.volumes = s.volumes.clone();
        req.options = s.options.clone();
    }
    let started = Instant::now();
    let session = engine.start(&req, &mut |l| em.log(0, LogKind::System, l))?;
    em.log(
        0,
        LogKind::System,
        format!(
            "Environment ready in {}",
            util::fmt_duration(started.elapsed())
        ),
    );
    Ok(session)
}

/// Where a step's commands come from: the job itself, or a composite action.
#[derive(Clone, Default)]
struct Scope {
    nested: bool,
    inputs: Option<Value>,
    action_path: Option<String>,
    env: IndexMap<String, String>,
}

/// What an `actions/cache` step stores: `paths` under `key`.
struct CacheEntry {
    key: String,
    paths: Vec<String>,
}

impl CacheEntry {
    /// Entries only match when they hold the same paths, as in GitHub.
    fn version(&self) -> String {
        format!("{:016x}", util::hash(self.paths.join("\n").as_bytes()))
    }
}

/// A Docker action resolved for one step.
struct DockerRun {
    image: ActionImage,
    entrypoint: Option<String>,
    pre_entrypoint: Option<String>,
    args: Vec<String>,
    env: IndexMap<String, String>,
}

struct StepFiles {
    env: String,
    output: String,
    path: String,
    summary: String,
    state: String,
}

struct JobRun<'a> {
    cx: &'a JobContext,
    pj: &'a PlannedJob,
    em: Emitter,
    session: Box<dyn Session>,
    /// Workflow + job env, plus anything appended to `$ATK_ENV`.
    env: IndexMap<String, String>,
    /// Entries appended to `$ATK_PATH`, most recent first.
    paths: Vec<String>,
    steps: Map<String, Value>,
    failed: bool,
    github: Value,
    runner: Value,
    secrets: Value,
    vars: Value,
    inputs: Value,
    event_path: String,
    copied_actions: HashSet<String>,
    /// `actions/cache` entries to save when the job succeeds.
    caches: Vec<CacheEntry>,
    seq: usize,
    deadline: Option<Instant>,
}

impl<'a> JobRun<'a> {
    fn new(cx: &'a JobContext, pj: &'a PlannedJob, em: Emitter, session: Box<dyn Session>) -> Self {
        let info = session.info().clone();
        let opts = &cx.opts;
        let g = &opts.git;
        let event_path = info.join(&info.temp, "event.json");
        let mut inputs: Map<String, Value> = pj
            .workflow
            .dispatch_inputs()
            .into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect();
        inputs.extend(
            opts.inputs
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone()))),
        );
        let github = json!({
            "event_name": opts.event,
            "event": event_payload(opts, &inputs),
            "event_path": event_path,
            "sha": g.sha,
            "ref": g.git_ref(),
            "ref_name": g.branch,
            "ref_type": "branch",
            "head_ref": "",
            "base_ref": "",
            "repository": g.repository,
            "repository_owner": g.owner,
            "actor": g.actor,
            "triggering_actor": g.actor,
            "workflow": pj.workflow_name,
            "job": pj.key,
            "run_id": opts.run_id,
            "run_number": "1",
            "run_attempt": "1",
            "workspace": info.workspace,
            "token": opts.secrets.get("TOKEN").cloned().unwrap_or_default(),
            "action_path": "",
        });
        let runner = json!({
            "name": "atk",
            "os": info.os,
            "arch": info.arch,
            "temp": info.temp,
            "tool_cache": info.tool_cache,
            "environment": "local",
        });
        let timeout = pj
            .job
            .timeout_minutes
            .as_ref()
            .and_then(|v| v.as_f64())
            .unwrap_or(360.0);
        Self {
            cx,
            pj,
            em,
            session,
            env: IndexMap::new(),
            paths: Vec::new(),
            steps: Map::new(),
            failed: false,
            github,
            runner,
            secrets: expr::object(&opts.secrets),
            vars: expr::object(&opts.vars),
            inputs: Value::Object(inputs),
            event_path,
            copied_actions: HashSet::new(),
            caches: Vec::new(),
            seq: 0,
            deadline: Instant::now().checked_add(Duration::from_secs_f64(timeout.max(0.0) * 60.0)),
        }
    }

    fn state(&self) -> JobState {
        JobState {
            failed: self.failed,
            cancelled: self.cx.cancel.is_cancelled(),
            blocked: false,
        }
    }

    /// An expression context over `data` whose `hashFiles()` reads the job's workspace.
    fn ctx<'d>(&self, data: &'d Value) -> Ctx<'d> {
        Ctx {
            files: Some(self.session.host_workspace().to_path_buf()),
            ..Ctx::new(data, self.state())
        }
    }

    fn job_status(&self) -> Status {
        if self.cx.cancel.is_cancelled() {
            Status::Cancelled
        } else if self.failed {
            Status::Failure
        } else {
            Status::Success
        }
    }

    /// Builds the expression context visible to a step.
    fn ctx_data(&self, scope: &Scope, step_env: &IndexMap<String, String>) -> Value {
        let env: Map<String, Value> = self
            .env
            .iter()
            .chain(scope.env.iter())
            .chain(step_env.iter())
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        let mut github = self.github.clone();
        if let Some(p) = &scope.action_path {
            github["action_path"] = json!(p);
        }
        json!({
            "atk": github,
            "github": github,
            "env": env,
            "matrix": self.pj.matrix,
            "secrets": self.secrets,
            "vars": self.vars,
            "inputs": scope.inputs.clone().unwrap_or_else(|| self.inputs.clone()),
            "steps": Value::Object(self.steps.clone()),
            "needs": self.cx.needs,
            "runner": self.runner,
            "job": { "status": self.job_status().as_str() },
            "strategy": {
                "fail-fast": self.pj.fail_fast,
                "job-index": self.pj.matrix_index,
                "job-total": self.pj.matrix_total,
            },
        })
    }

    fn execute(&mut self) -> Status {
        let payload = self.github["event"].to_string();
        let event_path = self.event_path.clone();
        if let Err(e) = self.session.write_file(&event_path, payload.as_bytes()) {
            self.em.log(
                1,
                LogKind::Warning,
                format!("could not write event payload: {e:#}"),
            );
        }
        if let Err(e) = self.init_env() {
            self.em.log(1, LogKind::Error, format!("{e:#}"));
            self.failed = true;
        }
        let steps = self.pj.job.steps.clone();
        let scope = Scope::default();
        for (n, step) in steps.iter().enumerate() {
            self.run_step(n + 1, step, &scope);
        }
        self.job_status()
    }

    fn init_env(&mut self) -> Result<()> {
        let scope = Scope::default();
        let empty = IndexMap::new();
        for (k, v) in self.pj.workflow.env.iter().chain(self.pj.job.env.iter()) {
            let data = self.ctx_data(&scope, &empty);
            let value =
                expr::interpolate_yaml(v, &self.ctx(&data)).map_err(|e| anyhow!("env.{k}: {e}"))?;
            self.env.insert(k.clone(), value);
        }
        Ok(())
    }

    fn outputs(&self) -> Map<String, Value> {
        let data = self.ctx_data(&Scope::default(), &IndexMap::new());
        let ctx = self.ctx(&data);
        self.pj
            .job
            .outputs
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    Value::String(expr::interpolate(v, &ctx).unwrap_or_default()),
                )
            })
            .collect()
    }

    fn run_step(&mut self, ui: usize, step: &Step, scope: &Scope) -> Status {
        let data = self.ctx_data(scope, &IndexMap::new());
        let ctx = self.ctx(&data);
        let title = step_title(step, &ctx);
        let should_run = match expr::condition(step.condition.as_ref(), &ctx) {
            Ok(b) => b,
            Err(e) => {
                self.em.log(
                    ui,
                    LogKind::Error,
                    format!("invalid `if` on step `{title}`: {e}"),
                );
                self.failed = true;
                if !scope.nested {
                    self.em.step(ui, Status::Failure);
                }
                return Status::Failure;
            }
        };
        if !should_run {
            self.record(step, Status::Skipped, Status::Skipped, Map::new());
            if scope.nested {
                self.em
                    .log(ui, LogKind::System, format!("Skipped: {title}"));
            } else {
                self.em.step(ui, Status::Skipped);
            }
            return Status::Skipped;
        }
        if scope.nested {
            self.em.log(ui, LogKind::Group, title.clone());
        } else {
            self.em.step(ui, Status::Running);
            self.em.log(ui, LogKind::Header, title.clone());
        }
        let (outcome, outputs) = match self.run_step_inner(ui, step, scope) {
            Ok(r) => r,
            Err(e) => {
                self.em.log(ui, LogKind::Error, format!("{e:#}"));
                (Status::Failure, Map::new())
            }
        };
        let continue_on_error = step
            .continue_on_error
            .as_ref()
            .map(|v| {
                expr::evaluate_yaml(v, &ctx)
                    .map(|v| expr::truthy(&v))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        let conclusion = if outcome == Status::Failure && continue_on_error {
            self.em.log(
                ui,
                LogKind::Notice,
                "Step failed but continue-on-error is set",
            );
            Status::Success
        } else {
            outcome
        };
        if conclusion == Status::Failure {
            self.failed = true;
        }
        self.record(step, outcome, conclusion, outputs);
        if !scope.nested {
            self.em.step(ui, conclusion);
        }
        conclusion
    }

    fn record(
        &mut self,
        step: &Step,
        outcome: Status,
        conclusion: Status,
        outputs: Map<String, Value>,
    ) {
        if let Some(id) = &step.id {
            self.steps.insert(
                id.clone(),
                json!({ "outcome": outcome.as_str(), "conclusion": conclusion.as_str(), "outputs": outputs }),
            );
        }
    }

    fn run_step_inner(
        &mut self,
        ui: usize,
        step: &Step,
        scope: &Scope,
    ) -> Result<(Status, Map<String, Value>)> {
        let mut step_env = IndexMap::new();
        {
            let data = self.ctx_data(scope, &IndexMap::new());
            let ctx = self.ctx(&data);
            for (k, v) in &step.env {
                step_env.insert(
                    k.clone(),
                    expr::interpolate_yaml(v, &ctx).map_err(|e| anyhow!("env.{k}: {e}"))?,
                );
            }
        }
        let data = self.ctx_data(scope, &step_env);
        let ctx = self.ctx(&data);
        let mut timeout = step
            .timeout_minutes
            .as_ref()
            .and_then(|v| expr::evaluate_yaml(v, &ctx).ok())
            .and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .map(|m| Duration::from_secs_f64(m.max(0.0) * 60.0));
        if let Some(deadline) = self.deadline {
            let left = deadline.saturating_duration_since(Instant::now());
            timeout = Some(timeout.map_or(left, |t| t.min(left)));
        }
        let mut env: IndexMap<String, String> = scope.env.clone();
        env.extend(step_env);

        if let Some(run) = &step.run {
            let script = expr::interpolate(run, &ctx).map_err(|e| anyhow!(e))?;
            let wd = step.working_directory.clone().or_else(|| {
                if scope.nested {
                    None
                } else {
                    self.pj
                        .job
                        .run_defaults()
                        .and_then(|d| d.working_directory.clone())
                        .or_else(|| {
                            self.pj
                                .workflow
                                .run_defaults()
                                .and_then(|d| d.working_directory.clone())
                        })
                }
            });
            let wd = wd
                .map(|w| expr::interpolate(&w, &ctx))
                .transpose()
                .map_err(|e| anyhow!(e))?;
            let shell = self.shell_for(step, scope);
            self.exec_script(ui, &script, &shell, wd.as_deref(), env, timeout)
        } else if let Some(uses) = &step.uses {
            let uses = expr::interpolate(uses, &ctx).map_err(|e| anyhow!(e))?;
            self.exec_action(ui, &uses, step, &ctx, env, timeout)
        } else {
            bail!("step has neither `run` nor `uses`")
        }
    }

    fn shell_for(&self, step: &Step, scope: &Scope) -> String {
        step.shell
            .clone()
            .or_else(|| {
                if scope.nested {
                    return None;
                }
                self.pj
                    .job
                    .run_defaults()
                    .and_then(|d| d.shell.clone())
                    .or_else(|| {
                        self.pj
                            .workflow
                            .run_defaults()
                            .and_then(|d| d.shell.clone())
                    })
            })
            .unwrap_or_else(|| self.session.info().default_shell.clone())
    }

    fn next_seq(&mut self) -> usize {
        self.seq += 1;
        self.seq
    }

    fn exec_script(
        &mut self,
        ui: usize,
        script: &str,
        shell: &str,
        wd: Option<&str>,
        env: IndexMap<String, String>,
        timeout: Option<Duration>,
    ) -> Result<(Status, Map<String, Value>)> {
        let info = self.session.info().clone();
        let seq = self.next_seq();
        let path = info.join(
            &info.join(&info.temp, "scripts"),
            &format!("step-{seq}{}", script_ext(shell)),
        );
        self.session
            .write_file(&path, wrap_script(shell, script).as_bytes())?;
        for line in script.lines().filter(|l| !l.trim().is_empty()).take(40) {
            self.em.log(ui, LogKind::Command, line.to_string());
        }
        let cwd = match wd {
            Some(w) if info.is_absolute(w) => w.to_string(),
            Some(w) => info.join(&info.workspace, w),
            None => info.workspace.clone(),
        };
        let req = ExecRequest {
            argv: shell_argv(shell, &path),
            cwd,
            timeout,
            ..Default::default()
        };
        self.exec_with_files(ui, req, env)
    }

    /// Runs a command with the step file protocol (`$ATK_ENV`, `$ATK_OUTPUT`, ...).
    fn exec_with_files(
        &mut self,
        ui: usize,
        mut req: ExecRequest,
        extra_env: IndexMap<String, String>,
    ) -> Result<(Status, Map<String, Value>)> {
        let info = self.session.info().clone();
        let seq = self.next_seq();
        let dir = info.join(&info.temp, "_files");
        let file = |name: &str| info.join(&dir, &format!("{name}-{seq}"));
        let files = StepFiles {
            env: file("env"),
            output: file("output"),
            path: file("path"),
            summary: file("summary"),
            state: file("state"),
        };
        // Shells create the files with `>>`, but `@actions/core` refuses to write to a file
        // that doesn't exist yet, so every step file must exist before the command runs.
        for f in [
            &files.env,
            &files.output,
            &files.path,
            &files.summary,
            &files.state,
        ] {
            self.session.write_file(f, b"")?;
        }

        let mut env: IndexMap<String, String> = self.base_env(&files).into_iter().collect();
        env.extend(self.env.clone());
        env.extend(extra_env);
        // A Docker action's image has its own PATH.
        if !self.paths.is_empty() && req.image.is_none() {
            let base = env
                .get("PATH")
                .cloned()
                .unwrap_or_else(|| info.path.clone());
            let mut parts = self.paths.clone();
            parts.push(base);
            env.insert("PATH".into(), parts.join(info.path_separator()));
        }
        req.env = env.into_iter().collect();

        let mut outputs = Map::new();
        let outcome = {
            let JobRun {
                session, em, cx, ..
            } = self;
            session.exec(&req, &cx.cancel, &mut |line| {
                handle_output(em, ui, line, &mut outputs)
            })?
        };

        for (k, v) in parse_file_commands(&self.session.read_file(&files.env)?) {
            self.env.insert(k, v);
        }
        for (k, v) in parse_file_commands(&self.session.read_file(&files.output)?) {
            outputs.insert(k, Value::String(v));
        }
        for p in self
            .session
            .read_file(&files.path)?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
        {
            self.paths.insert(0, p.to_string());
        }

        let status = if outcome.cancelled {
            self.em.log(ui, LogKind::Warning, "Cancelled");
            Status::Cancelled
        } else if outcome.timed_out {
            self.em.log(ui, LogKind::Error, "Timed out");
            Status::Failure
        } else if outcome.code != 0 {
            self.em.log(
                ui,
                LogKind::Error,
                format!("Process exited with code {}", outcome.code),
            );
            Status::Failure
        } else {
            Status::Success
        };
        Ok((status, outputs))
    }

    fn base_env(&self, files: &StepFiles) -> Vec<(String, String)> {
        let opts = &self.cx.opts;
        let g = &opts.git;
        let info = self.session.info();
        let vars = [
            ("WORKSPACE", info.workspace.clone()),
            ("REPOSITORY", g.repository.clone()),
            ("REPOSITORY_OWNER", g.owner.clone()),
            ("SHA", g.sha.clone()),
            ("REF", g.git_ref()),
            ("REF_NAME", g.branch.clone()),
            ("REF_TYPE", "branch".into()),
            ("EVENT_NAME", opts.event.clone()),
            ("EVENT_PATH", self.event_path.clone()),
            ("JOB", self.pj.key.clone()),
            ("WORKFLOW", self.pj.workflow_name.clone()),
            ("RUN_ID", opts.run_id.clone()),
            ("RUN_NUMBER", "1".into()),
            ("RUN_ATTEMPT", "1".into()),
            ("ACTOR", g.actor.clone()),
            ("ENV", files.env.clone()),
            ("OUTPUT", files.output.clone()),
            ("PATH", files.path.clone()),
            ("STEP_SUMMARY", files.summary.clone()),
            ("STATE", files.state.clone()),
        ];
        // `ATK_*` is the native naming; the `GITHUB_*` aliases let workflow files written
        // in that format run unchanged.
        let mut env: Vec<(String, String)> = vec![
            ("CI".into(), "true".into()),
            ("ATK".into(), "true".into()),
            ("GITHUB_ACTIONS".into(), "true".into()),
        ];
        for (name, value) in vars {
            env.push((format!("ATK_{name}"), value.clone()));
            env.push((format!("GITHUB_{name}"), value));
        }
        env.extend(
            [
                ("RUNNER_OS", info.os.to_string()),
                ("RUNNER_ARCH", info.arch.clone()),
                ("RUNNER_TEMP", info.temp.clone()),
                ("RUNNER_TOOL_CACHE", info.tool_cache.clone()),
                ("RUNNER_NAME", "atk".into()),
                ("RUNNER_ENVIRONMENT", "local".into()),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v)),
        );
        env
    }

    fn exec_action(
        &mut self,
        ui: usize,
        uses: &str,
        step: &Step,
        ctx: &Ctx,
        env: IndexMap<String, String>,
        timeout: Option<Duration>,
    ) -> Result<(Status, Map<String, Value>)> {
        let info = self.session.info().clone();
        let (host_dir, env_dir) = match action::parse(uses)? {
            ActionRef::Checkout => {
                self.em.log(
                    ui,
                    LogKind::System,
                    "Workspace is already provisioned by ATK; nothing to check out",
                );
                return Ok((Status::Success, Map::new()));
            }
            ActionRef::Cache(mode) => return self.exec_cache(ui, mode, step, ctx, timeout),
            ActionRef::Upload => return self.exec_upload(ui, step, ctx),
            ActionRef::Download => return self.exec_download(ui, step, ctx),
            ActionRef::Docker(image) => {
                let inputs = action_inputs(&IndexMap::new(), step, ctx)?;
                let run = DockerRun {
                    image: ActionImage::Pull(image),
                    entrypoint: None,
                    pre_entrypoint: None,
                    args: Vec::new(),
                    env: IndexMap::new(),
                };
                let env = action_env(env, &inputs, step, ui);
                return self.exec_docker(ui, run, &inputs, env, timeout);
            }
            ActionRef::Local(path) => (
                self.cx.opts.root.join(&path),
                info.join(&info.workspace, &path),
            ),
            ActionRef::Remote {
                owner,
                repo,
                path,
                git_ref,
            } => {
                // A step that can't run must not let the job pass as if it had.
                let Some(host_root) = action::locate(&self.cx.opts.root, &owner, &repo, &git_ref)
                else {
                    bail!(
                        "{uses} can't run: ATK runs offline and has no local copy \
                         (put one in .atk/actions/{owner}/{repo})"
                    );
                };
                let env_root = info.join(
                    &info.join(
                        &info.join(&info.temp, "_actions"),
                        &format!("{owner}/{repo}"),
                    ),
                    &util::slug(&git_ref),
                );
                if self.copied_actions.insert(env_root.clone()) {
                    self.session.copy_dir(&host_root, &env_root)?;
                }
                match path {
                    Some(p) => (host_root.join(&p), info.join(&env_root, &p)),
                    None => (host_root, env_root),
                }
            }
        };
        let def = action::load(&host_dir)?;
        let inputs = action_inputs(&def.inputs, step, ctx)?;
        let using = def.runs.using.to_ascii_lowercase();
        if using == "composite" {
            return self.run_composite(ui, &def, inputs, env_dir, env);
        }
        let docker = if using == "docker" {
            Some(self.docker_run(&def, &host_dir, &inputs, &env)?)
        } else {
            None
        };
        let mut env = action_env(env, &inputs, step, ui);
        env.insert("ATK_ACTION_PATH".into(), env_dir.clone());
        env.insert("GITHUB_ACTION_PATH".into(), env_dir.clone());
        if let Some(run) = docker {
            return self.exec_docker(ui, run, &inputs, env, timeout);
        }
        if using.starts_with("node") {
            let main = def
                .runs
                .main
                .clone()
                .context("node action has no `runs.main`")?;
            let req = ExecRequest {
                argv: vec!["node".to_string(), info.join(&env_dir, &main)],
                cwd: info.workspace.clone(),
                timeout,
                ..Default::default()
            };
            return self.exec_with_files(ui, req, env);
        }
        bail!(
            "{uses} can't run: `{}` actions are not supported yet",
            def.runs.using
        )
    }

    /// `actions/cache` and its `restore` and `save` halves. Entries live in a host folder per
    /// project, shared with the job's container, as plain tar archives of absolute paths.
    fn exec_cache(
        &mut self,
        ui: usize,
        mode: CacheMode,
        step: &Step,
        ctx: &Ctx,
        timeout: Option<Duration>,
    ) -> Result<(Status, Map<String, Value>)> {
        let inputs = action_inputs(&IndexMap::new(), step, ctx)?;
        let input = |name: &str| inputs.get(name).map(expr::to_str).unwrap_or_default();
        let lines = |name: &str| input_lines(&input(name));
        let entry = CacheEntry {
            key: input("key").trim().to_string(),
            paths: lines("path"),
        };
        if entry.key.is_empty() || entry.paths.is_empty() {
            bail!("actions/cache needs `path` and `key`");
        }
        if mode == CacheMode::Save {
            return Ok((self.save_cache(ui, &entry)?, Map::new()));
        }
        let found = self.find_cache(&entry, &lines("restore-keys"));
        let exact = found.as_ref().is_some_and(|(key, _)| *key == entry.key);
        let mut outputs = Map::new();
        outputs.insert("cache-hit".into(), json!(exact.to_string()));
        outputs.insert("cache-primary-key".into(), json!(entry.key));
        match found {
            None if input("fail-on-cache-miss") == "true" => {
                bail!("no cache entry for key {}", entry.key)
            }
            None => self.em.log(
                ui,
                LogKind::System,
                format!("Cache not found for key: {}", entry.key),
            ),
            Some((key, archive)) => {
                outputs.insert("cache-matched-key".into(), json!(key));
                if input("lookup-only") == "true" {
                    self.em
                        .log(ui, LogKind::System, format!("Cache found for key: {key}"));
                } else {
                    self.em.log(
                        ui,
                        LogKind::System,
                        format!("Restoring cache from key: {key}"),
                    );
                    let req = ExecRequest {
                        argv: ["tar", "-xf", &archive, "-C", "/"].map(String::from).into(),
                        cwd: self.session.info().workspace.clone(),
                        timeout,
                        ..Default::default()
                    };
                    let (status, _) = self.exec_with_files(ui, req, IndexMap::new())?;
                    if status != Status::Success {
                        return Ok((status, outputs));
                    }
                }
            }
        }
        if mode == CacheMode::Both && !exact {
            self.caches.push(entry);
        }
        Ok((Status::Success, outputs))
    }

    /// The entry for `key`, or else the newest one starting with the first restore key that
    /// matches any: its key and its archive inside the job.
    fn find_cache(&self, entry: &CacheEntry, restore_keys: &[String]) -> Option<(String, String)> {
        let info = self.session.info();
        let dir = info.join(info.cache.as_ref()?, &entry.version());
        let store = cache_store(&self.cx.opts.root)?.join(entry.version());
        let archive = |name: &str| info.join(&dir, &format!("{name}.tar"));
        let exact = cache_file_name(&entry.key);
        if store.join(format!("{exact}.tar")).is_file() {
            return Some((entry.key.clone(), archive(&exact)));
        }
        let names: Vec<(std::time::SystemTime, String)> = fs::read_dir(&store)
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let name = name.strip_suffix(".tar")?.to_string();
                Some((e.metadata().ok()?.modified().ok()?, name))
            })
            .collect();
        restore_keys.iter().find_map(|k| {
            let prefix = cache_file_name(k);
            let (_, name) = names
                .iter()
                .filter(|(_, n)| n.starts_with(&prefix))
                .max_by_key(|(t, _)| *t)?;
            Some((decode_cache_name(name), archive(name)))
        })
    }

    /// Archives `entry.paths` from inside the job into its store.
    fn save_cache(&mut self, ui: usize, entry: &CacheEntry) -> Result<Status> {
        let info = self.session.info().clone();
        let dir = info
            .cache
            .as_ref()
            .context("there is no cache folder to save to")?;
        let archive = info.join(
            &info.join(dir, &entry.version()),
            &format!("{}.tar", cache_file_name(&entry.key)),
        );
        self.em.log(
            ui,
            LogKind::System,
            format!("Saving cache for key: {}", entry.key),
        );
        let mut argv: Vec<String> = ["sh", "-c", SAVE_CACHE, "sh", &archive]
            .map(String::from)
            .into();
        argv.extend(entry.paths.iter().cloned());
        let req = ExecRequest {
            argv,
            cwd: info.workspace.clone(),
            ..Default::default()
        };
        Ok(self.exec_with_files(ui, req, IndexMap::new())?.0)
    }

    /// Saves what `actions/cache` steps didn't find, as their post step does on success.
    fn save_caches(&mut self, ui: usize) {
        for entry in std::mem::take(&mut self.caches) {
            let failed = match self.save_cache(ui, &entry) {
                Ok(status) => status != Status::Success,
                Err(e) => {
                    self.em.log(ui, LogKind::Warning, format!("{e:#}"));
                    true
                }
            };
            if failed {
                self.em.log(
                    ui,
                    LogKind::Warning,
                    format!("Could not save cache for key: {}", entry.key),
                );
            }
        }
    }

    /// `actions/upload-artifact`: copies the matched files into the run's artifacts folder,
    /// relative to the folder the paths share, as the action stores them.
    fn exec_upload(
        &mut self,
        ui: usize,
        step: &Step,
        ctx: &Ctx,
    ) -> Result<(Status, Map<String, Value>)> {
        let inputs = action_inputs(&IndexMap::new(), step, ctx)?;
        let input = |name: &str| {
            let v = inputs.get(name).map(expr::to_str).unwrap_or_default();
            v.trim().to_string()
        };
        let name = Some(input("name"))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "artifact".into());
        check_artifact_name(&name)?;
        let paths = input_lines(&input("path"));
        if paths.is_empty() {
            bail!("actions/upload-artifact needs `path`");
        }
        let info = self.session.info().clone();
        let (Some(host), Some(dir)) = (&self.cx.opts.artifacts, &info.artifacts) else {
            self.em.log(
                ui,
                LogKind::Warning,
                "This run has no history folder to keep artifacts in; nothing was uploaded",
            );
            return Ok((Status::Skipped, Map::new()));
        };
        let host = host.join(&name);
        if host.exists() && input("overwrite") != "true" {
            bail!(
                "an artifact named `{name}` already exists in this run \
                 (set `overwrite: true` to replace it)"
            );
        }
        let hidden = input("include-hidden-files");
        let dest = info.join(dir, &name);
        let mut argv: Vec<String> = ["sh", "-c", UPLOAD_ARTIFACT, "sh", &dest, &hidden]
            .map(String::from)
            .into();
        argv.extend(paths.iter().cloned());
        let req = ExecRequest {
            argv,
            cwd: info.workspace.clone(),
            ..Default::default()
        };
        let (status, out) = self.exec_with_files(ui, req, IndexMap::new())?;
        if status != Status::Success {
            return Ok((status, Map::new()));
        }
        let files: usize = out
            .get("files")
            .and_then(|n| expr::to_str(n).parse().ok())
            .unwrap_or(0);
        if files == 0 {
            let msg = format!(
                "No files were found with the provided path: {}. No artifacts will be uploaded.",
                paths.join(", ")
            );
            match input("if-no-files-found").as_str() {
                "error" => bail!(msg),
                "ignore" => self.em.log(ui, LogKind::System, msg),
                _ => self.em.log(ui, LogKind::Warning, msg),
            }
            return Ok((Status::Success, Map::new()));
        }
        self.em.log(
            ui,
            LogKind::System,
            format!(
                "Kept {files} file(s) as artifact `{name}` in {}",
                host.display()
            ),
        );
        let mut outputs = Map::new();
        outputs.insert("artifact-url".into(), json!(host.display().to_string()));
        Ok((Status::Success, outputs))
    }

    /// `actions/download-artifact`: copies artifacts this run uploaded into the job, one by
    /// name into `path`, or all (or those matching `pattern`) into a folder each.
    fn exec_download(
        &mut self,
        ui: usize,
        step: &Step,
        ctx: &Ctx,
    ) -> Result<(Status, Map<String, Value>)> {
        let inputs = action_inputs(&IndexMap::new(), step, ctx)?;
        let input = |name: &str| {
            let v = inputs.get(name).map(expr::to_str).unwrap_or_default();
            v.trim().to_string()
        };
        let run_id = input("run-id");
        if !run_id.is_empty() && run_id != self.cx.opts.run_id {
            bail!("only this run's artifacts can be downloaded locally, not run {run_id}'s");
        }
        let info = self.session.info().clone();
        let (Some(host), Some(dir)) = (&self.cx.opts.artifacts, &info.artifacts) else {
            bail!("this run has no history folder, so it keeps no artifacts to download");
        };
        let name = input("name");
        let (names, nested) = if name.is_empty() {
            let pattern = input("pattern");
            let mut names: Vec<String> = fs::read_dir(host)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| !n.ends_with(".partial"))
                .filter(|n| pattern.is_empty() || wildcard(&pattern, n))
                .collect();
            names.sort();
            (names, input("merge-multiple") != "true")
        } else {
            check_artifact_name(&name)?;
            if !host.join(&name).is_dir() {
                bail!("Artifact not found for name: {name}");
            }
            (vec![name], false)
        };
        if names.is_empty() {
            self.em
                .log(ui, LogKind::System, "No artifacts in this run to download");
        }
        let path = input("path");
        let mode = if nested { "nested" } else { "flat" };
        let mut argv: Vec<String> = ["sh", "-c", DOWNLOAD_ARTIFACT, "sh", &path, mode]
            .map(String::from)
            .into();
        for n in &names {
            self.em
                .log(ui, LogKind::System, format!("Downloading artifact `{n}`"));
            argv.push(info.join(dir, n));
        }
        let req = ExecRequest {
            argv,
            cwd: info.workspace.clone(),
            ..Default::default()
        };
        self.exec_with_files(ui, req, IndexMap::new())
    }

    /// Resolves a Docker action's image and entrypoints, and its `args` and `env`, which see
    /// the action's inputs.
    fn docker_run(
        &self,
        def: &ActionDef,
        dir: &Path,
        inputs: &Map<String, Value>,
        env: &IndexMap<String, String>,
    ) -> Result<DockerRun> {
        let image = def
            .runs
            .image
            .as_deref()
            .context("docker action has no `runs.image`")?;
        let image = match image.strip_prefix("docker://") {
            Some(name) => ActionImage::Pull(name.to_string()),
            None => ActionImage::Build(dir.join(image)),
        };
        let scope = Scope {
            inputs: Some(Value::Object(inputs.clone())),
            ..Default::default()
        };
        let data = self.ctx_data(&scope, env);
        let ctx = self.ctx(&data);
        let eval = |v: &Yaml| expr::interpolate_yaml(v, &ctx).map_err(|e| anyhow!(e));
        Ok(DockerRun {
            image,
            entrypoint: def.runs.entrypoint.clone(),
            pre_entrypoint: def.runs.pre_entrypoint.clone(),
            args: def.runs.args.iter().map(&eval).collect::<Result<_>>()?,
            env: def
                .runs
                .env
                .iter()
                .map(|(k, v)| eval(v).map(|v| (k.clone(), v)))
                .collect::<Result<_>>()?,
        })
    }

    fn exec_docker(
        &mut self,
        ui: usize,
        run: DockerRun,
        inputs: &Map<String, Value>,
        env: IndexMap<String, String>,
        timeout: Option<Duration>,
    ) -> Result<(Status, Map<String, Value>)> {
        // The action's own `entrypoint` and `args` win; inputs of those names stand in for
        // actions without them, such as `docker://` steps.
        let input = |name: &str| {
            inputs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| expr::to_str(v))
                .filter(|v| !v.is_empty())
        };
        let entrypoint = run.entrypoint.or_else(|| input("entrypoint"));
        let args = if run.args.is_empty() {
            input("args").map(|a| split_args(&a)).unwrap_or_default()
        } else {
            run.args
        };
        let image = {
            let JobRun {
                session, em, cx, ..
            } = self;
            session.prepare_image(&run.image, &cx.cancel, &mut |l| {
                em.log(ui, LogKind::System, l)
            })?
        };
        self.em.log(
            ui,
            LogKind::Command,
            format!("docker run {image} {}", args.join(" ")),
        );
        let mut env_all = run.env;
        env_all.extend(env);
        let workspace = self.session.info().workspace.clone();
        let req = |entrypoint: Option<String>| ExecRequest {
            argv: args.clone(),
            cwd: workspace.clone(),
            timeout,
            image: Some(image.clone()),
            entrypoint,
            ..Default::default()
        };
        if let Some(pre) = run.pre_entrypoint {
            let (status, _) = self.exec_with_files(ui, req(Some(pre)), env_all.clone())?;
            if status != Status::Success {
                return Ok((status, Map::new()));
            }
        }
        self.exec_with_files(ui, req(entrypoint), env_all)
    }

    fn run_composite(
        &mut self,
        ui: usize,
        def: &ActionDef,
        inputs: Map<String, Value>,
        action_dir: String,
        env: IndexMap<String, String>,
    ) -> Result<(Status, Map<String, Value>)> {
        let mut env = env;
        env.insert("ATK_ACTION_PATH".into(), action_dir.clone());
        env.insert("GITHUB_ACTION_PATH".into(), action_dir.clone());
        let scope = Scope {
            nested: true,
            inputs: Some(Value::Object(inputs)),
            action_path: Some(action_dir),
            env,
        };
        let saved_steps = std::mem::take(&mut self.steps);
        let saved_failed = std::mem::replace(&mut self.failed, false);
        for step in &def.runs.steps {
            self.run_step(ui, step, &scope);
        }
        let data = self.ctx_data(&scope, &IndexMap::new());
        let ctx = self.ctx(&data);
        let outputs = def
            .outputs
            .iter()
            .filter_map(|(name, o)| {
                let value = o.get("value").and_then(|v| v.as_str())?;
                Some((
                    name.clone(),
                    Value::String(expr::interpolate(value, &ctx).unwrap_or_default()),
                ))
            })
            .collect();
        let status = self.job_status();
        self.steps = saved_steps;
        self.failed = saved_failed;
        Ok((status, outputs))
    }
}

fn event_payload(opts: &RunOptions, inputs: &Map<String, Value>) -> Value {
    if let Some(p) = &opts.event_payload {
        return p.clone();
    }
    let g = &opts.git;
    let (owner, name) = g
        .repository
        .split_once('/')
        .unwrap_or(("", g.repository.as_str()));
    let mut payload = json!({
        "ref": g.git_ref(),
        "after": g.sha,
        "repository": { "full_name": g.repository, "name": name, "owner": { "login": owner }, "default_branch": g.branch },
        "sender": { "login": g.actor },
    });
    if opts.event == "workflow_dispatch" {
        payload["inputs"] = Value::Object(inputs.clone());
    }
    payload
}

/// Where `actions/cache` keeps a project's entries on the host.
fn cache_store(root: &Path) -> Option<PathBuf> {
    let name = root
        .file_name()
        .map(|n| util::slug(&n.to_string_lossy()))
        .unwrap_or_default();
    let id = util::hash(root.to_string_lossy().as_bytes());
    let dir = dirs::cache_dir()?.join("atk").join("cache");
    Some(dir.join(format!("{name}-{id:016x}")))
}

/// A cache key as a file name: safe bytes kept, others `%XX`-escaped, so a restore key that
/// prefixes a key also prefixes its file name. Overlong keys fall back to their hash.
fn cache_file_name(key: &str) -> String {
    let mut name = String::new();
    for b in key.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.".contains(&b) {
            name.push(char::from(b));
        } else {
            name.push_str(&format!("%{b:02X}"));
        }
    }
    if name.len() > 200 {
        return format!("{:016x}", util::hash(key.as_bytes()));
    }
    name
}

fn decode_cache_name(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
            Some(b) if bytes[i] == b'%' => {
                out.push(b);
                i += 3;
            }
            _ => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Archives the cache paths given after the archive name, relative to `/` so they restore
/// in place. `~` is the job's home, other relative paths are under the workspace, globs
/// expand, and `!pattern` excludes. Paths are stored with symlinks resolved: tar refuses
/// to extract through a symlink to an absolute target, such as `~/.cargo` in some images.
const SAVE_CACHE: &str = r#"set -e
archive="$1"; shift
mkdir -p "$(dirname "$archive")"
: > "$archive.list"; : > "$archive.exclude"
IFS='
'
for p in "$@"; do
  list="$archive.list"
  case "$p" in '!'*) list="$archive.exclude"; p="${p#!}" ;; esac
  case "$p" in '~') p="$HOME" ;; '~/'*) p="$HOME/${p#'~/'}" ;; esac
  case "$p" in /*) ;; *) p="$PWD/$p" ;; esac
  if [ "$list" = "$archive.exclude" ]; then printf '%s\n' "${p#/}" >> "$list"; fi
  for f in $p; do
    if [ -e "$f" ]; then f="$(readlink -f "$f")"; printf '%s\n' "${f#/}" >> "$list"; fi
  done
done
if [ -s "$archive.list" ]; then
  tar -cf "$archive.tmp" -C / -X "$archive.exclude" -T "$archive.list"
  mv "$archive.tmp" "$archive"
  chmod a+rw "$archive" 2>/dev/null || true
else
  echo "No files matched the cache paths; nothing to save"
fi
rm -f "$archive.list" "$archive.exclude"
"#;

/// Copies the files matched by the paths given after the artifact folder and the
/// `include-hidden-files` flag into that folder, relative to the folder the paths share (or
/// a lone file's own folder). Paths resolve like cache paths, directories bring everything
/// under them, and `!pattern` excludes. Writes the file count to the step's outputs.
const UPLOAD_ARTIFACT: &str = r#"set -e
dest="$1"; hidden="$2"; shift 2
work="$(mktemp -d)"; tmp="$dest.partial"
: > "$work/files"; : > "$work/exclude"; : > "$work/search"
IFS='
'
for p in "$@"; do
  neg=; case "$p" in '!'*) neg=1; p="${p#!}" ;; esac
  case "$p" in '~') p="$HOME" ;; '~/'*) p="$HOME/${p#'~/'}" ;; esac
  case "$p" in /*) ;; *) p="$PWD/${p#./}" ;; esac
  while [ "$p" != / ] && [ "${p%/}" != "$p" ]; do p="${p%/}"; done
  if [ -n "$neg" ]; then printf '%s\n' "$p" >> "$work/exclude"; continue; fi
  sp=; rest="${p#/}"
  while [ -n "$rest" ]; do
    seg="${rest%%/*}"
    case "$rest" in */*) rest="${rest#*/}" ;; *) rest= ;; esac
    case "$seg" in *[*?[]*) break ;; esac
    sp="$sp/$seg"
  done
  sp="${sp:-/}"
  printf '%s\n' "$sp" >> "$work/search"
  for f in $p; do
    [ -e "$f" ] || continue
    find -L "$f" ! -type d | while IFS= read -r m; do
      [ -e "$m" ] || continue
      if [ "$hidden" != true ]; then case "/${m#"$sp"}" in */.*) continue ;; esac; fi
      printf '%s\n' "$m"
    done >> "$work/files"
  done
done
sort -u "$work/files" -o "$work/files"
root=
while IFS= read -r s; do
  if [ -z "$root" ]; then root="$s"; continue; fi
  while :; do
    case "$s/" in "${root%/}/"*) break ;; esac
    root="${root%/*}"; root="${root:-/}"
  done
done < "$work/search"
[ -d "$root" ] || root="$(dirname "$root")"
rm -rf "$tmp"; mkdir -p "$tmp"
n=0
while IFS= read -r f; do
  skip=
  while IFS= read -r x; do
    case "$f" in $x|$x/*) skip=1; break ;; esac
  done < "$work/exclude"
  [ -z "$skip" ] || continue
  rel="${f#"${root%/}/"}"
  mkdir -p "$tmp/$(dirname "$rel")"
  cp -L "$f" "$tmp/$rel"
  n=$((n + 1))
done < "$work/files"
rm -rf "$work"
if [ "$n" -gt 0 ]; then
  rm -rf "$dest"; mv "$tmp" "$dest"
  chmod -R a+rwX "$dest" 2>/dev/null || true
else
  rm -rf "$tmp"
fi
echo "files=$n" >> "$GITHUB_OUTPUT"
"#;

/// Copies the artifact folders given after the destination and the mode into the
/// destination (`flat`), or into a folder per artifact under it (`nested`). The destination
/// resolves like a cache path and defaults to the workspace.
const DOWNLOAD_ARTIFACT: &str = r#"set -e
out="$1"; mode="$2"; shift 2
case "$out" in '') out="$PWD" ;; '~') out="$HOME" ;; '~/'*) out="$HOME/${out#'~/'}" ;; esac
case "$out" in /*) ;; *) out="$PWD/${out#./}" ;; esac
for a in "$@"; do
  d="$out"
  if [ "$mode" = nested ]; then d="$out/${a##*/}"; fi
  mkdir -p "$d"
  cp -R "$a/." "$d/"
done
echo "download-path=$out" >> "$GITHUB_OUTPUT"
"#;

/// Artifact names become folder names, so they can't hold path or reserved characters.
fn check_artifact_name(name: &str) -> Result<()> {
    if name.chars().any(|c| "\"\\/:<>|*?\r\n".contains(c))
        || name == "."
        || name == ".."
        || name.ends_with(".partial")
    {
        bail!("`{name}` is not a valid artifact name");
    }
    Ok(())
}

/// Matches `text` against a pattern where `*` is any run of characters and `?` one.
fn wildcard(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// A multi-line input's non-empty lines, trimmed.
fn input_lines(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// Adds what a JavaScript or Docker action reads: its inputs as `INPUT_*`, and its step id.
fn action_env(
    mut env: IndexMap<String, String>,
    inputs: &Map<String, Value>,
    step: &Step,
    ui: usize,
) -> IndexMap<String, String> {
    for (k, v) in inputs {
        env.insert(
            format!("INPUT_{}", k.to_uppercase().replace(' ', "_")),
            expr::to_str(v),
        );
    }
    env.insert(
        "GITHUB_ACTION".into(),
        step.id.clone().unwrap_or_else(|| format!("__step{ui}")),
    );
    env
}

/// Splits a command line on whitespace, keeping quoted parts together.
fn split_args(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut arg: Option<String> = None;
    let mut quote = None;
    for c in line.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => arg.get_or_insert_with(String::new).push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                arg.get_or_insert_with(String::new);
            }
            None if c.is_whitespace() => args.extend(arg.take()),
            None => arg.get_or_insert_with(String::new).push(c),
        }
    }
    args.extend(arg);
    args
}

fn action_inputs(
    declared: &IndexMap<String, Yaml>,
    step: &Step,
    ctx: &Ctx,
) -> Result<Map<String, Value>> {
    let mut inputs = Map::new();
    for (name, spec) in declared {
        let given = step
            .with
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v);
        let value = match (given, spec.get("default")) {
            (Some(v), _) | (None, Some(v)) => {
                expr::interpolate_yaml(v, ctx).map_err(|e| anyhow!("input {name}: {e}"))?
            }
            (None, None) => String::new(),
        };
        inputs.insert(name.clone(), Value::String(value));
    }
    for (k, v) in &step.with {
        if !inputs.keys().any(|n| n.eq_ignore_ascii_case(k)) {
            inputs.insert(
                k.clone(),
                Value::String(expr::interpolate_yaml(v, ctx).map_err(|e| anyhow!(e))?),
            );
        }
    }
    Ok(inputs)
}

/// Interprets workflow commands (`::error::`, `::add-mask::`, ...) in a line of step output.
fn handle_output(em: &Emitter, ui: usize, line: String, outputs: &mut Map<String, Value>) {
    let Some(cmd) = line.trim_start().strip_prefix("::") else {
        em.log(ui, LogKind::Output, line);
        return;
    };
    let Some((head, value)) = cmd.split_once("::") else {
        em.log(ui, LogKind::Output, line);
        return;
    };
    let (name, params) = head.split_once(' ').unwrap_or((head, ""));
    match name {
        "add-mask" => em.add_mask(value),
        "set-output" => {
            if let Some(key) = params.trim().strip_prefix("name=") {
                outputs.insert(key.to_string(), Value::String(value.to_string()));
            }
        }
        "group" => em.log(ui, LogKind::Group, value.to_string()),
        "endgroup" | "debug" | "save-state" | "stop-commands" | "echo" => {}
        "error" => em.log(ui, LogKind::Error, value.to_string()),
        "warning" => em.log(ui, LogKind::Warning, value.to_string()),
        "notice" => em.log(ui, LogKind::Notice, value.to_string()),
        _ => em.log(ui, LogKind::Output, line.clone()),
    }
}

/// Parses `$ATK_ENV` / `$ATK_OUTPUT` files: `KEY=value` and `KEY<<DELIM ... DELIM` blocks.
fn parse_file_commands(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let heredoc = line
            .find("<<")
            .filter(|&i| line.find('=').is_none_or(|e| i < e));
        if let Some(i) = heredoc {
            let key = line[..i].trim().to_string();
            let delim = line[i + 2..].trim();
            let mut value = Vec::new();
            for l in lines.by_ref() {
                if l == delim {
                    break;
                }
                value.push(l);
            }
            out.push((key, value.join("\n")));
        } else if let Some((k, v)) = line.split_once('=') {
            out.push((k.trim().to_string(), v.to_string()));
        }
    }
    out
}

fn script_ext(shell: &str) -> &'static str {
    match shell {
        "pwsh" | "powershell" => ".ps1",
        "cmd" => ".cmd",
        "python" => ".py",
        _ => ".sh",
    }
}

fn wrap_script(shell: &str, script: &str) -> String {
    match shell {
        "pwsh" | "powershell" => format!(
            "$ErrorActionPreference = 'stop'\n{script}\nif ((Test-Path -LiteralPath variable:\\LASTEXITCODE)) {{ exit $LASTEXITCODE }}\n"
        ),
        "cmd" => format!("@echo off\r\n{}\r\n", script.replace('\n', "\r\n")),
        _ => script.to_string(),
    }
}

/// The command line GitHub uses for each built-in shell; custom shells use `{0}` for the file.
fn shell_argv(shell: &str, file: &str) -> Vec<String> {
    let v = |parts: &[&str]| parts.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match shell {
        "bash" => v(&["bash", "--noprofile", "--norc", "-eo", "pipefail", file]),
        "sh" => v(&["sh", "-e", file]),
        "pwsh" => v(&["pwsh", "-command", &format!(". '{file}'")]),
        "powershell" => v(&["powershell", "-command", &format!(". '{file}'")]),
        "python" => v(&["python", file]),
        "cmd" => v(&[
            "cmd",
            "/D",
            "/E:ON",
            "/V:OFF",
            "/S",
            "/C",
            &format!("CALL \"{file}\""),
        ]),
        custom => {
            let mut parts: Vec<String> = custom
                .split_whitespace()
                .map(|p| p.replace("{0}", file))
                .collect();
            if !custom.contains("{0}") {
                parts.push(file.to_string());
            }
            parts
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_commands() {
        let parsed = parse_file_commands("A=1\nMULTI<<EOF\nline1\nline2\nEOF\nB=x=y\n");
        assert_eq!(
            parsed,
            vec![
                ("A".into(), "1".into()),
                ("MULTI".into(), "line1\nline2".into()),
                ("B".into(), "x=y".into())
            ]
        );
    }

    #[test]
    fn shells() {
        assert_eq!(shell_argv("bash", "/s.sh")[0], "bash");
        assert_eq!(
            shell_argv("node {0} --flag", "/s.js"),
            vec!["node", "/s.js", "--flag"]
        );
        assert_eq!(shell_argv("perl", "/s"), vec!["perl", "/s"]);
    }

    #[test]
    fn cache_names() {
        let key = "cargo-Linux-1a2b/x y";
        let name = cache_file_name(key);
        assert_eq!(name, "cargo-Linux-1a2b%2Fx%20y");
        assert!(name.starts_with(&cache_file_name("cargo-Linux-")));
        assert_eq!(decode_cache_name(&name), key);
    }

    #[test]
    fn artifact_names_and_patterns() {
        assert!(check_artifact_name("coverage-linux").is_ok());
        for bad in ["a/b", "..", "x:y", "q?", "crash.partial"] {
            assert!(check_artifact_name(bad).is_err(), "{bad}");
        }
        assert!(wildcard("atk-*", "atk-linux-x86_64"));
        assert!(wildcard("r?port", "report"));
        assert!(!wildcard("atk-*", "other"));
        assert!(wildcard("*", ""));
    }

    #[test]
    fn docker_args() {
        assert_eq!(
            split_args(r#"-c "echo a  b" '' x"#),
            vec!["-c", "echo a  b", "", "x"]
        );
        assert!(split_args("  ").is_empty());
    }
}
