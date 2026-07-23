use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::queries::runs as run_queries;

#[derive(Deserialize)]
pub struct LogWsQuery {
    step_run_id: Option<String>,
}

/// Live log tail for a run. Clients should first fetch historical lines via
/// `GET /api/runs/:id/logs`, then upgrade to this endpoint to receive the ongoing tail so no
/// lines are missed or duplicated across the fetch/subscribe boundary is minimized (a small
/// overlap is possible and expected; the frontend dedupes by log row id).
pub async fn run_logs_ws(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Query(query): Query<LogWsQuery>,
    _user: CurrentUser,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, run_id, query.step_run_id, socket))
}

async fn handle_socket(state: AppState, run_id: String, step_filter: Option<String>, mut socket: WebSocket) {
    let step_run_ids: Vec<String> = match &step_filter {
        Some(id) => vec![id.clone()],
        None => match run_queries::run_tree(&state.db, &run_id).await {
            Ok(Some(tree)) => tree.jobs.into_iter().flat_map(|j| j.steps.into_iter().map(|s| s.id)).collect(),
            _ => vec![],
        },
    };

    let mut receivers: Vec<_> = step_run_ids.iter().map(|id| state.log_hub.subscribe(id)).collect();

    if receivers.is_empty() {
        // No steps yet (run just queued); nothing to stream, close politely.
        let _ = socket.send(Message::Close(None)).await;
        return;
    }

    loop {
        let mut futs = Vec::new();
        for rx in receivers.iter_mut() {
            futs.push(Box::pin(rx.recv()));
        }

        tokio::select! {
            result = futures::future::select_all(futs) => {
                let (line_result, _index, _rest) = result;
                match line_result {
                    Ok(line) => {
                        let payload = serde_json::to_string(&line).unwrap_or_default();
                        if socket.send(Message::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => continue,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => continue,
                }
            }
        }
    }
}

/// Live resource-sample tail for a run, the `StatsHub` equivalent of `run_logs_ws`. Unlike logs
/// (per-step channels fanned out over the whole run tree), stats are already published under one
/// channel per `workflow_run_id` (see `stats_hub::StatsHub`), so there's only ever one receiver to
/// drive here.
pub async fn run_stats_ws(State(state): State<AppState>, Path(run_id): Path<String>, _user: CurrentUser, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_stats_socket(state, run_id, socket))
}

async fn handle_stats_socket(state: AppState, run_id: String, mut socket: WebSocket) {
    let mut rx = state.stats_hub.subscribe(&run_id);
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(sample) => {
                        let payload = serde_json::to_string(&sample).unwrap_or_default();
                        if socket.send(Message::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => continue,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => continue,
                }
            }
        }
    }
}

/// Live "a new run just started" push for a repo's Overview page, the `ActivityHub` equivalent of
/// `run_logs_ws`/`run_stats_ws`. One channel per `repo_id`; each message is the newly created
/// `WorkflowRun` itself so the frontend can show it immediately without a follow-up fetch.
pub async fn run_activity_ws(State(state): State<AppState>, Path(repo_id): Path<String>, _user: CurrentUser, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_activity_socket(state, repo_id, socket))
}

async fn handle_activity_socket(state: AppState, repo_id: String, mut socket: WebSocket) {
    let mut rx = state.activity_hub.subscribe(&repo_id);
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(run) => {
                        let payload = serde_json::to_string(&run).unwrap_or_default();
                        if socket.send(Message::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => continue,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => continue,
                }
            }
        }
    }
}
