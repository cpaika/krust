use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Json, IntoResponse},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx;

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
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.services().list(None).await {
        Ok(services) => Ok(Json(services)),
        Err(e) => {
            tracing::error!("Failed to list services: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_services(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.services().list(Some(&namespace)).await {
        Ok(services) => Ok(Json(services)),
        Err(e) => {
            tracing::error!("Failed to list services in namespace {}: {}", namespace, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_service(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(service): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.services().create(&namespace, service).await {
        Ok(created_service) => Ok((StatusCode::CREATED, Json(created_service))),
        Err(e) => {
            tracing::error!("Failed to create service: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_service(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.services().get(&namespace, &name).await {
        Ok(service) => Ok(Json(service)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get service: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
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
) -> Result<Json<Value>, StatusCode> {
    match state.storage.services().delete(&namespace, &name).await {
        Ok(_) => Ok(Json(json!({
            "kind": "Status",
            "apiVersion": "v1",
            "metadata": {},
            "status": "Success",
            "details": {
                "name": name,
                "kind": "Service",
                "uid": ""
            }
        }))),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to delete service: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

// Endpoints handlers
pub async fn list_all_endpoints(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.endpoints().list(None).await {
        Ok(endpoints) => Ok(Json(endpoints)),
        Err(e) => {
            tracing::error!("Failed to list endpoints: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_endpoints(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.endpoints().list(Some(&namespace)).await {
        Ok(endpoints) => Ok(Json(endpoints)),
        Err(e) => {
            tracing::error!("Failed to list endpoints in namespace {}: {}", namespace, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_endpoints(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.endpoints().get(&namespace, &name).await {
        Ok(endpoints) => Ok(Json(endpoints)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get endpoints: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

// Deployment handlers
pub async fn list_all_deployments(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.deployments().list(None).await {
        Ok(deployments) => Ok(Json(deployments)),
        Err(e) => {
            tracing::error!("Failed to list deployments: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_deployments(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.deployments().list(Some(&namespace)).await {
        Ok(deployments) => Ok(Json(deployments)),
        Err(e) => {
            tracing::error!("Failed to list deployments in namespace {}: {}", namespace, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_deployment(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(deployment): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.deployments().create(&namespace, deployment).await {
        Ok(created_deployment) => Ok((StatusCode::CREATED, Json(created_deployment))),
        Err(e) => {
            tracing::error!("Failed to create deployment: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_deployment(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.deployments().get(&namespace, &name).await {
        Ok(deployment) => Ok(Json(deployment)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get deployment: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn update_deployment(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(deployment): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.deployments().update(&namespace, &name, deployment).await {
        Ok(updated_deployment) => Ok(Json(updated_deployment)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to update deployment: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn patch_deployment(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Handle scale subresource
    if let Some(replicas) = patch["spec"]["replicas"].as_i64() {
        match state.storage.deployments().get(&namespace, &name).await {
            Ok(mut deployment) => {
                deployment["spec"]["replicas"] = json!(replicas);
                match state.storage.deployments().update(&namespace, &name, deployment).await {
                    Ok(updated) => Ok(Json(updated)),
                    Err(e) => {
                        tracing::error!("Failed to scale deployment: {}", e);
                        Err(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                }
            }
            Err(e) => {
                if e.to_string().contains("not found") {
                    Err(StatusCode::NOT_FOUND)
                } else {
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
    } else {
        // Regular patch
        match state.storage.deployments().get(&namespace, &name).await {
            Ok(mut deployment) => {
                // Merge patch into deployment
                if let Some(metadata) = patch["metadata"].as_object() {
                    for (key, value) in metadata {
                        deployment["metadata"][key] = value.clone();
                    }
                }
                if let Some(spec) = patch["spec"].as_object() {
                    for (key, value) in spec {
                        deployment["spec"][key] = value.clone();
                    }
                }
                
                match state.storage.deployments().update(&namespace, &name, deployment).await {
                    Ok(updated) => Ok(Json(updated)),
                    Err(e) => {
                        tracing::error!("Failed to patch deployment: {}", e);
                        Err(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                }
            }
            Err(e) => {
                if e.to_string().contains("not found") {
                    Err(StatusCode::NOT_FOUND)
                } else {
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
    }
}

pub async fn delete_deployment(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.deployments().delete(&namespace, &name).await {
        Ok(_) => Ok(Json(json!({
            "kind": "Status",
            "apiVersion": "v1",
            "metadata": {},
            "status": "Success",
            "details": {
                "name": name,
                "group": "apps",
                "kind": "Deployment",
                "uid": ""
            }
        }))),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to delete deployment: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

// Deployment scale subresource handlers
pub async fn get_deployment_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.deployments().get(&namespace, &name).await {
        Ok(deployment) => {
            Ok(Json(json!({
                "apiVersion": "autoscaling/v1",
                "kind": "Scale",
                "metadata": {
                    "name": name,
                    "namespace": namespace,
                    "uid": deployment["metadata"]["uid"],
                    "resourceVersion": deployment["metadata"]["resourceVersion"]
                },
                "spec": {
                    "replicas": deployment["spec"]["replicas"]
                },
                "status": {
                    "replicas": deployment["status"]["replicas"],
                    "selector": deployment["spec"]["selector"]
                }
            })))
        }
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn update_deployment_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(scale): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let new_replicas = scale["spec"]["replicas"].as_i64();
    
    if let Some(replicas) = new_replicas {
        match state.storage.deployments().get(&namespace, &name).await {
            Ok(mut deployment) => {
                deployment["spec"]["replicas"] = json!(replicas);
                
                // Also need to update the ReplicaSet
                update_replicaset_replicas(&state, &namespace, &name, replicas).await;
                
                match state.storage.deployments().update(&namespace, &name, deployment).await {
                    Ok(updated) => {
                        Ok(Json(json!({
                            "apiVersion": "autoscaling/v1",
                            "kind": "Scale",
                            "metadata": {
                                "name": name,
                                "namespace": namespace,
                                "uid": updated["metadata"]["uid"],
                                "resourceVersion": updated["metadata"]["resourceVersion"]
                            },
                            "spec": {
                                "replicas": replicas
                            },
                            "status": {
                                "replicas": updated["status"]["replicas"],
                                "selector": updated["spec"]["selector"]
                            }
                        })))
                    }
                    Err(e) => {
                        tracing::error!("Failed to scale deployment: {}", e);
                        Err(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                }
            }
            Err(e) => {
                if e.to_string().contains("not found") {
                    Err(StatusCode::NOT_FOUND)
                } else {
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}

pub async fn patch_deployment_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(scale): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    update_deployment_scale(State(state), Path((namespace, name)), Json(scale)).await
}

async fn update_replicaset_replicas(state: &AppState, namespace: &str, deployment_name: &str, replicas: i64) {
    // Find ReplicaSets owned by this deployment
    if let Ok(deployment) = state.storage.deployments().get(namespace, deployment_name).await {
        let deployment_uid = deployment["metadata"]["uid"].as_str().unwrap();
        
        // Update the replicas in the database directly for now
        let _ = sqlx::query(
            "UPDATE replicasets SET replicas = ? 
             WHERE namespace = ? AND owner_references LIKE ?"
        )
        .bind(replicas)
        .bind(namespace)
        .bind(format!("%\"uid\":\"{}%", deployment_uid))
        .execute(&*state.storage.pool)
        .await;
    }
}

// ReplicaSet handlers
pub async fn list_all_replicasets(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.replicasets().list(None).await {
        Ok(replicasets) => Ok(Json(replicasets)),
        Err(e) => {
            tracing::error!("Failed to list replicasets: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_replicasets(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.replicasets().list(Some(&namespace)).await {
        Ok(replicasets) => Ok(Json(replicasets)),
        Err(e) => {
            tracing::error!("Failed to list replicasets in namespace {}: {}", namespace, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_replicaset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.replicasets().get(&namespace, &name).await {
        Ok(replicaset) => Ok(Json(replicaset)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get replicaset: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
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

// Pod logs handler
pub async fn get_pod_logs(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(params): Query<LogParams>,
) -> Result<String, StatusCode> {
    // First check if pod exists
    let pod = match state.storage.pods().get(&namespace, &name).await {
        Ok(pod) => pod,
        Err(_) => return Err(StatusCode::NOT_FOUND),
    };
    
    // Get container name (default to first container if not specified)
    let container_name = if let Some(container) = params.container {
        container
    } else if let Some(containers) = pod["spec"]["containers"].as_array() {
        containers.first()
            .and_then(|c| c["name"].as_str())
            .unwrap_or("container")
            .to_string()
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };
    
    // Get logs from Docker
    let uid = pod["metadata"]["uid"].as_str().unwrap();
    let full_container_name = format!("k8s_{}_{}_{}_{}", 
        container_name, name, namespace, uid);
    
    match get_container_logs(&full_container_name, params.tail, params.follow).await {
        Ok(logs) => Ok(logs),
        Err(e) => {
            tracing::error!("Failed to get logs for container {}: {}", full_container_name, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_container_logs(container_name: &str, tail: Option<String>, follow: Option<bool>) -> Result<String, anyhow::Error> {
    use bollard::Docker;
    use bollard::container::LogsOptions;
    use futures::StreamExt;
    
    let docker = Docker::connect_with_local_defaults()?;
    
    let options = LogsOptions {
        stdout: true,
        stderr: true,
        follow: follow.unwrap_or(false),
        tail: tail.unwrap_or_else(|| "all".to_string()),
        ..Default::default()
    };
    
    let mut stream = docker.logs(container_name, Some(options));
    let mut logs = String::new();
    
    while let Some(result) = stream.next().await {
        match result {
            Ok(output) => {
                logs.push_str(&output.to_string());
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Error reading logs: {}", e));
            }
        }
    }
    
    Ok(logs)
}

#[derive(Deserialize)]
pub struct LogParams {
    container: Option<String>,
    follow: Option<bool>,
    tail: Option<String>,
    timestamps: Option<bool>,
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
// Port-forward handlers for kubectl port-forward support
pub async fn pod_portforward_get(
    Path((namespace, name)): Path<(String, String)>,
) -> impl IntoResponse {
    // Return 101 Switching Protocols for WebSocket upgrade
    // kubectl expects this for port-forward protocol negotiation
    StatusCode::SWITCHING_PROTOCOLS
}

pub async fn pod_portforward_post(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> StatusCode {
    // Check if pod exists
    match state.storage.pods().get(&namespace, &name).await {
        Ok(_) => {
            // For now, return 501 Not Implemented as port-forwarding 
            // requires complex WebSocket/SPDY protocol handling
            StatusCode::NOT_IMPLEMENTED
        }
        Err(_) => StatusCode::NOT_FOUND,
    }
}
