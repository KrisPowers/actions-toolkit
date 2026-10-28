//! `atk serve`: ATK's tools over HTTP, so a dashboard such as truebrew can choose
//! projects, list their workflows, start runs and follow them. The same tools as
//! `atk mcp` and `atk call`.
//!
//! A project is either a folder under `--browse` (the host's folders, usually
//! mounted read-only) or a Git repository cloned into `--projects`. The chosen ones
//! are kept in `projects.json` in ATK's data folder; tools name a project by its
//! name, never by a path.
//!
//! Every route but `/health` needs `Authorization: Bearer <key>`, the key being the
//! contents of `--key-file`. The file is read on each request, so it may appear after
//! startup; without one those routes refuse everything.
//!
//! - `GET /api/projects`: the chosen projects
//! - `POST /api/projects`: adds one, `{"path": "<folder under --browse>"}` or
//!   `{"repo": "https://...", "branch": "...", "token": "..."}`; a token is used for
//!   the clone and never kept
//! - `POST /api/projects/pull` `{"name", "token"?}`: updates a cloned repository
//! - `POST /api/projects/remove` `{"name"}`: forgets a project, deleting its clone
//! - `GET /api/browse?path=<folder>`: the folders in a folder under `--browse`
//! - `GET /api/tools`: the tool definitions (as `atk tools`)
//! - `POST /api/call/<tool>`: runs one tool with a JSON object of arguments

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::mcp;

/// Largest request body read.
const MAX_BODY: u64 = 1 << 20;

struct Ctx {
    /// Folders that can be chosen as projects, if any.
    browse: Option<PathBuf>,
    /// Where repositories are cloned.
    repos: PathBuf,
    key_file: Option<PathBuf>,
    /// The chosen projects.
    registry: PathBuf,
    /// One change to the registry at a time.
    lock: Mutex<()>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Project {
    name: String,
    path: PathBuf,
    /// `folder` or `repo`.
    source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
}

/// Serves until the process is stopped. Each request gets its own thread, since a
/// tool such as `wait_for_run` can block for minutes.
pub fn run(
    listen: &str,
    repos: &Path,
    browse: Option<PathBuf>,
    key_file: Option<PathBuf>,
) -> Result<()> {
    std::fs::create_dir_all(repos)
        .with_context(|| format!("cannot create {}", repos.display()))?;
    let repos = crate::util::canonical(repos)?;
    let browse = match browse {
        Some(b) if b.is_dir() => Some(crate::util::canonical(&b)?),
        Some(b) => {
            eprintln!("warning: {} is not a folder, so none can be browsed", b.display());
            None
        }
        None => None,
    };
    let registry = dirs::data_dir()
        .map(|d| d.join("atk").join("projects.json"))
        .unwrap_or_else(|| repos.join(".projects.json"));
    if key_file.is_none() {
        eprintln!("warning: no --key-file, so the API refuses every request");
    }
    let server = Server::http(listen).map_err(|e| anyhow!("cannot listen on {listen}: {e}"))?;
    eprintln!("atk serving on {listen}");
    reap_children();
    let ctx = Arc::new(Ctx {
        browse,
        repos,
        key_file,
        registry,
        lock: Mutex::new(()),
    });
    for req in server.incoming_requests() {
        let ctx = ctx.clone();
        thread::spawn(move || handle(&ctx, req));
    }
    Ok(())
}

fn handle(ctx: &Ctx, mut req: Request) {
    let full = req.url().to_string();
    let (url, query) = full.split_once('?').unwrap_or((&full, ""));
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
            .and_then(|_| route(ctx, &method, url, query, &text))
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

fn object(body: &str) -> Result<serde_json::Map<String, Value>> {
    if body.trim().is_empty() {
        return Ok(serde_json::Map::new());
    }
    match serde_json::from_str(body).context("the body must be a JSON object")? {
        Value::Object(m) => Ok(m),
        _ => bail!("the body must be a JSON object"),
    }
}

fn string(obj: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn route(ctx: &Ctx, method: &Method, url: &str, query: &str, body: &str) -> Result<Value> {
    match (method, url) {
        (Method::Get, "/api/projects") => Ok(json!({
            "projects": load(ctx).iter().map(view).collect::<Vec<_>>(),
            "browse": ctx.browse.is_some(),
        })),
        (Method::Post, "/api/projects") => {
            let obj = object(body)?;
            let added = match (string(&obj, "path"), string(&obj, "repo")) {
                (Some(path), None) => add_folder(ctx, &path)?,
                (None, Some(repo)) => add_repo(
                    ctx,
                    &repo,
                    string(&obj, "branch"),
                    string(&obj, "token"),
                )?,
                _ => bail!("give either a `path` or a `repo`"),
            };
            Ok(view(&added))
        }
        (Method::Post, "/api/projects/pull") => {
            let obj = object(body)?;
            let name = string(&obj, "name").context("`name` is required")?;
            pull(ctx, &name, string(&obj, "token"))?;
            Ok(json!({ "ok": true }))
        }
        (Method::Post, "/api/projects/remove") => {
            let obj = object(body)?;
            remove(ctx, &string(&obj, "name").context("`name` is required")?)?;
            Ok(json!({ "ok": true }))
        }
        (Method::Get, "/api/browse") => browse(ctx, &query_param(query, "path")),
        (Method::Get, "/api/tools") => Ok(mcp::export(mcp::Format::Mcp)),
        (Method::Post, path) if path.starts_with("/api/call/") => {
            let tool = &path["/api/call/".len()..];
            let mut args = object(body)?;
            let root = match string(&args, "project") {
                Some(name) => project_path(ctx, &name)?,
                None => bail!("`project` is required"),
            };
            args.insert("project".into(), json!(root));
            mcp::call(&root, tool, &Value::Object(args))
        }
        _ => bail!("no route {url}"),
    }
}

// ---- projects ----

fn load(ctx: &Ctx) -> Vec<Project> {
    std::fs::read(&ctx.registry)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(ctx: &Ctx, projects: &[Project]) -> Result<()> {
    if let Some(dir) = ctx.registry.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = ctx.registry.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(projects)?)?;
    std::fs::rename(&tmp, &ctx.registry)?;
    Ok(())
}

/// A project as the dashboard lists it: where it comes from and whether it has
/// workflows.
fn view(p: &Project) -> Value {
    json!({
        "name": p.name,
        "source": p.source,
        "repo": p.repo,
        "branch": p.branch,
        "path": p.path,
        "exists": p.path.is_dir(),
        "workflows": p.path.join(".github").join("workflows").is_dir(),
    })
}

fn project_path(ctx: &Ctx, name: &str) -> Result<PathBuf> {
    let p = load(ctx)
        .into_iter()
        .find(|p| p.name == name)
        .with_context(|| format!("no project {name}"))?;
    if !p.path.is_dir() {
        bail!("{} is gone from {}", name, p.path.display());
    }
    Ok(p.path)
}

/// A name for a new project: `base`, cleaned, made unique among `taken`.
fn unique_name(base: &str, taken: &[Project], dir: Option<&Path>) -> String {
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let clean = clean.trim_matches(|c| c == '-' || c == '.').to_string();
    let clean = if clean.is_empty() {
        "project".to_string()
    } else {
        clean
    };
    let free = |n: &str| {
        !taken.iter().any(|p| p.name == n) && dir.is_none_or(|d| !d.join(n).exists())
    };
    let mut name = clean.clone();
    let mut i = 2;
    while !free(&name) {
        name = format!("{clean}-{i}");
        i += 1;
    }
    name
}

/// Resolves a folder below the browse folder, refusing anything outside it.
fn browse_path(ctx: &Ctx, rel: &str) -> Result<PathBuf> {
    let root = ctx.browse.as_ref().context("no folders are shared with ATK")?;
    let rel = Path::new(rel.trim_matches('/'));
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        bail!("not a folder under the shared folders");
    }
    let path = crate::util::canonical(&root.join(rel)).context("no such folder")?;
    if !path.starts_with(root) || !path.is_dir() {
        bail!("not a folder under the shared folders");
    }
    Ok(path)
}

fn add_folder(ctx: &Ctx, rel: &str) -> Result<Project> {
    let path = browse_path(ctx, rel)?;
    let _guard = ctx.lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut projects = load(ctx);
    if let Some(p) = projects.iter().find(|p| p.path == path) {
        return Ok(p.clone());
    }
    let base = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let p = Project {
        name: unique_name(&base, &projects, None),
        path,
        source: "folder".into(),
        repo: None,
        branch: None,
    };
    projects.push(p.clone());
    save(ctx, &projects)?;
    Ok(p)
}

/// The URL git is given: `repo` with the token, when there is one, as the password.
fn auth_url(repo: &str, token: Option<&str>) -> String {
    match token {
        Some(t) => repo.replacen("https://", &format!("https://x-access-token:{t}@"), 1),
        None => repo.to_string(),
    }
}

/// Runs git, failing with its own message, the token blanked out.
fn git(args: &[&str], token: Option<&str>) -> Result<()> {
    let out = Command::new("git")
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("git is not installed")?;
    if !out.status.success() {
        let mut msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if let Some(t) = token {
            msg = msg.replace(t, "***");
        }
        bail!("git failed: {msg}");
    }
    Ok(())
}

fn add_repo(
    ctx: &Ctx,
    repo: &str,
    branch: Option<String>,
    token: Option<String>,
) -> Result<Project> {
    if !repo.starts_with("https://") {
        bail!("give the repository's https:// address");
    }
    let base = repo
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    let name = {
        let _guard = ctx.lock.lock().unwrap_or_else(|e| e.into_inner());
        unique_name(&base, &load(ctx), Some(&ctx.repos))
    };
    let dest = ctx.repos.join(&name);
    let url = auth_url(repo, token.as_deref());
    let dest_str = dest.to_string_lossy().into_owned();
    let mut args = vec!["clone", "--depth", "1"];
    if let Some(b) = &branch {
        args.extend(["--branch", b.as_str()]);
    }
    args.extend([url.as_str(), dest_str.as_str()]);
    if let Err(e) = git(&args, token.as_deref()) {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(e);
    }
    // The token isn't kept: the clone remembers the plain address.
    git(&["-C", &dest_str, "remote", "set-url", "origin", repo], None)?;
    let p = Project {
        name,
        path: dest,
        source: "repo".into(),
        repo: Some(repo.to_string()),
        branch,
    };
    let _guard = ctx.lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut projects = load(ctx);
    projects.push(p.clone());
    save(ctx, &projects)?;
    Ok(p)
}

/// Brings a cloned repository up to date with its remote.
fn pull(ctx: &Ctx, name: &str, token: Option<String>) -> Result<()> {
    let p = load(ctx)
        .into_iter()
        .find(|p| p.name == name)
        .with_context(|| format!("no project {name}"))?;
    let Some(repo) = p.repo.as_deref() else {
        bail!("{name} is a folder; it's always current");
    };
    let dir = p.path.to_string_lossy().into_owned();
    let url = auth_url(repo, token.as_deref());
    let refspec = p.branch.clone().unwrap_or_else(|| "HEAD".into());
    git(&["-C", &dir, "fetch", "--depth", "1", &url, &refspec], token.as_deref())?;
    git(&["-C", &dir, "reset", "--hard", "FETCH_HEAD"], None)
}

fn remove(ctx: &Ctx, name: &str) -> Result<()> {
    let _guard = ctx.lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut projects = load(ctx);
    let i = projects
        .iter()
        .position(|p| p.name == name)
        .with_context(|| format!("no project {name}"))?;
    let p = projects.remove(i);
    // Only a clone ATK made is deleted; a shared folder is left alone.
    if p.source == "repo" && p.path.starts_with(&ctx.repos) && p.path != ctx.repos {
        let _ = std::fs::remove_dir_all(&p.path);
    }
    save(ctx, &projects)
}

/// The folders in one folder under the browse folder, each with whether it has
/// workflows and whether it's a Git repository.
fn browse(ctx: &Ctx, rel: &str) -> Result<Value> {
    let dir = browse_path(ctx, rel)?;
    let root = ctx.browse.as_ref().context("no folders are shared with ATK")?;
    let rel_of = |p: &Path| {
        p.strip_prefix(root)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    };
    let mut folders: Vec<Value> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| {
            let p = e.path();
            json!({
                "name": e.file_name().to_string_lossy(),
                "path": rel_of(&p),
                "workflows": p.join(".github").join("workflows").is_dir(),
                "git": p.join(".git").exists(),
            })
        })
        .collect();
    folders.sort_by_key(|f| f["name"].as_str().unwrap_or_default().to_lowercase());
    let here = rel_of(&dir);
    let parent = (dir != *root).then(|| dir.parent().map(rel_of).unwrap_or_default());
    Ok(json!({ "path": here, "parent": parent, "folders": folders }))
}

/// One parameter of a query string, percent-decoded.
fn query_param(query: &str, key: &str) -> String {
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| decode(v))
        .unwrap_or_default()
}

fn decode(s: &str) -> String {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let pair = (bytes[i] == b'%' && i + 2 < bytes.len())
            .then(|| Some(hex(bytes[i + 1])? * 16 + hex(bytes[i + 2])?))
            .flatten();
        match (bytes[i], pair) {
            (_, Some(b)) => {
                out.push(b);
                i += 2;
            }
            (b'+', None) => out.push(b' '),
            (b, None) => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_query_values() {
        assert_eq!(query_param("path=OneDrive%2FDocuments+x&y=1", "path"), "OneDrive/Documents x");
        assert_eq!(query_param("y=1", "path"), "");
        assert_eq!(decode("100%"), "100%");
    }

    #[test]
    fn names_are_clean_and_unique() {
        let taken = vec![Project {
            name: "app".into(),
            path: PathBuf::from("/x"),
            source: "folder".into(),
            repo: None,
            branch: None,
        }];
        assert_eq!(unique_name("app", &taken, None), "app-2");
        assert_eq!(unique_name("my app!", &taken, None), "my-app");
        assert_eq!(unique_name("..", &taken, None), "project");
    }

    #[test]
    fn token_goes_in_the_url_only() {
        assert_eq!(
            auth_url("https://github.com/a/b.git", Some("t0k")),
            "https://x-access-token:t0k@github.com/a/b.git"
        );
        assert_eq!(auth_url("https://github.com/a/b.git", None), "https://github.com/a/b.git");
    }
}
