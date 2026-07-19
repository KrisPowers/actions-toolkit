use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use octocrab::Octocrab;
use serde::Serialize;

const WORKFLOWS_DIR: &str = ".github/workflows";

/// A workflow file living in a repo's `.github/workflows` directory on GitHub, i.e. one that
/// runs on GitHub's own Actions runners rather than through this app.
#[derive(Debug, Clone, Serialize)]
pub struct GithubWorkflowFile {
    pub name: String,
    pub path: String,
    pub sha: String,
}

/// List the YAML workflow files GitHub Actions would run for this repo. Returns an empty list
/// (rather than an error) when the repo has no `.github/workflows` directory at all.
pub async fn list_workflow_files(client: &Octocrab, owner: &str, repo: &str) -> Result<Vec<GithubWorkflowFile>> {
    let result = client.repos(owner, repo).get_content().path(WORKFLOWS_DIR).send().await;

    let items = match result {
        Ok(content) => content.items,
        Err(octocrab::Error::GitHub { source, .. }) if source.status_code == axum::http::StatusCode::NOT_FOUND => {
            return Ok(Vec::new());
        }
        Err(e) => return Err(e).context("failed to list .github/workflows contents"),
    };

    Ok(items
        .into_iter()
        .filter(|item| item.r#type == "file" && (item.name.ends_with(".yml") || item.name.ends_with(".yaml")))
        .map(|item| GithubWorkflowFile { name: item.name, path: item.path, sha: item.sha })
        .collect())
}

/// Fetch and decode the raw YAML source of a single workflow file at `path`.
pub async fn get_workflow_content(client: &Octocrab, owner: &str, repo: &str, path: &str) -> Result<String> {
    let content = client
        .repos(owner, repo)
        .get_content()
        .path(path)
        .send()
        .await
        .with_context(|| format!("failed to fetch contents of {path}"))?;

    let file = content.items.into_iter().next().context("file not found")?;
    let encoded = file.content.context("file had no content")?;
    let decoded = general_purpose::STANDARD
        .decode(encoded.replace('\n', ""))
        .context("failed to decode file content as base64")?;
    String::from_utf8(decoded).context("file content was not valid UTF-8")
}
