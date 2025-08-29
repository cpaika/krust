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

// ResourceQuota handlers - List all resourcequotas across namespaces
pub async fn list_all_resourcequotas(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.resourcequotas().list(None).await {
        Ok(quotas) => Ok(Json(quotas)),
        Err(e) => {
            tracing::error!("Failed to list all resource quotas: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_resourcequotas(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.resourcequotas().list(Some(&namespace)).await {
        Ok(quotas) => Ok(Json(quotas)),
        Err(e) => {
            tracing::error!("Failed to list resource quotas: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_resourcequota(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(quota): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.resourcequotas().create(&namespace, quota).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create resource quota: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_resourcequota(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.resourcequotas().get(&namespace, &name).await {
        Ok(Some(quota)) => Ok(Json(quota)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get resource quota: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_resourcequota(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(quota): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.resourcequotas().update(&namespace, &name, quota).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to update resource quota: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_resourcequota(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.resourcequotas().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            tracing::error!("Failed to delete resource quota: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_resourcequota_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.resourcequotas().get(&namespace, &name).await {
        Ok(Some(quota)) => Ok(Json(quota)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get resource quota status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_resourcequota_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(status_update): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let status = status_update["status"].clone();
    match state.storage.resourcequotas().update_status(&namespace, &name, status).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to update resource quota status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// LimitRange handlers - List all limitranges across namespaces
pub async fn list_all_limitranges(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.limitranges().list(None).await {
        Ok(limitranges) => Ok(Json(limitranges)),
        Err(e) => {
            tracing::error!("Failed to list all limit ranges: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_limitranges(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.limitranges().list(Some(&namespace)).await {
        Ok(limitranges) => Ok(Json(limitranges)),
        Err(e) => {
            tracing::error!("Failed to list limit ranges: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_limitrange(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(limitrange): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.limitranges().create(&namespace, limitrange).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create limit range: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_limitrange(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.limitranges().get(&namespace, &name).await {
        Ok(Some(limitrange)) => Ok(Json(limitrange)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get limit range: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_limitrange(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(limitrange): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.limitranges().update(&namespace, &name, limitrange).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to update limit range: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_limitrange(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.limitranges().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            tracing::error!("Failed to delete limit range: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}