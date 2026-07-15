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

use clap::Parser;
use tokio::sync::RwLock;

use crate::app::{AppState, AppStateInner};
use crate::auth::jwt::JwtCodec;
use crate::config::AppConfig;
use crate::crypto::EncryptionKey;
use crate::runner::log_stream::LogHub;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init();

    let config = AppConfig::parse();
    std::fs::create_dir_all(&config.data_dir)?;
    std::fs::create_dir_all(config.workspaces_dir())?;
    std::fs::create_dir_all(config.artifacts_dir())?;

    let db = db::connect(&config.db_path()).await?;
    tracing::info!(path = %config.db_path().display(), "database ready");

    let enc = EncryptionKey::load_or_generate(config.encryption_key.as_deref(), &config.secrets_dir())?;

    let jwt_secret = match &config.jwt_secret {
        Some(s) => s.clone(),
        None => {
            let path = config.secrets_dir().join("jwt.key");
            std::fs::create_dir_all(&config.secrets_dir())?;
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
    let jwt = JwtCodec::new(&jwt_secret);

    let docker = match runner::docker::connect(config.docker_host.as_deref()) {
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

    let port = config.port;
    let bind_addr = config.bind_addr.clone();

    let state = AppState(Arc::new(AppStateInner {
        db,
        config,
        jwt,
        enc,
        docker,
        log_hub,
        github_client: RwLock::new(None),
    }));

    let app = api::router(state);

    let listener = tokio::net::TcpListener::bind(format!("{bind_addr}:{port}")).await?;
    tracing::info!(port, "actions-toolkit listening");
    axum::serve(listener, app).await?;

    Ok(())
}
