use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Json, IntoResponse},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::server::AppState;

#[derive(Deserialize)]
pub struct ListParams {
    #[serde(rename = "labelSelector")]
    label_selector: Option<String>,
    #[serde(rename = "fieldSelector")]
    field_selector: Option<String>,
    limit: Option<i32>,
    #[serde(rename = "continue")]
    continue_token: Option<String>,
    watch: Option<bool>,
    #[serde(rename = "resourceVersion")]
    resource_version: Option<String>,
}

// Namespace handlers
pub async fn list_namespaces(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    Ok(Json(json!({
        "apiVersion": "v1",
        "kind": "NamespaceList",
        "metadata": {
            "resourceVersion": "1"
        },
        "items": []
    })))
}

pub async fn create_namespace(
    State(state): State<AppState>,
    Json(namespace): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    Ok((StatusCode::CREATED, Json(namespace)))
}

pub async fn get_namespace(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    Err(StatusCode::NOT_FOUND)
}

pub async fn update_namespace(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(namespace): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    Ok(Json(namespace))
}

pub async fn patch_namespace(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    Ok(Json(patch))
}

pub async fn delete_namespace(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> StatusCode {
    StatusCode::OK
}

// Pod handlers
pub async fn list_all_pods(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.pods().list(None).await {
        Ok(pods) => Ok(Json(pods)),
        Err(e) => {
            tracing::error!("Failed to list pods: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_pods(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.pods().list(Some(&namespace)).await {
        Ok(pods) => Ok(Json(pods)),
        Err(e) => {
            tracing::error!("Failed to list pods in namespace {}: {}", namespace, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_pod(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(pod): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.pods().create(&namespace, pod).await {
        Ok(created_pod) => Ok((StatusCode::CREATED, Json(created_pod))),
        Err(e) => {
            tracing::error!("Failed to create pod: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_pod(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.pods().get(&namespace, &name).await {
        Ok(pod) => Ok(Json(pod)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get pod: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn update_pod(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(pod): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.pods().update(&namespace, &name, pod).await {
        Ok(updated_pod) => Ok(Json(updated_pod)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to update pod: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn patch_pod(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // For now, patch is implemented as a full update
    // In a real implementation, we'd merge the patch with the existing object
    match state.storage.pods().get(&namespace, &name).await {
        Ok(mut pod) => {
            // Simple merge of patch into pod
            if let Some(metadata) = patch["metadata"].as_object() {
                for (key, value) in metadata {
                    pod["metadata"][key] = value.clone();
                }
            }
            if let Some(spec) = patch["spec"].as_object() {
                for (key, value) in spec {
                    pod["spec"][key] = value.clone();
                }
            }
            if let Some(status) = patch["status"].as_object() {
                for (key, value) in status {
                    pod["status"][key] = value.clone();
                }
            }
            
            match state.storage.pods().update(&namespace, &name, pod).await {
                Ok(updated_pod) => Ok(Json(updated_pod)),
                Err(e) => {
                    tracing::error!("Failed to patch pod: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get pod for patching: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_pod(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.pods().delete(&namespace, &name).await {
        Ok(_) => Ok(Json(json!({
            "kind": "Status",
            "apiVersion": "v1",
            "metadata": {},
            "status": "Success",
            "details": {
                "name": name,
                "kind": "Pod",
                "uid": ""
            }
        }))),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to delete pod: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

// Service handlers
pub async fn list_all_services(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    Ok(Json(json!({
        "apiVersion": "v1",
        "kind": "ServiceList",
        "metadata": {
            "resourceVersion": "1"
        },
        "items": []
    })))
}

pub async fn list_services(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    Ok(Json(json!({
        "apiVersion": "v1",
        "kind": "ServiceList",
        "metadata": {
            "resourceVersion": "1"
        },
        "items": []
    })))
}

pub async fn create_service(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(service): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    Ok((StatusCode::CREATED, Json(service)))
}

pub async fn get_service(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    Err(StatusCode::NOT_FOUND)
}

pub async fn update_service(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(service): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    Ok(Json(service))
}

pub async fn patch_service(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    Ok(Json(patch))
}

pub async fn delete_service(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> StatusCode {
    StatusCode::OK
}

// Node handlers
pub async fn list_nodes(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    Ok(Json(json!({
        "apiVersion": "v1",
        "kind": "NodeList",
        "metadata": {
            "resourceVersion": "1"
        },
        "items": [{
            "apiVersion": "v1",
            "kind": "Node",
            "metadata": {
                "name": "krust-node",
                "uid": "node-uid-1",
                "resourceVersion": "1",
                "creationTimestamp": chrono::Utc::now().to_rfc3339()
            },
            "spec": {},
            "status": {
                "conditions": [
                    {
                        "type": "Ready",
                        "status": "True",
                        "lastHeartbeatTime": chrono::Utc::now().to_rfc3339(),
                        "lastTransitionTime": chrono::Utc::now().to_rfc3339(),
                        "reason": "KubeletReady",
                        "message": "kubelet is posting ready status"
                    }
                ],
                "addresses": [
                    {
                        "type": "InternalIP",
                        "address": "127.0.0.1"
                    }
                ],
                "capacity": {
                    "cpu": "8",
                    "memory": "16Gi",
                    "pods": "110"
                },
                "allocatable": {
                    "cpu": "8",
                    "memory": "16Gi",
                    "pods": "110"
                }
            }
        }]
    })))
}

pub async fn get_node(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    if name == "krust-node" {
        Ok(Json(json!({
            "apiVersion": "v1",
            "kind": "Node",
            "metadata": {
                "name": "krust-node",
                "uid": "node-uid-1",
                "resourceVersion": "1",
                "creationTimestamp": chrono::Utc::now().to_rfc3339()
            },
            "spec": {},
            "status": {
                "conditions": [
                    {
                        "type": "Ready",
                        "status": "True",
                        "lastHeartbeatTime": chrono::Utc::now().to_rfc3339(),
                        "lastTransitionTime": chrono::Utc::now().to_rfc3339(),
                        "reason": "KubeletReady",
                        "message": "kubelet is posting ready status"
                    }
                ],
                "addresses": [
                    {
                        "type": "InternalIP",
                        "address": "127.0.0.1"
                    }
                ],
                "capacity": {
                    "cpu": "8",
                    "memory": "16Gi",
                    "pods": "110"
                },
                "allocatable": {
                    "cpu": "8",
                    "memory": "16Gi",
                    "pods": "110"
                }
            }
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

// Watch handlers
pub async fn watch_pods(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, StatusCode> {
    use axum::response::sse::{Event, Sse};
    use futures::stream::StreamExt;
    
    if params.watch != Some(true) {
        // Regular list if not watching
        return match state.storage.pods().list(None).await {
            Ok(pods) => Ok(Json(pods).into_response()),
            Err(e) => {
                tracing::error!("Failed to list pods: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        };
    }
    
    // Stream watch events
    match state.storage.watch().watch_stream(
        "pods".to_string(),
        None,
        params.resource_version.clone()
    ).await {
        Ok(stream) => {
            let sse_stream = stream.map(|result| match result {
                Ok(event) => Ok(Event::default().data(event.to_string())),
                Err(e) => {
                    tracing::error!("Watch stream error: {}", e);
                    Err(axum::Error::new(e))
                }
            });
            
            Ok(Sse::new(sse_stream)
                .keep_alive(axum::response::sse::KeepAlive::default())
                .into_response())
        }
        Err(e) => {
            tracing::error!("Failed to create watch stream: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn watch_namespace_pods(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(params): Query<ListParams>,
) -> Result<axum::response::Response, StatusCode> {
    use axum::response::sse::{Event, Sse};
    use futures::stream::StreamExt;
    
    if params.watch != Some(true) {
        // Regular list if not watching
        return match state.storage.pods().list(Some(&namespace)).await {
            Ok(pods) => Ok(Json(pods).into_response()),
            Err(e) => {
                tracing::error!("Failed to list pods in namespace {}: {}", namespace, e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        };
    }
    
    // Stream watch events
    match state.storage.watch().watch_stream(
        "pods".to_string(),
        Some(namespace.clone()),
        params.resource_version.clone()
    ).await {
        Ok(stream) => {
            let sse_stream = stream.map(|result| match result {
                Ok(event) => Ok(Event::default().data(event.to_string())),
                Err(e) => {
                    tracing::error!("Watch stream error: {}", e);
                    Err(axum::Error::new(e))
                }
            });
            
            Ok(Sse::new(sse_stream)
                .keep_alive(axum::response::sse::KeepAlive::default())
                .into_response())
        }
        Err(e) => {
            tracing::error!("Failed to create watch stream: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}