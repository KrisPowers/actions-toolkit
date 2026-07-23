use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use bollard::Docker;

use crate::runner::run_client::RunClient;
use crate::workflow::model::ArtifactSpec;

/// For each declared `artifacts:` entry on a job, copy the path out of the job's container
/// into `data/artifacts/<run_id>/<name>/` and record an `artifacts` row.
pub async fn capture(
    docker: &Docker,
    run_client: &Arc<dyn RunClient>,
    container_id: &str,
    artifacts_root: &Path,
    workflow_run_id: &str,
    job_run_id: &str,
    specs: &[ArtifactSpec],
) -> Result<()> {
    for spec in specs {
        let dest_dir = artifacts_root.join(workflow_run_id).join(&spec.name);
        if let Err(e) =
            super::docker::download_path(docker, container_id, &spec.path, &dest_dir).await
        {
            tracing::warn!(error = %e, artifact = %spec.name, "failed to capture artifact");
            continue;
        }

        let size_bytes = dir_size(&dest_dir).unwrap_or(0);
        run_client.record_artifact(workflow_run_id, Some(job_run_id), &spec.name, &dest_dir.to_string_lossy(), size_bytes as i64).await?;
    }
    Ok(())
}

/// Same as `capture`, but for jobs running via the Sandbox backend rather than Docker: the
/// workspace is already a real host directory (no container to `download_path` out of), so this
/// is a plain recursive copy instead of a tar-stream download. `spec.path` is authored the same
/// way either backend expects it (e.g. `/workspace/dist`, matching the Docker container's own
/// `/workspace` mount point), so it's resolved relative to `workspace_dir` here the same way.
pub async fn capture_from_workspace(
    run_client: &Arc<dyn RunClient>,
    workspace_dir: &Path,
    artifacts_root: &Path,
    workflow_run_id: &str,
    job_run_id: &str,
    specs: &[ArtifactSpec],
) -> Result<()> {
    for spec in specs {
        let src = resolve_workspace_relative_path(workspace_dir, &spec.path);
        let dest_dir = artifacts_root.join(workflow_run_id).join(&spec.name);
        if let Err(e) = copy_recursive(&src, &dest_dir) {
            tracing::warn!(error = %e, artifact = %spec.name, "failed to capture artifact");
            continue;
        }

        let size_bytes = dir_size(&dest_dir).unwrap_or(0);
        run_client.record_artifact(workflow_run_id, Some(job_run_id), &spec.name, &dest_dir.to_string_lossy(), size_bytes as i64).await?;
    }
    Ok(())
}

/// Strips a leading `/workspace` (the same in-sandbox mount point both backends use) or a bare
/// leading `/` from an artifact path, then resolves it relative to the real host workspace dir.
fn resolve_workspace_relative_path(workspace_dir: &Path, spec_path: &str) -> std::path::PathBuf {
    let relative = spec_path.strip_prefix("/workspace").unwrap_or(spec_path).trim_start_matches('/');
    workspace_dir.join(relative)
}

fn copy_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dest)?;
    }
    Ok(())
}

fn dir_size(dir: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    if dir.is_file() {
        return Ok(std::fs::metadata(dir)?.len());
    }
    for entry in walk(dir)? {
        let meta = std::fs::metadata(&entry)?;
        if meta.is_file() {
            total += meta.len();
        }
    }
    Ok(total)
}

fn walk(dir: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path)?);
        } else {
            out.push(path);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_workspace_relative_path_strips_workspace_prefix() {
        let workspace = Path::new("/data/workspaces/run-1");
        assert_eq!(resolve_workspace_relative_path(workspace, "/workspace/dist"), workspace.join("dist"));
        assert_eq!(resolve_workspace_relative_path(workspace, "/workspace"), workspace.join(""));
    }

    #[test]
    fn resolve_workspace_relative_path_strips_bare_leading_slash() {
        let workspace = Path::new("/data/workspaces/run-1");
        assert_eq!(resolve_workspace_relative_path(workspace, "/dist"), workspace.join("dist"));
    }

    #[test]
    fn resolve_workspace_relative_path_leaves_relative_paths_alone() {
        let workspace = Path::new("/data/workspaces/run-1");
        assert_eq!(resolve_workspace_relative_path(workspace, "dist"), workspace.join("dist"));
    }
}
