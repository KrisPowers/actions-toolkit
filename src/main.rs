mod api;
mod app;
mod auth;
mod config;
mod crypto;
mod db;
mod error;
mod github;
mod runner;
mod telemetry;
mod workflow;
mod ws;

use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use crate::app::{AppState, AppStateInner};
use crate::auth::jwt::JwtCodec;
use crate::config::{Cli, Command};
use crate::db::queries::settings as settings_queries;
use crate::runner::log_stream::LogHub;

/// How many ports past the requested one to try before giving up.
const MAX_PORT_ATTEMPTS: u16 = 20;

/// Binds `bind_addr:preferred_port`, falling back to the next port(s) up if it's already in
/// use. Most users don't have a fixed reason to need exactly port 7890, so a busy default port
/// shouldn't stop the server from starting.
async fn bind_with_fallback(bind_addr: &str, preferred_port: u16) -> anyhow::Result<TcpListener> {
    let mut last_err = None;
    for offset in 0..MAX_PORT_ATTEMPTS {
        let port = preferred_port.saturating_add(offset);
        match TcpListener::bind(format!("{bind_addr}:{port}")).await {
            Ok(listener) => {
                if offset > 0 {
                    tracing::warn!(
                        requested = preferred_port,
                        actual = port,
                        "requested port was already in use, bound to the next available port instead"
                    );
                }
                return Ok(listener);
            }
            Err(e) => {
                tracing::debug!(port, error = %e, "port unavailable, trying next");
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap()).with_context(|| {
        format!(
            "could not bind any port in {preferred_port}..={} on {bind_addr}",
            preferred_port.saturating_add(MAX_PORT_ATTEMPTS - 1)
        )
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init();

    let cli = Cli::parse();

    let args = match cli.command {
        Command::Init(args) => {
            let data_dir = config::resolve_data_dir(args.data_dir);
            let boot = config::bootstrap(data_dir, None, None).await?;
            println!("actions-toolkit initialized at {}", boot.app_config.data_dir.display());
            return Ok(());
        }
        Command::Start(args) => args,
    };

    let data_dir = config::resolve_data_dir(args.data_dir.clone());
    let config::Bootstrapped { db, app_config, enc, jwt_secret } =
        config::bootstrap(data_dir, args.jwt_secret.clone(), args.encryption_key.clone()).await?;
    let jwt = JwtCodec::new(&jwt_secret);

    let settings = settings_queries::get(&db).await?;
    let patch = settings_queries::SettingsPatch {
        port: args.port,
        bind_addr: args.bind_addr.clone(),
        docker_host: args.docker_host.clone().map(Some),
        max_concurrent_jobs: args.max_concurrent_jobs,
    };
    let settings = if patch.is_empty() { settings } else { settings_queries::update(&db, patch).await? };

    let docker = match runner::docker::connect(settings.docker_host.as_deref()) {
        Ok(client) => match runner::docker::ping(&client).await {
            Ok(()) => {
                tracing::info!("connected to Docker Engine");
                Some(client)
            }
            Err(e) => {
                tracing::warn!(error = %e, "Docker Engine unreachable; workflow dispatch will fail until Docker is running");
                None
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "could not connect to Docker; workflow dispatch will fail until Docker is available");
            None
        }
    };

    let log_hub = Arc::new(LogHub::new());
    tokio::spawn(LogHub::run_periodic_flush(log_hub.clone(), db.clone()));

    let port = settings.port as u16;
    let bind_addr = settings.bind_addr.clone();

    let state = AppState(Arc::new(AppStateInner {
        db,
        config: app_config,
        jwt,
        enc,
        docker,
        log_hub,
        github_client: RwLock::new(None),
    }));

    let app = api::router(state);

    let listener = bind_with_fallback(&bind_addr, port).await?;
    let actual_port = listener.local_addr()?.port();
    tracing::info!(port = actual_port, "actions-toolkit listening");
    axum::serve(listener, app).await?;

    Ok(())
}
