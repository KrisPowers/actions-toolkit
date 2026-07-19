use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Client ID of the shared actions-toolkit GitHub App (https://github.com/settings/apps/actionstoolkit).
/// Public by design: every actions-toolkit install authorizes through this same App via the
/// OAuth authorization-code + PKCE flow, so no per-user registration or client secret is needed.
/// Overridable via `GITHUB_APP_CLIENT_ID` for forks that register their own App.
pub const DEFAULT_GITHUB_APP_CLIENT_ID: &str = "Iv23liCp6juYQps4Dxdu";

/// Slug of the shared actions-toolkit GitHub App, used to build its public install URL
/// (`https://github.com/apps/<slug>/installations/new`) and to match it against a user's
/// installations after a device-flow connect.
pub const GITHUB_APP_SLUG: &str = "actionstoolkit";

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

    /// Client ID of the GitHub App used for the Connect GitHub OAuth flow. Defaults to the
    /// shared actions-toolkit App (see `DEFAULT_GITHUB_APP_CLIENT_ID`); only needs overriding by
    /// forks that register their own App.
    #[arg(long, env = "GITHUB_APP_CLIENT_ID")]
    pub github_app_client_id: Option<String>,
}

/// Resolved, per-process configuration: just where the data lives. Runtime settings that can
/// change without restarting the binary (port, bind address, Docker host override, max
/// concurrent jobs) live in the `settings` table instead, see `db::queries::settings`.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub github_app_client_id: String,
    /// GitHub's OAuth token endpoint (used for both the device-flow poll and refresh grants).
    /// Always `github::oauth::GITHUB_TOKEN_URL` outside of tests; not exposed as a CLI flag/env
    /// var since there's no legitimate reason for a real deployment to point this anywhere else.
    /// Tests construct `AppConfig` directly and override it to a mock server's URI, the same
    /// pattern already used for `github_app_client_id` in fixtures.
    pub github_oauth_token_url: String,
    /// GitHub's device-code endpoint. Same testability rationale as `github_oauth_token_url`.
    pub github_device_code_url: String,
}

impl AppConfig {
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("actions-toolkit.db")
    }

    pub fn workspaces_dir(&self) -> PathBuf {
        self.data_dir.join("workspaces")
    }

    pub fn buckets_dir(&self) -> PathBuf {
        self.data_dir.join("buckets")
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
