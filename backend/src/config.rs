use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "actions-toolkit", about = "Local, self-hosted GitHub Actions-compatible runner")]
pub struct AppConfig {
    /// Port the web server (backend API + frontend) listens on.
    #[arg(long, env = "PORT", default_value_t = 7890)]
    pub port: u16,

    /// Bind address for the HTTP server.
    #[arg(long, env = "BIND_ADDR", default_value = "0.0.0.0")]
    pub bind_addr: String,

    /// Directory used for the SQLite database, workspaces, and artifacts.
    #[arg(long, env = "DATA_DIR", default_value = "data")]
    pub data_dir: PathBuf,

    /// Secret used to sign session JWTs. Generated and persisted on first run if not set.
    #[arg(long, env = "JWT_SECRET")]
    pub jwt_secret: Option<String>,

    /// 32-byte (base64) key used to encrypt PATs/webhook secrets at rest.
    /// Generated and persisted on first run if not set.
    #[arg(long, env = "ENCRYPTION_KEY")]
    pub encryption_key: Option<String>,

    /// Override for the Docker Engine connection (defaults to platform-local socket/pipe).
    #[arg(long, env = "DOCKER_HOST")]
    pub docker_host: Option<String>,

    /// Maximum number of jobs that may run concurrently across all workflow runs.
    #[arg(long, env = "MAX_CONCURRENT_JOBS", default_value_t = 4)]
    pub max_concurrent_jobs: usize,
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
