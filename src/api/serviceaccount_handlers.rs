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

// List all ServiceAccounts across namespaces
pub async fn list_all_serviceaccounts(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.serviceaccounts().list(None).await {
        Ok(sas) => Ok(Json(sas)),
        Err(e) => {
            tracing::error!("Failed to list all service accounts: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// List ServiceAccounts in a namespace
pub async fn list_serviceaccounts(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.serviceaccounts().list(Some(&namespace)).await {
        Ok(sas) => Ok(Json(sas)),
        Err(e) => {
            tracing::error!("Failed to list service accounts: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_serviceaccount(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(sa): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.serviceaccounts().create(&namespace, sa).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create service account: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_serviceaccount(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.serviceaccounts().get(&namespace, &name).await {
        Ok(Some(sa)) => Ok(Json(sa)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get service account: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_serviceaccount(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(sa): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.serviceaccounts().update(&namespace, &name, sa).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to update service account: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn patch_serviceaccount(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Get current ServiceAccount
    let current = match state.storage.serviceaccounts().get(&namespace, &name).await {
        Ok(Some(sa)) => sa,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get service account for patch: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Apply JSON merge patch
    let mut patched = current;
    if let Some(obj) = patch.as_object() {
        if let Some(patched_obj) = patched.as_object_mut() {
            for (key, value) in obj {
                patched_obj.insert(key.clone(), value.clone());
            }
        }
    }

    // Update the ServiceAccount
    match state.storage.serviceaccounts().update(&namespace, &name, patched).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to patch service account: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_serviceaccount(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.serviceaccounts().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            tracing::error!("Failed to delete service account: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Create token for ServiceAccount
pub async fn create_serviceaccount_token(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(token_request): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.serviceaccounts().create_token(&namespace, &name, token_request).await {
        Ok(token_response) => Ok((StatusCode::CREATED, Json(token_response))),
        Err(e) => {
            tracing::error!("Failed to create service account token: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}