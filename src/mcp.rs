//! ATK for AI agents. One set of tools (run a workflow, watch it, review it) reachable three
//! ways: `atk mcp` serves them over the Model Context Protocol on stdio, for MCP clients
//! such as Claude Code, Claude Desktop, the OpenAI Agents SDK or an Ollama front end;
//! `atk tools --format openai|anthropic` prints them as function definitions for a custom
//! agent loop; and `atk call <tool> '<json>'` executes one, so that loop only needs a shell.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use serde_json::{json, Map, Value};

use crate::engine;
use crate::headless::{self, RunRequest};
use crate::history::{RunRecord, Store};
use crate::report;
use crate::workflow;

/// Longest a single tool call may block waiting for a run.
const MAX_WAIT_SECS: u64 = 900;
/// Log lines returned per `get_logs` call unless asked otherwise.
const LOG_PAGE: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// MCP `tools/list` entries
    Mcp,
    /// OpenAI (and Ollama) function-calling tools
    Openai,
    /// Anthropic Messages API tools
    Anthropic,
}

struct Tool {
    name: &'static str,
    description: &'static str,
    /// Properties beyond the `project` every tool accepts.
    properties: Value,
    required: &'static [&'static str],
}

fn tools() -> Vec<Tool> {
    let id = json!({ "type": "string", "description": "Run id, a unique start of one, or `last` for the project's newest run" });
    let wait = json!({ "type": "integer", "minimum": 0, "maximum": MAX_WAIT_SECS, "description": "Seconds to wait for the run to end before returning (0 returns right away)" });
    vec![
        Tool {
            name: "list_workflows",
            description: "List the project's workflows: file, name, triggers, and each job's id, runs-on, needs, step count and matrix size.",
            properties: json!({}),
            required: &[],
        },
        Tool {
            name: "validate_workflows",
            description: "Parse and plan workflow files without running them, reporting any error. Use after editing a workflow.",
            properties: json!({
                "workflow": { "type": "string", "description": "Workflow path, file name or name; omit to check all" },
            }),
            required: &[],
        },
        Tool {
            name: "run_workflow",
            description: "Start a workflow, or one job of it, in Docker in the background. Returns the run id at once (or after wait_seconds). Follow it with wait_for_run, get_run and get_logs.",
            properties: json!({
                "workflow": { "type": "string", "description": "Workflow path, file name or name (see list_workflows)" },
                "job": { "type": "string", "description": "Run only this job id, treating its needs as done" },
                "event": { "type": "string", "description": "Event to simulate: push, pull_request, workflow_dispatch, schedule, release" },
                "inputs": { "type": "object", "additionalProperties": { "type": "string" }, "description": "workflow_dispatch inputs" },
                "wait_seconds": wait,
            }),
            required: &["workflow"],
        },
        Tool {
            name: "wait_for_run",
            description: "Block until a run ends or the timeout passes, then return its review (see get_run) with done=true/false. Call again while done is false.",
            properties: json!({
                "id": id,
                "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": MAX_WAIT_SECS, "description": "Default 120" },
            }),
            required: &["id"],
        },
        Tool {
            name: "get_run",
            description: "A run's review: outcome (running, success, failure, cancelled, interrupted), every job and step with status and duration, and for each failed step its error lines and last output lines.",
            properties: json!({ "id": id }),
            required: &["id"],
        },
        Tool {
            name: "get_logs",
            description: "A page of a run's log lines, optionally for one job and step. Pass next_offset back as offset to read on, also while the run is still going.",
            properties: json!({
                "id": id,
                "job": { "type": "string", "description": "Job number, name, or start of its name (all matrix variants)" },
                "step": { "type": "string", "description": "Step number or name" },
                "offset": { "type": "integer", "minimum": 0, "description": "Lines to skip, from a previous next_offset" },
                "limit": { "type": "integer", "minimum": 1, "description": "Lines to return, default 200" },
            }),
            required: &["id"],
        },
        Tool {
            name: "list_runs",
            description: "The project's runs, newest first, including ones still going.",
            properties: json!({ "limit": { "type": "integer", "minimum": 1, "description": "Default 20" } }),
            required: &[],
        },
        Tool {
            name: "cancel_run",
            description: "Ask a running run to stop. Its running steps are killed and its containers removed.",
            properties: json!({ "id": id }),
            required: &["id"],
        },
        Tool {
            name: "rerun",
            description: "Start a past run's workflow (and job, and event) again in the background; returns the new run id.",
            properties: json!({ "id": id, "wait_seconds": wait }),
            required: &["id"],
        },
        Tool {
            name: "docker_status",
            description: "Whether Docker is installed and running, which every run needs.",
            properties: json!({}),
            required: &[],
        },
    ]
}

fn schema(tool: &Tool) -> Value {
    let mut props = tool.properties.as_object().cloned().unwrap_or_default();
    props.insert(
        "project".into(),
        json!({ "type": "string", "description": "Project folder; defaults to the one ATK was started in" }),
    );
    json!({ "type": "object", "properties": props, "required": tool.required })
}

/// The tool definitions in `format`.
pub fn export(format: Format) -> Value {
    tools()
        .iter()
        .map(|t| match format {
            Format::Mcp => {
                json!({ "name": t.name, "description": t.description, "inputSchema": schema(t) })
            }
            Format::Openai => json!({
                "type": "function",
                "function": { "name": t.name, "description": t.description, "parameters": schema(t) },
            }),
            Format::Anthropic => {
                json!({ "name": t.name, "description": t.description, "input_schema": schema(t) })
            }
        })
        .collect()
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn num_arg(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

fn store() -> Result<Store> {
    Store::open_default().context("this system has no user data folder for the run history")
}

/// Runs tool `name` with JSON `args` for the project at `default_root` (unless `project`
/// is given) and returns its JSON result.
pub fn call(default_root: &Path, name: &str, args: &Value) -> Result<Value> {
    let root = match str_arg(args, "project") {
        Some(p) => workflow::find_root(
            &crate::util::canonical(Path::new(p))
                .with_context(|| format!("{p} does not exist"))?,
        ),
        None => default_root.to_path_buf(),
    };
    let find_run = |store: &Store| -> Result<RunRecord> {
        store.find(&root, str_arg(args, "id").context("`id` is required")?)
    };
    match name {
        "list_workflows" => Ok(json!({ "project": root, "workflows": report::workflows(&root) })),
        "validate_workflows" => {
            let only = str_arg(args, "workflow")
                .map(|w| workflow::find(&root, w))
                .transpose()
                .map_err(anyhow::Error::msg)?;
            let (results, ok) = report::validate(&root, only.as_deref());
            Ok(json!({ "valid": ok, "workflows": results }))
        }
        "run_workflow" => {
            let workflow = str_arg(args, "workflow").context("`workflow` is required")?;
            let inputs = args
                .get("inputs")
                .and_then(Value::as_object)
                .map(|m| m.iter().map(|(k, v)| (k.clone(), plain(v))).collect())
                .unwrap_or_default();
            let req = RunRequest {
                workflow: workflow::find(&root, workflow).map_err(anyhow::Error::msg)?,
                job: str_arg(args, "job").map(String::from),
                event: str_arg(args, "event").map(String::from),
                inputs,
                ..Default::default()
            };
            start(&root, &req, num_arg(args, "wait_seconds").unwrap_or(0))
        }
        "rerun" => {
            let record = find_run(&store()?)?;
            let req = RunRequest {
                workflow: record.workflow,
                job: record.job,
                event: Some(record.event),
                ..Default::default()
            };
            start(&record.project, &req, num_arg(args, "wait_seconds").unwrap_or(0))
        }
        "wait_for_run" => {
            let store = store()?;
            let record = find_run(&store)?;
            let secs = num_arg(args, "timeout_seconds").unwrap_or(120);
            let done = headless::wait(
                &store,
                &record.id,
                Duration::from_secs(secs.min(MAX_WAIT_SECS)),
            );
            let mut v = review(&store, &record.id)?;
            v["done"] = json!(done);
            Ok(v)
        }
        "get_run" => {
            let store = store()?;
            let record = find_run(&store)?;
            Ok(report::review(&store, &record))
        }
        "get_logs" => {
            let store = store()?;
            let record = find_run(&store)?;
            let lines = report::filter_logs(
                &record,
                store.logs(&record.id),
                str_arg(args, "job"),
                str_arg(args, "step"),
            )?;
            let offset = (num_arg(args, "offset").unwrap_or(0) as usize).min(lines.len());
            let limit = num_arg(args, "limit").map_or(LOG_PAGE, |n| n as usize);
            let end = offset.saturating_add(limit).min(lines.len());
            let page: Vec<Value> = lines[offset..end]
                .iter()
                .map(|l| report::log_line(&record, l))
                .collect();
            Ok(json!({
                "id": record.id,
                "outcome": record.outcome.as_str(),
                "lines": page,
                "next_offset": end,
                "total": lines.len(),
                "more": end < lines.len() || !record.outcome.is_done(),
            }))
        }
        "list_runs" => {
            let limit = num_arg(args, "limit").unwrap_or(20) as usize;
            let runs: Vec<Value> = store()?
                .list(&root)
                .iter()
                .take(limit)
                .map(report::run)
                .collect();
            Ok(json!({ "project": root, "runs": runs }))
        }
        "cancel_run" => {
            let store = store()?;
            let record = find_run(&store)?;
            if record.outcome.is_done() {
                return Ok(json!({ "id": record.id, "outcome": record.outcome.as_str(), "cancelled": false }));
            }
            store.request_cancel(&record.id)?;
            Ok(json!({ "id": record.id, "cancelled": true }))
        }
        "docker_status" => {
            let d = engine::detect();
            Ok(json!({ "ready": d.is_ready(), "status": d.status_label(), "version": d.version, "note": d.note }))
        }
        other => bail!("unknown tool `{other}` (see `atk tools`)"),
    }
}

/// A JSON input value as the string workflows see.
fn plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn review(store: &Store, id: &str) -> Result<Value> {
    let record = store.get(id).context("the run was deleted")?;
    Ok(report::review(store, &record))
}

fn start(root: &Path, req: &RunRequest, wait_secs: u64) -> Result<Value> {
    let store = store()?;
    let id = headless::spawn_detached(root, &store, req)?;
    let done = wait_secs > 0
        && headless::wait(
            &store,
            &id,
            Duration::from_secs(wait_secs.min(MAX_WAIT_SECS)),
        );
    let mut v = review(&store, &id)?;
    v["done"] = json!(done);
    Ok(v)
}

const INSTRUCTIONS: &str = "ATK runs this project's CI/CD workflows locally, each job in its own \
Docker container. Typical flow: list_workflows, run_workflow (returns a run id), wait_for_run \
until done, then get_run to review failures and get_logs for detail. Fix the code or workflow, \
validate_workflows, and rerun.";

/// Serves the tools over MCP (newline-delimited JSON-RPC on stdin/stdout) until stdin closes.
pub fn serve(root: PathBuf) -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&root, &msg),
            Err(e) => Some(json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": format!("parse error: {e}") },
            })),
        };
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// The response to one JSON-RPC request, or `None` for a notification or a response.
fn handle(root: &Path, msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(Value::as_str)?;
    let id = msg.get("id").cloned()?;
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2025-06-18"),
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "atk", "title": "Actions Toolkit", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": export(Format::Mcp) })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| Value::Object(Map::new()));
            let (text, is_error) = match call(root, name, &args) {
                Ok(v) => (serde_json::to_string_pretty(&v).unwrap_or_default(), false),
                Err(e) => (format!("{e:#}"), true),
            };
            Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
        }
        other => Err(json!({ "code": -32601, "message": format!("method not found: {other}") })),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_lists_every_tool() {
        let n = tools().len();
        for format in [Format::Mcp, Format::Openai, Format::Anthropic] {
            assert_eq!(export(format).as_array().unwrap().len(), n);
        }
        let openai = export(Format::Openai);
        assert_eq!(openai[0]["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn speaks_json_rpc() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/demo");
        let init = handle(
            &root,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }),
        )
        .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert!(handle(&root, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());

        let call = handle(
            &root,
            &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "list_workflows", "arguments": {} } }),
        )
        .unwrap();
        assert_eq!(call["result"]["isError"], false);
        let text = call["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Demo"), "{text}");

        let unknown = handle(&root, &json!({ "jsonrpc": "2.0", "id": 3, "method": "nope" })).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
    }
}
