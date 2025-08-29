use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::Value;

use super::server::AppState;

#[derive(Deserialize)]
pub struct ListParams {
    #[serde(rename = "labelSelector")]
    label_selector: Option<String>,
    limit: Option<i32>,
}

// PriorityClass handlers
pub async fn list_priorityclasses(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.priorityclasses().list().await {
        Ok(pcs) => Ok(Json(pcs)),
        Err(e) => {
            tracing::error!("Failed to list priority classes: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_priorityclass(
    State(state): State<AppState>,
    Json(pc): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.priorityclasses().create(pc).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create priority class: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_priorityclass(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.priorityclasses().get(&name).await {
        Ok(Some(pc)) => Ok(Json(pc)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get priority class: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_priorityclass(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(pc): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // PriorityClass doesn't support updates in real k8s, but we'll allow it for now
    match state.storage.priorityclasses().delete(&name).await {
        Ok(_) => {},
        Err(_) => {}
    }
    
    match state.storage.priorityclasses().create(pc).await {
        Ok(created) => Ok(Json(created)),
        Err(e) => {
            tracing::error!("Failed to update priority class: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_priorityclass(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.priorityclasses().delete(&name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            tracing::error!("Failed to delete priority class: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// StorageClass handlers
pub async fn list_storageclasses(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.storageclasses().list().await {
        Ok(scs) => Ok(Json(scs)),
        Err(e) => {
            tracing::error!("Failed to list storage classes: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_storageclass(
    State(state): State<AppState>,
    Json(sc): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.storageclasses().create(sc).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create storage class: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_storageclass(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.storageclasses().get(&name).await {
        Ok(Some(sc)) => Ok(Json(sc)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get storage class: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_storageclass(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(sc): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // StorageClass doesn't support updates in real k8s, but we'll allow it for now
    match state.storage.storageclasses().delete(&name).await {
        Ok(_) => {},
        Err(_) => {}
    }
    
    match state.storage.storageclasses().create(sc).await {
        Ok(created) => Ok(Json(created)),
        Err(e) => {
            tracing::error!("Failed to update storage class: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_storageclass(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.storageclasses().delete(&name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            tracing::error!("Failed to delete storage class: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}