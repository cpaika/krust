use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use tracing::{error, info};

use crate::api::server::AppState;

pub async fn create_cronjob(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut cronjob): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate kind
    if cronjob.get("kind").and_then(|k| k.as_str()) != Some("CronJob") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ensure metadata exists
    if !cronjob.get("metadata").is_some() {
        cronjob["metadata"] = json!({});
    }

    info!(
        "Creating CronJob {} in namespace {}",
        cronjob["metadata"]["name"].as_str().unwrap_or("unknown"),
        namespace
    );

    let store = state.storage.cronjobs();
    match store.create(&namespace, cronjob).await {
        Ok(created_cronjob) => Ok((StatusCode::CREATED, Json(created_cronjob))),
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                error!("CronJob already exists: {}", e);
                return Ok((StatusCode::CONFLICT, Json(json!({
                    "kind": "Status",
                    "apiVersion": "v1",
                    "metadata": {},
                    "status": "Failure",
                    "message": "CronJob already exists",
                    "reason": "AlreadyExists",
                    "code": 409
                }))));
            }
            error!("Failed to create CronJob: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_cronjob(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting CronJob {} in namespace {}", name, namespace);
    
    let store = state.storage.cronjobs();
    match store.get(&namespace, &name).await {
        Ok(cronjob) => Ok(Json(cronjob)),
        Err(e) if e.to_string().contains("not found") => {
            error!("CronJob {}/{} not found", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to get CronJob: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_cronjobs_namespaced(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("Listing CronJobs in namespace {}", namespace);
    
    let store = state.storage.cronjobs();
    match store.list(Some(&namespace)).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list CronJobs: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_cronjobs_all_namespaces(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    info!("Listing CronJobs in all namespaces");
    
    let store = state.storage.cronjobs();
    match store.list(None).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list CronJobs: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_cronjob_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(status_update): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("Updating CronJob status {} in namespace {}", name, namespace);
    
    let store = state.storage.cronjobs();
    
    // Extract status from the request body
    let status = if let Some(s) = status_update.get("status") {
        s.clone()
    } else {
        // If the entire body is the status
        status_update
    };
    
    match store.update_status(&namespace, &name, status).await {
        Ok(_) => {
            // Retrieve and return the updated CronJob
            match store.get(&namespace, &name).await {
                Ok(cronjob) => Ok(Json(cronjob)),
                Err(e) => {
                    error!("Failed to retrieve updated CronJob: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Err(e) => {
            error!("Failed to update CronJob status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_cronjob(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Deleting CronJob {} in namespace {}", name, namespace);
    
    let store = state.storage.cronjobs();
    match store.delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) if e.to_string().contains("not found") => {
            error!("CronJob {}/{} not found", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to delete CronJob: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}