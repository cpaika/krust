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

// Role handlers
pub async fn list_roles(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.roles().list(Some(&namespace)).await {
        Ok(roles) => Ok(Json(roles)),
        Err(e) => {
            tracing::error!("Failed to list roles: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_role(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(role): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.roles().create(&namespace, role).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create role: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_role(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.roles().get(&namespace, &name).await {
        Ok(role) => Ok(Json(role)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get role: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn update_role(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(role): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.roles().update(&namespace, &name, role).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to update role: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_role(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.roles().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to delete role: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

// RoleBinding handlers
pub async fn list_rolebindings(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.rolebindings().list(Some(&namespace)).await {
        Ok(rolebindings) => Ok(Json(rolebindings)),
        Err(e) => {
            tracing::error!("Failed to list rolebindings: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_rolebinding(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(rolebinding): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.rolebindings().create(&namespace, rolebinding).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create rolebinding: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_rolebinding(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.rolebindings().get(&namespace, &name).await {
        Ok(rolebinding) => Ok(Json(rolebinding)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get rolebinding: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_rolebinding(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.rolebindings().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to delete rolebinding: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

// ClusterRole handlers
pub async fn list_clusterroles(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.clusterroles().list().await {
        Ok(clusterroles) => Ok(Json(clusterroles)),
        Err(e) => {
            tracing::error!("Failed to list clusterroles: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_clusterrole(
    State(state): State<AppState>,
    Json(clusterrole): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.clusterroles().create(clusterrole).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create clusterrole: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_clusterrole(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.clusterroles().get(&name).await {
        Ok(clusterrole) => Ok(Json(clusterrole)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get clusterrole: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn update_clusterrole(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(clusterrole): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.clusterroles().update(&name, clusterrole).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to update clusterrole: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_clusterrole(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.clusterroles().delete(&name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to delete clusterrole: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

// ClusterRoleBinding handlers
pub async fn list_clusterrolebindings(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.clusterrolebindings().list().await {
        Ok(clusterrolebindings) => Ok(Json(clusterrolebindings)),
        Err(e) => {
            tracing::error!("Failed to list clusterrolebindings: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_clusterrolebinding(
    State(state): State<AppState>,
    Json(clusterrolebinding): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.clusterrolebindings().create(clusterrolebinding).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create clusterrolebinding: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_clusterrolebinding(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.clusterrolebindings().get(&name).await {
        Ok(clusterrolebinding) => Ok(Json(clusterrolebinding)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get clusterrolebinding: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_clusterrolebinding(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.clusterrolebindings().delete(&name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to delete clusterrolebinding: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}