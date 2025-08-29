use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::Value;
use tracing::{error, info};

use crate::api::server::AppState;

pub async fn create_daemonset(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut daemonset): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate kind
    if daemonset.get("kind").and_then(|k| k.as_str()) != Some("DaemonSet") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ensure metadata exists
    if !daemonset.get("metadata").is_some() {
        daemonset["metadata"] = serde_json::json!({});
    }

    info!(
        "Creating DaemonSet {} in namespace {}",
        daemonset["metadata"]["name"].as_str().unwrap_or("unknown"),
        namespace
    );

    match state.storage.daemonsets().create(&namespace, daemonset).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                error!("DaemonSet already exists: {}", e);
                Err(StatusCode::CONFLICT)
            } else {
                error!("Failed to create DaemonSet: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn get_daemonset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting DaemonSet {} in namespace {}", name, namespace);

    match state.storage.daemonsets().get(&namespace, &name).await {
        Ok(daemonset) => Ok(Json(daemonset)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to get DaemonSet: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn list_daemonsets(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("Listing DaemonSets in namespace {}", namespace);

    match state.storage.daemonsets().list(Some(&namespace)).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list DaemonSets: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_all_daemonsets(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    info!("Listing all DaemonSets");

    match state.storage.daemonsets().list(None).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list all DaemonSets: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_daemonset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(daemonset): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Validate kind
    if daemonset.get("kind").and_then(|k| k.as_str()) != Some("DaemonSet") {
        return Err(StatusCode::BAD_REQUEST);
    }

    info!("Updating DaemonSet {} in namespace {}", name, namespace);

    match state.storage.daemonsets().update(&namespace, &name, daemonset).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to update DaemonSet: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn get_daemonset_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting DaemonSet status {} in namespace {}", name, namespace);

    match state.storage.daemonsets().get(&namespace, &name).await {
        Ok(daemonset) => Ok(Json(daemonset)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to get DaemonSet status: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_daemonset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Deleting DaemonSet {} in namespace {}", name, namespace);

    match state.storage.daemonsets().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to delete DaemonSet: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}