use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use tokio_util::io::ReaderStream;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::models::Artifact;
use crate::db::queries::artifacts as artifact_queries;
use crate::error::{AppError, AppResult};

pub async fn list_for_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<Artifact>>> {
    Ok(Json(artifact_queries::list_for_run(&state.db, &run_id).await?))
}

pub async fn download(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Response> {
    let artifact = artifact_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    let path = std::path::PathBuf::from(&artifact.path_on_disk);

    if path.is_dir() {
        return Err(AppError::BadRequest(
            "artifact is a directory; download individual files or a future zip export".into(),
        ));
    }

    let file = tokio::fs::File::open(&path).await.map_err(|_| AppError::NotFound)?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, artifact.content_type.unwrap_or_else(|| "application/octet-stream".to_string()))
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", artifact.name),
        )
        .body(body)
        .unwrap()
        .into_response())
}
