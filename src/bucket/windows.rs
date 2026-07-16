//! Windows Bucket backend (AppContainer + Job Objects) — scoped as a separate milestone (see
//! the Bucket plan's M2) from the Linux backend built in this pass. Stubbed so the crate
//! compiles and the OS-dispatch shape in `mod.rs` is already correct for when M2 lands.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use sqlx::SqlitePool;

use super::{BucketCapability, BucketHandle, BucketSpec, ExecResult};

pub async fn probe_capability() -> BucketCapability {
    BucketCapability {
        ok: false,
        reason: Some("Bucket's Windows backend (AppContainer + Job Objects) is not implemented yet".to_string()),
    }
}

pub async fn create_job_bucket(_pool: &SqlitePool, _buckets_root: &Path, _spec: BucketSpec<'_>) -> Result<BucketHandle> {
    bail!("Bucket is not yet implemented on Windows")
}

pub async fn exec_step<F>(
    _handle: &BucketHandle,
    _shell_command: &str,
    _working_dir: Option<&str>,
    _env: &[String],
    _on_line: F,
) -> Result<ExecResult>
where
    F: FnMut(&str, String) + Send,
{
    bail!("Bucket is not yet implemented on Windows")
}

pub async fn remove_bucket(_pool: &SqlitePool, _handle: &BucketHandle) -> Result<()> {
    bail!("Bucket is not yet implemented on Windows")
}

pub(crate) fn handle_from_bucket_row(buckets_root: &Path, row: &crate::db::models::Bucket) -> BucketHandle {
    BucketHandle {
        id: row.id.clone(),
        workspace: PathBuf::from(&row.workspace_path),
        root_skeleton: buckets_root.join(&row.id).join("root"),
    }
}
