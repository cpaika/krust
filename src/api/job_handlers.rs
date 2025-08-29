use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use tracing::{error, info};

use crate::api::server::AppState;

pub async fn create_job(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut job): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate kind
    if job.get("kind").and_then(|k| k.as_str()) != Some("Job") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ensure metadata exists
    if !job.get("metadata").is_some() {
        job["metadata"] = json!({});
    }

    info!(
        "Creating Job {} in namespace {}",
        job["metadata"]["name"].as_str().unwrap_or("unknown"),
        namespace
    );

    let store = state.storage.jobs();
    match store.create(&namespace, job).await {
        Ok(created_job) => Ok((StatusCode::CREATED, Json(created_job))),
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                error!("Job already exists: {}", e);
                return Ok((StatusCode::CONFLICT, Json(json!({
                    "kind": "Status",
                    "apiVersion": "v1",
                    "metadata": {},
                    "status": "Failure",
                    "message": "Job already exists",
                    "reason": "AlreadyExists",
                    "code": 409
                }))));
            }
            error!("Failed to create Job: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_job(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting Job {} in namespace {}", name, namespace);
    
    let store = state.storage.jobs();
    match store.get(&namespace, &name).await {
        Ok(job) => Ok(Json(job)),
        Err(e) if e.to_string().contains("not found") => {
            error!("Job {}/{} not found", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to get Job: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_jobs_namespaced(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("Listing Jobs in namespace {}", namespace);
    
    let store = state.storage.jobs();
    match store.list(Some(&namespace)).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list Jobs: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_jobs_all_namespaces(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    info!("Listing Jobs in all namespaces");
    
    let store = state.storage.jobs();
    match store.list(None).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list Jobs: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_job_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut status_update): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("Updating Job status {} in namespace {}", name, namespace);
    
    let store = state.storage.jobs();
    
    // Extract status from the request body
    let status = if let Some(s) = status_update.get("status") {
        s.clone()
    } else {
        // If the entire body is the status
        status_update
    };
    
    match store.update_status(&namespace, &name, status).await {
        Ok(_) => {
            // Retrieve and return the updated Job
            match store.get(&namespace, &name).await {
                Ok(job) => Ok(Json(job)),
                Err(e) => {
                    error!("Failed to retrieve updated Job: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Err(e) => {
            error!("Failed to update Job status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_job(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Deleting Job {} in namespace {}", name, namespace);
    
    let store = state.storage.jobs();
    match store.delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) if e.to_string().contains("not found") => {
            error!("Job {}/{} not found", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to delete Job: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}