use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use tracing::{error, info};

use crate::api::server::AppState;

pub async fn create_statefulset(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut statefulset): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate kind
    if statefulset.get("kind").and_then(|k| k.as_str()) != Some("StatefulSet") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ensure metadata exists
    if !statefulset.get("metadata").is_some() {
        statefulset["metadata"] = json!({});
    }

    info!(
        "Creating StatefulSet {} in namespace {}",
        statefulset["metadata"]["name"].as_str().unwrap_or("unknown"),
        namespace
    );

    match state.storage.statefulsets().create(&namespace, statefulset).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                error!("StatefulSet already exists: {}", e);
                Err(StatusCode::CONFLICT)
            } else {
                error!("Failed to create StatefulSet: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn get_statefulset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting StatefulSet {} in namespace {}", name, namespace);

    match state.storage.statefulsets().get(&namespace, &name).await {
        Ok(statefulset) => Ok(Json(statefulset)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to get StatefulSet: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn list_statefulsets(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("Listing StatefulSets in namespace {}", namespace);

    match state.storage.statefulsets().list(Some(&namespace)).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list StatefulSets: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_all_statefulsets(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    info!("Listing all StatefulSets");

    match state.storage.statefulsets().list(None).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list all StatefulSets: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_statefulset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(statefulset): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Validate kind
    if statefulset.get("kind").and_then(|k| k.as_str()) != Some("StatefulSet") {
        return Err(StatusCode::BAD_REQUEST);
    }

    info!("Updating StatefulSet {} in namespace {}", name, namespace);

    match state.storage.statefulsets().update(&namespace, &name, statefulset).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to update StatefulSet: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn get_statefulset_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting StatefulSet scale {} in namespace {}", name, namespace);

    match state.storage.statefulsets().get(&namespace, &name).await {
        Ok(statefulset) => {
            let scale = json!({
                "apiVersion": "autoscaling/v1",
                "kind": "Scale",
                "metadata": statefulset["metadata"],
                "spec": {
                    "replicas": statefulset["spec"]["replicas"]
                },
                "status": {
                    "replicas": statefulset["status"]["replicas"],
                    "selector": statefulset["spec"]["selector"]
                }
            });
            Ok(Json(scale))
        }
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to get StatefulSet scale: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn update_statefulset_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(scale): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Validate kind
    if scale.get("kind").and_then(|k| k.as_str()) != Some("Scale") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let replicas = scale["spec"]["replicas"]
        .as_i64()
        .ok_or(StatusCode::BAD_REQUEST)?;

    info!("Scaling StatefulSet {} in namespace {} to {} replicas", name, namespace, replicas);

    match state.storage.statefulsets().update_scale(&namespace, &name, replicas).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to scale StatefulSet: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn get_statefulset_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting StatefulSet status {} in namespace {}", name, namespace);

    match state.storage.statefulsets().get(&namespace, &name).await {
        Ok(statefulset) => Ok(Json(statefulset)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to get StatefulSet status: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_statefulset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Deleting StatefulSet {} in namespace {}", name, namespace);

    match state.storage.statefulsets().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to delete StatefulSet: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}