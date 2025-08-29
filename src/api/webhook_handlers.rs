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

// ValidatingWebhookConfiguration handlers
pub async fn list_validating_webhooks(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.validating_webhooks().list().await {
        Ok(vwcs) => Ok(Json(vwcs)),
        Err(e) => {
            tracing::error!("Failed to list validating webhook configurations: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_validating_webhook(
    State(state): State<AppState>,
    Json(vwc): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.validating_webhooks().create(vwc).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create validating webhook configuration: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_validating_webhook(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.validating_webhooks().get(&name).await {
        Ok(Some(vwc)) => Ok(Json(vwc)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get validating webhook configuration: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_validating_webhook(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(vwc): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.validating_webhooks().update(&name, vwc).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to update validating webhook configuration: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_validating_webhook(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.validating_webhooks().delete(&name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            tracing::error!("Failed to delete validating webhook configuration: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// MutatingWebhookConfiguration handlers
pub async fn list_mutating_webhooks(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.mutating_webhooks().list().await {
        Ok(mwcs) => Ok(Json(mwcs)),
        Err(e) => {
            tracing::error!("Failed to list mutating webhook configurations: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_mutating_webhook(
    State(state): State<AppState>,
    Json(mwc): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.mutating_webhooks().create(mwc).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create mutating webhook configuration: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_mutating_webhook(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.mutating_webhooks().get(&name).await {
        Ok(Some(mwc)) => Ok(Json(mwc)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get mutating webhook configuration: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_mutating_webhook(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(mwc): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.mutating_webhooks().update(&name, mwc).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to update mutating webhook configuration: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_mutating_webhook(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.mutating_webhooks().delete(&name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            tracing::error!("Failed to delete mutating webhook configuration: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}