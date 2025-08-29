use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::Value;
use tracing::{error, info};

use crate::api::server::AppState;

pub async fn create_pv(
    State(state): State<AppState>,
    Json(mut pv): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate kind
    if pv.get("kind").and_then(|k| k.as_str()) != Some("PersistentVolume") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ensure metadata exists
    if !pv.get("metadata").is_some() {
        pv["metadata"] = serde_json::json!({});
    }

    info!(
        "Creating PersistentVolume {}",
        pv["metadata"]["name"].as_str().unwrap_or("unknown")
    );

    match state.storage.persistent_volumes().create(pv).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                error!("PersistentVolume already exists: {}", e);
                Err(StatusCode::CONFLICT)
            } else {
                error!("Failed to create PersistentVolume: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn get_pv(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting PersistentVolume {}", name);

    match state.storage.persistent_volumes().get(&name).await {
        Ok(pv) => Ok(Json(pv)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to get PersistentVolume: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn list_pvs(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    info!("Listing PersistentVolumes");

    match state.storage.persistent_volumes().list().await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list PersistentVolumes: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_pv(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(pv): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Validate kind
    if pv.get("kind").and_then(|k| k.as_str()) != Some("PersistentVolume") {
        return Err(StatusCode::BAD_REQUEST);
    }

    info!("Updating PersistentVolume {}", name);

    match state.storage.persistent_volumes().update(&name, pv).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to update PersistentVolume: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_pv(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("Deleting PersistentVolume {}", name);

    match state.storage.persistent_volumes().delete(&name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to delete PersistentVolume: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}