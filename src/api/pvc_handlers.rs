use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::Value;
use tracing::{error, info};

use crate::api::server::AppState;

pub async fn create_pvc(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut pvc): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate kind
    if pvc.get("kind").and_then(|k| k.as_str()) != Some("PersistentVolumeClaim") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ensure metadata exists
    if !pvc.get("metadata").is_some() {
        pvc["metadata"] = serde_json::json!({});
    }

    info!(
        "Creating PersistentVolumeClaim {} in namespace {}",
        pvc["metadata"]["name"].as_str().unwrap_or("unknown"),
        namespace
    );

    match state.storage.persistent_volume_claims().create(&namespace, pvc).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                error!("PersistentVolumeClaim already exists: {}", e);
                Err(StatusCode::CONFLICT)
            } else {
                error!("Failed to create PersistentVolumeClaim: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn get_pvc(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting PersistentVolumeClaim {} in namespace {}", name, namespace);

    match state.storage.persistent_volume_claims().get(&namespace, &name).await {
        Ok(pvc) => Ok(Json(pvc)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to get PersistentVolumeClaim: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn list_pvcs(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("Listing PersistentVolumeClaims in namespace {}", namespace);

    match state.storage.persistent_volume_claims().list(Some(&namespace)).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list PersistentVolumeClaims: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_all_pvcs(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    info!("Listing all PersistentVolumeClaims");

    match state.storage.persistent_volume_claims().list(None).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list all PersistentVolumeClaims: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_pvc(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(pvc): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Validate kind
    if pvc.get("kind").and_then(|k| k.as_str()) != Some("PersistentVolumeClaim") {
        return Err(StatusCode::BAD_REQUEST);
    }

    info!("Updating PersistentVolumeClaim {} in namespace {}", name, namespace);

    match state.storage.persistent_volume_claims().update(&namespace, &name, pvc).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to update PersistentVolumeClaim: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_pvc(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Deleting PersistentVolumeClaim {} in namespace {}", name, namespace);

    match state.storage.persistent_volume_claims().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to delete PersistentVolumeClaim: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}