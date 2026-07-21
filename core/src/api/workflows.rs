use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::models::Workflow as WorkflowRow;
use crate::db::queries::{repos as repo_queries, workflows as workflow_queries};
use crate::error::{AppError, AppResult};
use crate::workflow::{validate, yaml};

pub async fn list_for_repo(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<WorkflowRow>>> {
    Ok(Json(workflow_queries::list_for_repo(&state.db, &repo_id).await?))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<WorkflowRow>> {
    Ok(Json(workflow_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?))
}

#[derive(Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub description: Option<String>,
    pub yaml_source: Option<String>,
    pub workflow_json: Option<serde_json::Value>,
    pub file_path: Option<String>,
}

fn resolve_yaml_and_json(req_yaml: Option<String>, req_json: Option<serde_json::Value>) -> AppResult<(String, String)> {
    let model = if let Some(yaml_source) = &req_yaml {
        yaml::parse(yaml_source).map_err(|e| AppError::BadRequest(e.to_string()))?
    } else if let Some(json) = &req_json {
        serde_json::from_value(json.clone()).map_err(|e| AppError::BadRequest(e.to_string()))?
    } else {
        return Err(AppError::BadRequest("either yaml_source or workflow_json is required".into()));
    };

    validate::validate(&model).map_err(|e| AppError::BadRequest(e.to_string()))?;

    let canonical_yaml = yaml::to_yaml(&model).map_err(AppError::Internal)?;
    let canonical_json = yaml::to_json(&model).map_err(AppError::Internal)?;
    Ok((canonical_yaml, canonical_json))
}

pub async fn create(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    _user: CurrentUser,
    Json(req): Json<CreateWorkflowRequest>,
) -> AppResult<Json<WorkflowRow>> {
    repo_queries::find_by_id(&state.db, &repo_id).await?.ok_or(AppError::NotFound)?;
    let (yaml_source, parsed_json) = resolve_yaml_and_json(req.yaml_source, req.workflow_json)?;
    let file_path = req.file_path.unwrap_or_else(|| format!(".actions-toolkit/{}.yml", req.name));

    let workflow = workflow_queries::create(
        &state.db,
        &repo_id,
        &req.name,
        req.description.as_deref(),
        &file_path,
        &yaml_source,
        &parsed_json,
    )
    .await
        .map_err(|e| match e {
            sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
                AppError::Conflict("a workflow with this name already exists for this repo".into())
            }
            other => AppError::Database(other),
        })?;

    Ok(Json(workflow))
}

#[derive(Deserialize)]
pub struct UpdateWorkflowRequest {
    pub yaml_source: Option<String>,
    pub workflow_json: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct WorkflowSaveResponse {
    pub workflow: WorkflowRow,
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
    Json(req): Json<UpdateWorkflowRequest>,
) -> AppResult<Json<WorkflowSaveResponse>> {
    workflow_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    let (yaml_source, parsed_json) = resolve_yaml_and_json(req.yaml_source, req.workflow_json)?;
    workflow_queries::update(&state.db, &id, &yaml_source, &parsed_json).await?;
    let workflow = workflow_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    Ok(Json(WorkflowSaveResponse { workflow }))
}

#[derive(Deserialize)]
pub struct SetEnabledRequest {
    pub enabled: bool,
}

pub async fn set_enabled(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
    Json(req): Json<SetEnabledRequest>,
) -> AppResult<()> {
    workflow_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    workflow_queries::set_enabled(&state.db, &id, req.enabled).await?;
    Ok(())
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<()> {
    workflow_queries::delete(&state.db, &id).await?;
    Ok(())
}

#[derive(Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct ValidateRequest {
    pub yaml_source: Option<String>,
    pub workflow_json: Option<serde_json::Value>,
}

pub async fn validate_workflow(
    _user: CurrentUser,
    Json(req): Json<ValidateRequest>,
) -> Json<ValidateResponse> {
    match resolve_yaml_and_json(req.yaml_source, req.workflow_json) {
        Ok(_) => Json(ValidateResponse { valid: true, error: None }),
        Err(e) => Json(ValidateResponse {
            valid: false,
            error: Some(e.to_string()),
        }),
    }
}

fn yaml_filename(file_path: &str) -> String {
    file_path.rsplit('/').next().unwrap_or(file_path).to_string()
}

pub async fn export(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Response> {
    let workflow = workflow_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "text/yaml")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", yaml_filename(&workflow.file_path)),
        )
        .body(Body::from(workflow.yaml_source))
        .unwrap()
        .into_response())
}

pub async fn export_repo(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Response> {
    let repo = repo_queries::find_by_id(&state.db, &repo_id).await?.ok_or(AppError::NotFound)?;
    let workflows = workflow_queries::list_for_repo(&state.db, &repo_id).await?;

    let zip_bytes = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut buf);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for workflow in &workflows {
            writer.start_file(&workflow.file_path, options)?;
            std::io::Write::write_all(&mut writer, workflow.yaml_source.as_bytes())?;
        }

        writer.finish()?;
        Ok(buf.into_inner())
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .map_err(AppError::Internal)?;

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}-{}-workflows.zip\"", repo.owner, repo.name),
        )
        .body(Body::from(zip_bytes))
        .unwrap()
        .into_response())
}

pub async fn dispatch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<crate::db::models::WorkflowRun>> {
    let workflow_row = workflow_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    let repo = repo_queries::find_by_id(&state.db, &workflow_row.repo_id).await?.ok_or(AppError::NotFound)?;

    let run = crate::runner::dispatch::spawn_run(
        &state,
        &workflow_row,
        &repo,
        "manual",
        None,
        Some(&format!("refs/heads/{}", repo.default_branch)),
        None,
        None,
    )
    .await
    .map_err(AppError::Internal)?;

    Ok(Json(run))
}
