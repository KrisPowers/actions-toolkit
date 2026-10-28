//! `atk serve`: ATK's tools over HTTP, so a dashboard such as truebrew can list a
//! project's workflows, start runs and follow them. The same tools as `atk mcp` and
//! `atk call`, limited to the projects in one folder.
//!
//! Every route but `/health` needs `Authorization: Bearer <key>`, the key being the
//! contents of `--key-file`. The file is read on each request, so it may appear after
//! startup; without one those routes refuse everything.
//!
//! - `GET /api/projects`: the project folders under `--projects`
//! - `GET /api/tools`: the tool definitions (as `atk tools`)
//! - `POST /api/call/<tool>`: runs one tool with a JSON object of arguments, whose
//!   `project` is a folder name under `--projects`, never a path

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::mcp;
use crate::workflow;

/// Largest request body read.
const MAX_BODY: u64 = 1 << 20;

struct Ctx {
    projects: PathBuf,
    key_file: Option<PathBuf>,
}

/// Serves until the process is stopped. Each request gets its own thread, since a
/// tool such as `wait_for_run` can block for minutes.
pub fn run(listen: &str, projects: &Path, key_file: Option<PathBuf>) -> Result<()> {
    std::fs::create_dir_all(projects)
        .with_context(|| format!("cannot create {}", projects.display()))?;
    let projects = crate::util::canonical(projects)?;
    if key_file.is_none() {
        eprintln!("warning: no --key-file, so the API refuses every request");
    }
    let server = Server::http(listen).map_err(|e| anyhow!("cannot listen on {listen}: {e}"))?;
    eprintln!("atk serving {} on {listen}", projects.display());
    reap_children();
    let ctx = Arc::new(Ctx { projects, key_file });
    for req in server.incoming_requests() {
        let ctx = ctx.clone();
        thread::spawn(move || handle(&ctx, req));
    }
    Ok(())
}

fn handle(ctx: &Ctx, mut req: Request) {
    let url = req.url().split('?').next().unwrap_or("").to_string();
    let method = req.method().clone();
    let (code, body): (u16, Value) = if method == Method::Get && url == "/health" {
        (200, json!({ "ok": true }))
    } else if !authorized(ctx, &req) {
        (401, json!({ "error": "missing or wrong key" }))
    } else {
        let mut text = String::new();
        let read = req.as_reader().take(MAX_BODY).read_to_string(&mut text);
        match read
            .map_err(anyhow::Error::from)
            .and_then(|_| route(ctx, &method, &url, &text))
        {
            Ok(v) => (200, v),
            Err(e) => (400, json!({ "error": format!("{e:#}") })),
        }
    };
    let mut resp = Response::from_string(body.to_string()).with_status_code(code);
    if let Ok(h) = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]) {
        resp = resp.with_header(h);
    }
    let _ = req.respond(resp);
}

fn authorized(ctx: &Ctx, req: &Request) -> bool {
    let Some(key) = ctx
        .key_file
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
    else {
        return false;
    };
    req.headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .and_then(|h| h.value.as_str().strip_prefix("Bearer "))
        .is_some_and(|given| same(key.as_bytes(), given.as_bytes()))
}

/// Compares in constant time for equal lengths.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn route(ctx: &Ctx, method: &Method, url: &str, body: &str) -> Result<Value> {
    match (method, url) {
        (Method::Get, "/api/projects") => Ok(json!({ "projects": projects(&ctx.projects) })),
        (Method::Get, "/api/tools") => Ok(mcp::export(mcp::Format::Mcp)),
        (Method::Post, path) if path.starts_with("/api/call/") => {
            let tool = &path["/api/call/".len()..];
            let mut args: Value = if body.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(body).context("the body must be a JSON object")?
            };
            let Some(obj) = args.as_object_mut() else {
                bail!("the body must be a JSON object");
            };
            let root = match obj.get("project").and_then(Value::as_str) {
                Some(name) => project(&ctx.projects, name)?,
                None => ctx.projects.clone(),
            };
            obj.insert("project".into(), json!(root));
            mcp::call(&root, tool, &args)
        }
        _ => bail!("no route {url}"),
    }
}

/// The folders under the projects folder, each with whether it has workflows.
fn projects(dir: &Path) -> Vec<Value> {
    let mut out: Vec<Value> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let has = e.path().join(".github").join("workflows").is_dir();
            json!({ "name": name, "workflows": has })
        })
        .collect();
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    out
}

/// Resolves a project folder name under the projects folder, refusing anything that
/// would reach outside it.
fn project(dir: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
        bail!("`project` is a folder name under the projects folder");
    }
    let path =
        crate::util::canonical(&dir.join(name)).with_context(|| format!("no project {name}"))?;
    let root = workflow::find_root(&path);
    if !path.is_dir() || !path.starts_with(dir) || !root.starts_with(dir) {
        bail!("no project {name}");
    }
    Ok(root)
}

/// Background runs are started as child processes and outlive the request that
/// started them; this collects them once they exit.
#[cfg(unix)]
fn reap_children() {
    thread::spawn(|| loop {
        // SAFETY: waitpid with WNOHANG and a null status pointer only collects
        // children that have already exited.
        while unsafe { libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) } > 0 {}
        thread::sleep(std::time::Duration::from_secs(5));
    });
}

#[cfg(not(unix))]
fn reap_children() {}
