use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use sqlx::SqlitePool;

use crate::crypto::EncryptionKey;

#[derive(Parser, Debug, Clone)]
#[command(name = "actions-toolkit", about = "Local, self-hosted GitHub Actions-compatible runner")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Create the data directory and initialize the database with default settings, without
    /// starting the server. Safe to run more than once (installers use this so the database
    /// exists before the first `start`); `start` runs the same bootstrap itself, so running
    /// `init` first is a convenience, not a requirement.
    Init(InitArgs),

    /// Start the server (backend API + embedded UI) and keep it running in the foreground.
    #[command(alias = "listen")]
    Start(StartArgs),

    /// Internal: re-exec target used by the Bucket sandbox to perform namespace/mount setup
    /// from a freshly-forked, single-threaded child before exec'ing a step's command. Not
    /// meant to be invoked directly; hidden from `--help`.
    #[command(name = "__bucket-init", hide = true)]
    BucketInit(BucketInitArgs),
}

#[derive(Args, Debug, Clone)]
pub struct BucketInitArgs {
    /// Path to the JSON-serialized `bucket::BucketInitSpec` describing the sandbox to set up
    /// and the command to run inside it.
    pub spec_path: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct InitArgs {
    /// Directory used for the SQLite database, workspaces, and artifacts. Defaults to an
    /// OS-standard per-user data directory if unset.
    #[arg(long, env = "DATA_DIR")]
    pub data_dir: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct StartArgs {
    /// Directory used for the SQLite database, workspaces, and artifacts. Defaults to an
    /// OS-standard per-user data directory if unset.
    #[arg(long, env = "DATA_DIR")]
    pub data_dir: Option<PathBuf>,

    /// Preferred port for the web server (backend API + frontend). Passing this updates the
    /// stored default, so a later plain `start` remembers it. If the resolved port is already
    /// in use, the next free port is tried automatically; check the startup log for the port
    /// actually bound.
    #[arg(long, env = "PORT")]
    pub port: Option<u16>,

    /// Bind address for the HTTP server. Passing this updates the stored default.
    #[arg(long, env = "BIND_ADDR")]
    pub bind_addr: Option<String>,

    /// Secret used to sign session JWTs. Generated and persisted under DATA_DIR/secrets/ on
    /// first run if not set.
    #[arg(long, env = "JWT_SECRET")]
    pub jwt_secret: Option<String>,

    /// 32-byte (base64) key used to encrypt PATs/webhook secrets at rest. Generated and
    /// persisted under DATA_DIR/secrets/ on first run if not set.
    #[arg(long, env = "ENCRYPTION_KEY")]
    pub encryption_key: Option<String>,

    /// Override for the Docker Engine connection (defaults to platform-local socket/pipe).
    /// Passing this updates the stored default.
    #[arg(long, env = "DOCKER_HOST")]
    pub docker_host: Option<String>,

    /// Maximum number of jobs that may run concurrently across all workflow runs. Passing this
    /// updates the stored default.
    #[arg(long, env = "MAX_CONCURRENT_JOBS")]
    pub max_concurrent_jobs: Option<usize>,
}

/// Resolved, per-process configuration: just where the data lives. Runtime settings that can
/// change without restarting the binary (port, bind address, Docker host override, max
/// concurrent jobs) live in the `settings` table instead, see `db::queries::settings`.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub data_dir: PathBuf,
}

impl AppConfig {
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("actions-toolkit.db")
    }

    pub fn workspaces_dir(&self) -> PathBuf {
        self.data_dir.join("workspaces")
    }

    pub fn artifacts_dir(&self) -> PathBuf {
        self.data_dir.join("artifacts")
    }

    pub fn secrets_dir(&self) -> PathBuf {
        self.data_dir.join("secrets")
    }
}

/// Resolve the data directory: an explicit override (flag/env) wins, otherwise an OS-standard,
/// machine-local per-user data directory (e.g. `~/.local/share/actions-toolkit` on Linux,
/// `%LOCALAPPDATA%\actions-toolkit` on Windows). Falls back to a relative `./data` (with a
/// warning) on the rare platform where no home/data directory can be resolved.
pub fn resolve_data_dir(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = explicit {
        return dir;
    }
    if let Some(dir) = dirs::data_local_dir() {
        return dir.join("actions-toolkit");
    }
    tracing::warn!("could not resolve an OS-standard data directory; falling back to ./data");
    PathBuf::from("data")
}

pub struct Bootstrapped {
    pub db: SqlitePool,
    pub app_config: AppConfig,
    pub enc: EncryptionKey,
    pub jwt_secret: String,
}

/// Create the data/workspaces/artifacts directories, open (and migrate) the database, and
/// load-or-generate the JWT signing secret and encryption key. Shared by `init` and `start` so
/// both produce an identical, ready-to-serve data directory.
pub async fn bootstrap(data_dir: PathBuf, jwt_secret: Option<String>, encryption_key: Option<String>) -> Result<Bootstrapped> {
    let app_config = AppConfig { data_dir };
    std::fs::create_dir_all(&app_config.data_dir)?;
    std::fs::create_dir_all(app_config.workspaces_dir())?;
    std::fs::create_dir_all(app_config.artifacts_dir())?;

    let db = crate::db::connect(&app_config.db_path()).await?;
    tracing::info!(path = %app_config.db_path().display(), "database ready");

    let enc = EncryptionKey::load_or_generate(encryption_key.as_deref(), &app_config.secrets_dir())?;

    let jwt_secret = match jwt_secret {
        Some(s) => s,
        None => {
            let path = app_config.secrets_dir().join("jwt.key");
            std::fs::create_dir_all(&app_config.secrets_dir())?;
            if path.exists() {
                std::fs::read_to_string(&path)?.trim().to_string()
            } else {
                use rand::RngCore;
                let mut bytes = [0u8; 32];
                rand::rngs::OsRng.fill_bytes(&mut bytes);
                let secret = hex::encode(bytes);
                std::fs::write(&path, &secret)?;
                secret
            }
        }
    };

    Ok(Bootstrapped { db, app_config, enc, jwt_secret })
}
