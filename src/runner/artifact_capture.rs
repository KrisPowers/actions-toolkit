use std::path::Path;

use anyhow::Result;
use bollard::Docker;
use sqlx::SqlitePool;

use crate::db::queries::artifacts as artifact_queries;
use crate::workflow::model::ArtifactSpec;

/// For each declared `artifacts:` entry on a job, copy the path out of the job's container
/// into `data/artifacts/<run_id>/<name>/` and record an `artifacts` row.
pub async fn capture(
    docker: &Docker,
    pool: &SqlitePool,
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
        artifact_queries::create(
            pool,
            workflow_run_id,
            Some(job_run_id),
            &spec.name,
            &dest_dir.to_string_lossy(),
            size_bytes as i64,
            None,
        )
        .await?;
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
