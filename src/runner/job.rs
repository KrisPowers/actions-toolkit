//! Executes a single job: provisions a session, runs each step, collects outputs, tears down.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use indexmap::IndexMap;
use serde_json::{json, Map, Value};

use super::{resolve_image, LogKind, RunEvent, RunOptions, Status};
use crate::engine::{Engine, ExecRequest, Session, SessionRequest};
use crate::util::{self, Cancel};
use crate::workflow::action::{self, ActionRef};
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
    em.log(
        0,
        LogKind::System,
        format!("Engine: {}", engine.kind().name()),
    );
    if !pj.runs_on.is_empty() {
        em.log(
            0,
            LogKind::System,
            format!("runs-on: {}", pj.runs_on.join(", ")),
        );
    }
    let data = json!({ "matrix": pj.matrix, "github": { "event_name": cx.opts.event } });
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
        cancel: cx.cancel.clone(),
    };
    if engine.uses_images() {
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
    } else if spec.is_some() {
        em.log(
            0,
            LogKind::Warning,
            format!(
                "`container:` is ignored by the {} engine",
                engine.kind().name()
            ),
        );
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
    /// Workflow + job env, plus anything appended to `$GITHUB_ENV`.
    env: IndexMap<String, String>,
    /// Entries appended to `$GITHUB_PATH`, most recent first.
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
            "server_url": "https://github.com",
            "api_url": "https://api.github.com",
            "graphql_url": "https://api.github.com/graphql",
            "token": opts.secrets.get("GITHUB_TOKEN").cloned().unwrap_or_default(),
            "action_path": "",
        });
        let runner = json!({
            "name": "maverick",
            "os": info.os,
            "arch": info.arch,
            "temp": info.temp,
            "tool_cache": info.tool_cache,
            "environment": "github-hosted",
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
            let value = expr::interpolate_yaml(v, &Ctx::new(&data, self.state()))
                .map_err(|e| anyhow!("env.{k}: {e}"))?;
            self.env.insert(k.clone(), value);
        }
        Ok(())
    }

    fn outputs(&self) -> Map<String, Value> {
        let data = self.ctx_data(&Scope::default(), &IndexMap::new());
        let ctx = Ctx::new(&data, self.state());
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
        let ctx = Ctx::new(&data, self.state());
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
            let ctx = Ctx::new(&data, self.state());
            for (k, v) in &step.env {
                step_env.insert(
                    k.clone(),
                    expr::interpolate_yaml(v, &ctx).map_err(|e| anyhow!("env.{k}: {e}"))?,
                );
            }
        }
        let data = self.ctx_data(scope, &step_env);
        let ctx = Ctx::new(&data, self.state());
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
        self.exec_with_files(ui, shell_argv(shell, &path), env, cwd, timeout)
    }

    /// Runs a command with the GitHub runner file protocol (`$GITHUB_ENV`, `$GITHUB_OUTPUT`, ...).
    fn exec_with_files(
        &mut self,
        ui: usize,
        argv: Vec<String>,
        extra_env: IndexMap<String, String>,
        cwd: String,
        timeout: Option<Duration>,
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
        // File names are unique per command, so creating the directory is enough: `>>` creates
        // the rest, and missing files read back as empty. (One write keeps VM round-trips low.)
        self.session.write_file(&files.env, b"")?;

        let mut env: IndexMap<String, String> = self.base_env(&files).into_iter().collect();
        env.extend(self.env.clone());
        env.extend(extra_env);
        if !self.paths.is_empty() {
            let base = env
                .get("PATH")
                .cloned()
                .unwrap_or_else(|| info.path.clone());
            let mut parts = self.paths.clone();
            parts.push(base);
            env.insert("PATH".into(), parts.join(info.path_separator()));
        }

        let req = ExecRequest {
            argv,
            env: env.into_iter().collect(),
            cwd,
            timeout,
        };
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
        [
            ("CI", "true".to_string()),
            ("GITHUB_ACTIONS", "true".into()),
            ("MAVERICK", "true".into()),
            ("GITHUB_WORKSPACE", info.workspace.clone()),
            ("GITHUB_REPOSITORY", g.repository.clone()),
            ("GITHUB_REPOSITORY_OWNER", g.owner.clone()),
            ("GITHUB_SHA", g.sha.clone()),
            ("GITHUB_REF", g.git_ref()),
            ("GITHUB_REF_NAME", g.branch.clone()),
            ("GITHUB_REF_TYPE", "branch".into()),
            ("GITHUB_EVENT_NAME", opts.event.clone()),
            ("GITHUB_EVENT_PATH", self.event_path.clone()),
            ("GITHUB_JOB", self.pj.key.clone()),
            ("GITHUB_WORKFLOW", self.pj.workflow_name.clone()),
            ("GITHUB_RUN_ID", opts.run_id.clone()),
            ("GITHUB_RUN_NUMBER", "1".into()),
            ("GITHUB_RUN_ATTEMPT", "1".into()),
            ("GITHUB_ACTOR", g.actor.clone()),
            ("GITHUB_SERVER_URL", "https://github.com".into()),
            ("GITHUB_API_URL", "https://api.github.com".into()),
            (
                "GITHUB_GRAPHQL_URL",
                "https://api.github.com/graphql".into(),
            ),
            ("GITHUB_ENV", files.env.clone()),
            ("GITHUB_OUTPUT", files.output.clone()),
            ("GITHUB_PATH", files.path.clone()),
            ("GITHUB_STEP_SUMMARY", files.summary.clone()),
            ("GITHUB_STATE", files.state.clone()),
            ("RUNNER_OS", info.os.to_string()),
            ("RUNNER_ARCH", info.arch.clone()),
            ("RUNNER_TEMP", info.temp.clone()),
            ("RUNNER_TOOL_CACHE", info.tool_cache.clone()),
            ("RUNNER_NAME", "maverick".into()),
            ("RUNNER_ENVIRONMENT", "github-hosted".into()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
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
                    "Workspace is already provisioned by maverick; nothing to check out",
                );
                return Ok((Status::Success, Map::new()));
            }
            ActionRef::Unsupported(why) => {
                self.em
                    .log(ui, LogKind::Warning, format!("Skipping {uses}: {why}"));
                return Ok((Status::Skipped, Map::new()));
            }
            ActionRef::Docker(image) => {
                self.em.log(
                    ui,
                    LogKind::Warning,
                    format!("Skipping docker://{image}: Docker step actions are not supported yet"),
                );
                return Ok((Status::Skipped, Map::new()));
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
                let host_root = {
                    let em = &self.em;
                    action::fetch(&owner, &repo, &git_ref, self.cx.opts.pull, &mut |l| {
                        em.log(ui, LogKind::System, l)
                    })?
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
        let inputs = action_inputs(&def, step, ctx)?;
        let using = def.runs.using.to_ascii_lowercase();
        if using == "composite" {
            return self.run_composite(ui, &def, inputs, env_dir, env);
        }
        if using.starts_with("node") {
            let main = def
                .runs
                .main
                .clone()
                .context("node action has no `runs.main`")?;
            let mut env = env;
            for (k, v) in &inputs {
                env.insert(
                    format!("INPUT_{}", k.to_uppercase().replace(' ', "_")),
                    expr::to_str(v),
                );
            }
            env.insert("GITHUB_ACTION_PATH".into(), env_dir.clone());
            env.insert(
                "GITHUB_ACTION".into(),
                step.id.clone().unwrap_or_else(|| format!("__step{ui}")),
            );
            let argv = vec!["node".to_string(), info.join(&env_dir, &main)];
            return self.exec_with_files(ui, argv, env, info.workspace.clone(), timeout);
        }
        self.em.log(
            ui,
            LogKind::Warning,
            format!(
                "Skipping {uses}: `{}` actions are not supported yet",
                def.runs.using
            ),
        );
        Ok((Status::Skipped, Map::new()))
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
        let ctx = Ctx::new(&data, self.state());
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
