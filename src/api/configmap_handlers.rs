use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use tracing::{error, info};

use super::handlers::ListParams;
use super::server::AppState;

// ConfigMap handlers
pub async fn list_all_configmaps(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.configmaps().list(None).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list configmaps: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_configmaps(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.configmaps().list(Some(&namespace)).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list configmaps in namespace {}: {}", namespace, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_configmap(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut configmap): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate kind
    if configmap.get("kind").and_then(|k| k.as_str()) != Some("ConfigMap") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ensure namespace in metadata matches path
    configmap["metadata"]["namespace"] = json!(namespace);

    match state.storage.configmaps().create(&namespace, configmap).await {
        Ok(created) => {
            info!("Created ConfigMap {}/{}", namespace, created["metadata"]["name"]);
            Ok((StatusCode::CREATED, Json(created)))
        }
        Err(e) => {
            error!("Failed to create configmap: {}", e);
            if e.to_string().contains("UNIQUE constraint") {
                Err(StatusCode::CONFLICT)
            } else {
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn get_configmap(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.configmaps().get(&namespace, &name).await {
        Ok(configmap) => Ok(Json(configmap)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to get configmap {}/{}: {}", namespace, name, e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn update_configmap(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut configmap): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Ensure namespace and name in metadata match path
    configmap["metadata"]["namespace"] = json!(namespace);
    configmap["metadata"]["name"] = json!(name);

    match state.storage.configmaps().update(&namespace, &name, configmap).await {
        Ok(updated) => {
            info!("Updated ConfigMap {}/{}", namespace, name);
            Ok(Json(updated))
        }
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else if e.to_string().contains("immutable") {
                Err(StatusCode::UNPROCESSABLE_ENTITY)
            } else {
                error!("Failed to update configmap {}/{}: {}", namespace, name, e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn patch_configmap(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.configmaps().patch(&namespace, &name, patch).await {
        Ok(patched) => {
            info!("Patched ConfigMap {}/{}", namespace, name);
            Ok(Json(patched))
        }
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else if e.to_string().contains("immutable") {
                Err(StatusCode::UNPROCESSABLE_ENTITY)
            } else {
                error!("Failed to patch configmap {}/{}: {}", namespace, name, e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_configmap(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.configmaps().delete(&namespace, &name).await {
        Ok(deleted) => {
            info!("Deleted ConfigMap {}/{}", namespace, name);
            Ok(Json(deleted))
        }
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to delete configmap {}/{}: {}", namespace, name, e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}