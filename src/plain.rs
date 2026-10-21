//! Line-oriented output for non-interactive use (CI, pipes, `--plain`).

use std::io::{IsTerminal, Write};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;

use crate::engine::Engine;
use crate::runner::{self, LogKind, RunEvent, RunOptions, Status};
use crate::util::{fmt_duration, Cancel};
use crate::workflow::plan::RunPlan;

struct Paint(bool);

impl Paint {
    fn wrap(&self, code: &str, s: &str) -> String {
        if self.0 {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
}

pub fn run(plan: Arc<RunPlan>, engine: Arc<dyn Engine>, opts: Arc<RunOptions>) -> Result<bool> {
    let cancel = Cancel::new();
    {
        let cancel = cancel.clone();
        // Children run in their own process groups, so Ctrl-C must be forwarded explicitly.
        let _ = ctrlc::set_handler(move || {
            eprintln!("\nCancelling…");
            cancel.cancel();
        });
    }
    let p = Paint(std::io::stdout().is_terminal());
    let width = plan
        .jobs
        .iter()
        .map(|j| j.label.chars().count())
        .max()
        .unwrap_or(0)
        .min(28);
    println!(
        "{} {} {}",
        p.wrap("1;36", "◆ maverick"),
        p.wrap("1", &plan.title),
        p.wrap(
            "2",
            &format!("· {} · event {}", engine.kind().name(), opts.event)
        )
    );

    let started = Instant::now();
    let mut step_started: Vec<Vec<Option<Instant>>> = plan
        .jobs
        .iter()
        .map(|j| vec![None; j.step_names.len()])
        .collect();
    let mut job_status = vec![Status::Pending; plan.jobs.len()];
    let (rx, handle) = runner::spawn(plan.clone(), engine, opts, cancel);
    let mut out = std::io::stdout().lock();

    for event in rx {
        match event {
            RunEvent::Log {
                job, kind, text, ..
            } => {
                let prefix = p.wrap(
                    "2",
                    &format!("{:<width$} │", truncate(&plan.jobs[job].label, width)),
                );
                let body = match kind {
                    LogKind::Output => text,
                    LogKind::Command => p.wrap("2", &format!("$ {text}")),
                    LogKind::System => p.wrap("2", &text),
                    LogKind::Header | LogKind::Group => p.wrap("1", &format!("▸ {text}")),
                    LogKind::Notice => p.wrap("34", &format!("notice: {text}")),
                    LogKind::Warning => p.wrap("33", &format!("warning: {text}")),
                    LogKind::Error => p.wrap("31", &format!("error: {text}")),
                };
                let _ = writeln!(out, "{prefix} {body}");
            }
            RunEvent::Step { job, step, status } => {
                let name = &plan.jobs[job].step_names[step];
                match status {
                    Status::Running => step_started[job][step] = Some(Instant::now()),
                    Status::Success | Status::Failure | Status::Cancelled
                        if step_started[job][step].is_some() =>
                    {
                        let took = step_started[job][step]
                            .map(|t| fmt_duration(t.elapsed()))
                            .unwrap_or_default();
                        let prefix = p.wrap(
                            "2",
                            &format!("{:<width$} │", truncate(&plan.jobs[job].label, width)),
                        );
                        let _ = writeln!(
                            out,
                            "{prefix} {} {name} {}",
                            icon(&p, status),
                            p.wrap("2", &took)
                        );
                    }
                    _ => {}
                }
            }
            RunEvent::Job { job, status } => job_status[job] = status,
            RunEvent::Finished { .. } => break,
        }
    }
    let ok = handle.join().unwrap_or(false);

    let _ = writeln!(out);
    for (job, status) in plan.jobs.iter().zip(&job_status) {
        let _ = writeln!(out, "  {} {}", icon(&p, *status), job.label);
    }
    let verdict = if ok {
        p.wrap("1;32", "Succeeded")
    } else {
        p.wrap("1;31", "Failed")
    };
    let _ = writeln!(out, "\n{verdict} in {}", fmt_duration(started.elapsed()));
    Ok(ok)
}

fn icon(p: &Paint, status: Status) -> String {
    match status {
        Status::Success => p.wrap("32", "✓"),
        Status::Failure => p.wrap("31", "✗"),
        Status::Cancelled => p.wrap("35", "■"),
        Status::Skipped => p.wrap("2", "⊘"),
        Status::Running => p.wrap("33", "●"),
        Status::Pending => p.wrap("2", "○"),
    }
}
