use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};

use crate::app::AppState;
use crate::db::queries::{repos as repo_queries, webhook_events as event_queries, workflows as workflow_queries};
use crate::workflow::{trigger_match, yaml};

/// Public webhook receiver. Not behind the auth middleware; protected instead by the per-repo
/// HMAC signature on the raw request body.
pub async fn receive(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Ok(Some(repo)) = repo_queries::find_by_id(&state.db, &repo_id).await else {
        return StatusCode::NOT_FOUND;
    };

    let github_event = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let delivery_id = headers.get("X-GitHub-Delivery").and_then(|v| v.to_str().ok()).map(str::to_string);
    let signature = headers.get("X-Hub-Signature-256").and_then(|v| v.to_str().ok()).unwrap_or("");

    let secret = match state.enc.decrypt_str(&repo.webhook_secret_encrypted, &repo.webhook_secret_nonce) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, repo_id, "failed to decrypt webhook secret");
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };

    let signature_valid = crate::github::webhook_verify::verify(&secret, &body, signature);

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => serde_json::Value::Null,
    };
    let payload_json = payload.to_string();

    if !signature_valid {
        tracing::warn!(repo_id, github_event, "webhook signature verification failed");
        let _ = event_queries::record(
            &state.db,
            Some(&repo_id),
            &github_event,
            delivery_id.as_deref(),
            &payload_json,
            false,
            "[]",
        )
        .await;
        return StatusCode::UNAUTHORIZED;
    }

    let mut matched_ids = Vec::new();
    if let Ok(workflows) = workflow_queries::list_enabled_for_repo(&state.db, &repo_id).await {
        for workflow_row in workflows {
            let Ok(model) = yaml::parse(&workflow_row.yaml_source) else { continue };
            let Some(matched) = trigger_match::matches(&model, &github_event, &payload) else { continue };

            matched_ids.push(workflow_row.id.clone());

            if let Err(e) = crate::runner::dispatch::spawn_run(
                &state,
                &workflow_row,
                &repo,
                &github_event,
                Some(&payload_json),
                matched.ref_name.as_deref(),
                matched.commit_sha.as_deref(),
            )
            .await
            {
                tracing::error!(error = %e, workflow_id = %workflow_row.id, "failed to spawn run for matched webhook");
            }
        }
    }

    let matched_json = serde_json::to_string(&matched_ids).unwrap_or_else(|_| "[]".to_string());
    let _ = event_queries::record(
        &state.db,
        Some(&repo_id),
        &github_event,
        delivery_id.as_deref(),
        &payload_json,
        true,
        &matched_json,
    )
    .await;

    StatusCode::OK
}
