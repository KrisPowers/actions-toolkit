use std::path::PathBuf;

use anyhow::Result;

pub fn workspace_dir(workspaces_root: &std::path::Path, run_id: &str) -> PathBuf {
    workspaces_root.join(run_id)
}

pub fn ensure(workspaces_root: &std::path::Path, run_id: &str) -> Result<PathBuf> {
    let dir = workspace_dir(workspaces_root, run_id);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn cleanup(workspaces_root: &std::path::Path, run_id: &str) {
    let dir = workspace_dir(workspaces_root, run_id);
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(error = %e, dir = %dir.display(), "failed to clean up workspace dir");
        }
    }
}
