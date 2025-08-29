use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Json, IntoResponse},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx;
use uuid::Uuid;

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
    // Query namespaces from database
    let result = sqlx::query_as::<_, (String, String, String, i64, Option<String>, Option<String>, Option<String>, Option<String>)>(
        "SELECT uid, name, creation_timestamp, resource_version, labels, annotations, spec, status FROM namespaces WHERE deletion_timestamp IS NULL"
    )
    .fetch_all(state.storage.pool())
    .await;
    
    let items = match result {
        Ok(rows) => {
            tracing::info!("Found {} namespaces in database", rows.len());
            rows.into_iter().map(|(uid, name, creation_timestamp, resource_version, labels, annotations, spec, status)| {
                let mut ns = json!({
                    "apiVersion": "v1",
                    "kind": "Namespace",
                    "metadata": {
                        "uid": uid,
                        "name": name,
                        "resourceVersion": resource_version.to_string(),
                        "creationTimestamp": creation_timestamp
                    }
                });
                
                if let Some(labels_str) = labels {
                    if let Ok(labels_val) = serde_json::from_str::<Value>(&labels_str) {
                        ns["metadata"]["labels"] = labels_val;
                    }
                }
                
                if let Some(annotations_str) = annotations {
                    if let Ok(annotations_val) = serde_json::from_str::<Value>(&annotations_str) {
                        ns["metadata"]["annotations"] = annotations_val;
                    }
                }
                
                if let Some(spec_str) = spec {
                    if let Ok(spec_val) = serde_json::from_str::<Value>(&spec_str) {
                        ns["spec"] = spec_val;
                    }
                }
                
                if let Some(status_str) = status {
                    if let Ok(status_val) = serde_json::from_str::<Value>(&status_str) {
                        ns["status"] = status_val;
                    }
                }
                
                ns
            }).collect::<Vec<_>>()
        }
        Err(e) => {
            tracing::error!("Failed to list namespaces: {}", e);
            vec![]
        }
    };
    
    Ok(Json(json!({
        "apiVersion": "v1",
        "kind": "NamespaceList",
        "metadata": {
            "resourceVersion": "1"
        },
        "items": items
    })))
}

pub async fn create_namespace(
    State(state): State<AppState>,
    Json(mut namespace): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate API version and kind
    let api_version = namespace.get("apiVersion")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            tracing::warn!("Missing apiVersion in namespace creation request");
            StatusCode::BAD_REQUEST
        })?;
    
    if api_version != "v1" {
        // Auto-correct wrong API version for namespaces
        tracing::info!("Correcting API version from {} to v1", api_version);
        namespace["apiVersion"] = json!("v1");
    }
    
    let kind = namespace.get("kind")
        .and_then(|k| k.as_str())
        .ok_or_else(|| {
            tracing::warn!("Missing kind in namespace creation request");
            StatusCode::BAD_REQUEST
        })?;
    
    if kind != "Namespace" {
        tracing::warn!("Wrong kind '{}' for namespace endpoint", kind);
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Validate namespace name
    let name = namespace.get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    // Kubernetes name validation:
    // - Must be non-empty
    // - Must be 253 characters or less
    // - Must consist of lowercase alphanumeric characters or '-'
    // - Must start and end with an alphanumeric character
    if name.is_empty() {
        tracing::warn!("Namespace name cannot be empty");
        return Err(StatusCode::BAD_REQUEST);
    }
    
    if name.len() > 253 {
        tracing::warn!("Namespace name {} is too long (max 253 characters)", name);
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Check for valid characters and format
    let valid_chars = name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    let starts_ends_valid = name.chars().next().map(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).unwrap_or(false)
        && name.chars().last().map(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).unwrap_or(false);
    
    if !valid_chars || !starts_ends_valid {
        tracing::warn!("Namespace name {} contains invalid characters or format", name);
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Add required metadata if missing
    if namespace.get("metadata").and_then(|m| m.get("uid")).is_none() {
        let uid = Uuid::new_v4().to_string();
        namespace["metadata"]["uid"] = json!(uid);
    }
    
    if namespace.get("metadata").and_then(|m| m.get("resourceVersion")).is_none() {
        namespace["metadata"]["resourceVersion"] = json!("1");
    }
    
    if namespace.get("metadata").and_then(|m| m.get("creationTimestamp")).is_none() {
        namespace["metadata"]["creationTimestamp"] = json!(Utc::now().to_rfc3339());
    }
    
    // Save namespace directly to database
    let name = namespace.get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    let uid = namespace.get("metadata")
        .and_then(|m| m.get("uid"))
        .and_then(|u| u.as_str())
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let labels = namespace.get("metadata")
        .and_then(|m| m.get("labels"))
        .map(|l| l.to_string())
        .unwrap_or_else(|| "{}".to_string());
    
    let annotations = namespace.get("metadata")
        .and_then(|m| m.get("annotations"))
        .map(|a| a.to_string())
        .unwrap_or_else(|| "{}".to_string());
    
    let spec = namespace.get("spec")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "{}".to_string());
    
    // Insert directly into namespaces table
    match sqlx::query(
        "INSERT INTO namespaces (uid, name, resource_version, creation_timestamp, labels, annotations, spec) 
         VALUES (?, ?, 1, CURRENT_TIMESTAMP, ?, ?, ?)"
    )
    .bind(uid)
    .bind(name)
    .bind(&labels)
    .bind(&annotations)
    .bind(&spec)
    .execute(state.storage.pool())
    .await {
        Ok(result) => {
            tracing::info!("Created namespace {} with {} rows affected", name, result.rows_affected());
            Ok((StatusCode::CREATED, Json(namespace)))
        },
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint") {
                tracing::warn!("Namespace {} already exists", name);
                Err(StatusCode::CONFLICT)
            } else {
                tracing::error!("Failed to create namespace {}: {:?}", name, e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn get_namespace(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let result = sqlx::query_as::<_, (String, String, String, i64, Option<String>, Option<String>, Option<String>, Option<String>)>(
        "SELECT uid, name, creation_timestamp, resource_version, labels, annotations, spec, status FROM namespaces WHERE name = ? AND deletion_timestamp IS NULL"
    )
    .bind(&name)
    .fetch_one(state.storage.pool())
    .await;
    
    match result {
        Ok((uid, name, creation_timestamp, resource_version, labels, annotations, spec, status)) => {
            let mut ns = json!({
                "apiVersion": "v1",
                "kind": "Namespace",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "resourceVersion": resource_version.to_string(),
                    "creationTimestamp": creation_timestamp
                }
            });
            
            if let Some(labels_str) = labels {
                if let Ok(labels_val) = serde_json::from_str::<Value>(&labels_str) {
                    ns["metadata"]["labels"] = labels_val;
                }
            }
            
            if let Some(annotations_str) = annotations {
                if let Ok(annotations_val) = serde_json::from_str::<Value>(&annotations_str) {
                    ns["metadata"]["annotations"] = annotations_val;
                }
            }
            
            if let Some(spec_str) = spec {
                if let Ok(spec_val) = serde_json::from_str::<Value>(&spec_str) {
                    ns["spec"] = spec_val;
                }
            }
            
            if let Some(status_str) = status {
                if let Ok(status_val) = serde_json::from_str::<Value>(&status_str) {
                    ns["status"] = status_val;
                }
            }
            
            Ok(Json(ns))
        }
        Err(_) => Err(StatusCode::NOT_FOUND)
    }
}

pub async fn update_namespace(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(mut namespace): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Extract fields to update
    let labels = namespace.get("metadata")
        .and_then(|m| m.get("labels"))
        .map(|l| l.to_string())
        .unwrap_or_else(|| "{}".to_string());
    
    let annotations = namespace.get("metadata")
        .and_then(|m| m.get("annotations"))
        .map(|a| a.to_string())
        .unwrap_or_else(|| "{}".to_string());
    
    let spec = namespace.get("spec")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "{}".to_string());
    
    let status = namespace.get("status")
        .map(|s| s.to_string());
    
    // Update namespace
    let result = if let Some(status_str) = status {
        sqlx::query(
            "UPDATE namespaces SET labels = ?, annotations = ?, spec = ?, status = ?, resource_version = resource_version + 1 WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(&labels)
        .bind(&annotations)
        .bind(&spec)
        .bind(&status_str)
        .bind(&name)
        .execute(state.storage.pool())
        .await
    } else {
        sqlx::query(
            "UPDATE namespaces SET labels = ?, annotations = ?, spec = ?, resource_version = resource_version + 1 WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(&labels)
        .bind(&annotations)
        .bind(&spec)
        .bind(&name)
        .execute(state.storage.pool())
        .await
    };
    
    match result {
        Ok(result) => {
            if result.rows_affected() > 0 {
                // Update resource version in response
                if let Some(metadata) = namespace.get_mut("metadata").and_then(|m| m.as_object_mut()) {
                    // Fetch the new resource version
                    let rv_result = sqlx::query_as::<_, (i64,)>(
                        "SELECT resource_version FROM namespaces WHERE name = ?"
                    )
                    .bind(&name)
                    .fetch_one(state.storage.pool())
                    .await;
                    
                    if let Ok((new_rv,)) = rv_result {
                        metadata.insert("resourceVersion".to_string(), json!(new_rv.to_string()));
                    }
                }
                Ok(Json(namespace))
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        }
        Err(e) => {
            tracing::error!("Failed to update namespace {}: {}", name, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
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
    // Mark namespace as deleted
    match sqlx::query(
        "UPDATE namespaces SET deletion_timestamp = CURRENT_TIMESTAMP WHERE name = ?"
    )
    .bind(&name)
    .execute(state.storage.pool())
    .await {
        Ok(result) => {
            if result.rows_affected() > 0 {
                tracing::info!("Deleted namespace {}", name);
                StatusCode::OK
            } else {
                tracing::warn!("Namespace {} not found for deletion", name);
                StatusCode::NOT_FOUND
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete namespace {}: {}", name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
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
    Json(mut pod): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate API version and kind
    let api_version = pod.get("apiVersion")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            tracing::warn!("Missing apiVersion in pod creation request");
            StatusCode::BAD_REQUEST
        })?;
    
    // Pods should use v1, not apps/v1 (that's for Deployments, etc.)
    if api_version != "v1" {
        tracing::info!("Correcting Pod API version from {} to v1", api_version);
        pod["apiVersion"] = json!("v1");
    }
    
    let kind = pod.get("kind")
        .and_then(|k| k.as_str())
        .ok_or_else(|| {
            tracing::warn!("Missing kind in pod creation request");
            StatusCode::BAD_REQUEST
        })?;
    
    if kind != "Pod" {
        tracing::warn!("Wrong kind '{}' for pod endpoint", kind);
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Validate pod has at least one container
    let containers = pod.get("spec")
        .and_then(|s| s.get("containers"))
        .and_then(|c| c.as_array());
    
    if containers.map(|c| c.is_empty()).unwrap_or(true) {
        tracing::warn!("Pod must have at least one container");
        return Err(StatusCode::BAD_REQUEST);
    }
    
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
    Json(mut pod): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Get the existing pod to enforce immutability
    let existing_pod = match state.storage.pods().get(&namespace, &name).await {
        Ok(pod) => pod,
        Err(e) if e.to_string().contains("not found") => {
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            tracing::error!("Failed to get pod for update: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    
    // Enforce pod spec immutability - most spec fields cannot be changed after creation
    // Only certain fields like activeDeadlineSeconds, tolerations, and terminationGracePeriodSeconds can be updated
    
    // Preserve immutable spec fields from the existing pod
    if let (Some(existing_spec), Some(new_spec)) = (existing_pod.get("spec"), pod.get_mut("spec")) {
        // Containers are immutable (except for image in some cases, but we'll be strict)
        if existing_spec.get("containers").is_some() {
            new_spec["containers"] = existing_spec["containers"].clone();
        }
        
        // These fields are immutable and must be preserved
        let immutable_fields = [
            "initContainers",
            "hostNetwork", 
            "hostPID",
            "hostIPC",
            "serviceAccountName",
            "serviceAccount",
            "nodeName",  // Can only be set via binding
            "restartPolicy",
            "schedulerName",
            "securityContext",
            "volumes",
            "dnsPolicy",
            "nodeSelector",
            "affinity",
        ];
        
        for field in &immutable_fields {
            if existing_spec.get(field).is_some() {
                new_spec[field] = existing_spec[field].clone();
            }
        }
    }
    
    // Allow metadata updates (labels, annotations, finalizers)
    // but preserve immutable metadata fields
    if let Some(existing_meta) = existing_pod.get("metadata") {
        if let Some(new_meta) = pod.get_mut("metadata") {
            // Preserve immutable metadata fields
            new_meta["uid"] = existing_meta["uid"].clone();
            new_meta["name"] = existing_meta["name"].clone();
            new_meta["namespace"] = existing_meta["namespace"].clone();
            new_meta["creationTimestamp"] = existing_meta["creationTimestamp"].clone();
            
            // Preserve resourceVersion if not provided
            if new_meta.get("resourceVersion").is_none() {
                new_meta["resourceVersion"] = existing_meta["resourceVersion"].clone();
            }
        }
    }
    
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
    Json(mut service): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate API version and kind
    let api_version = service.get("apiVersion")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            tracing::warn!("Missing apiVersion in service creation request");
            StatusCode::BAD_REQUEST
        })?;
    
    if api_version != "v1" {
        tracing::info!("Correcting Service API version from {} to v1", api_version);
        service["apiVersion"] = json!("v1");
    }
    
    let kind = service.get("kind")
        .and_then(|k| k.as_str())
        .ok_or_else(|| {
            tracing::warn!("Missing kind in service creation request");
            StatusCode::BAD_REQUEST
        })?;
    
    if kind != "Service" {
        tracing::warn!("Wrong kind '{}' for service endpoint", kind);
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Validate service ports
    if let Some(spec) = service.get_mut("spec") {
        if let Some(ports) = spec.get_mut("ports").and_then(|p| p.as_array_mut()) {
            for port in ports.iter_mut() {
                // Validate port number (1-65535)
                if let Some(port_num) = port.get("port").and_then(|p| p.as_i64()) {
                    if port_num <= 0 || port_num > 65535 {
                        tracing::warn!("Invalid port number: {}", port_num);
                        if port_num > 65535 {
                            // Clamp to max valid port
                            port["port"] = json!(65535);
                            tracing::info!("Clamped port {} to 65535", port_num);
                        } else {
                            return Err(StatusCode::BAD_REQUEST);
                        }
                    }
                }
                
                // Validate targetPort if it's a number
                if let Some(target_port) = port.get("targetPort").and_then(|p| p.as_i64()) {
                    if target_port <= 0 || target_port > 65535 {
                        tracing::warn!("Invalid targetPort number: {}", target_port);
                        return Err(StatusCode::BAD_REQUEST);
                    }
                } else if let Some(target_port_str) = port.get("targetPort").and_then(|p| p.as_str()) {
                    // Validate targetPort name format (IANA_SVC_NAME)
                    // Must be lowercase alphanumeric or '-', max 15 chars
                    if target_port_str.len() > 15 || 
                       !target_port_str.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') ||
                       target_port_str.starts_with('-') || target_port_str.ends_with('-') {
                        tracing::warn!("Invalid targetPort name format: {}", target_port_str);
                        // We'll be lenient and allow it, but log the warning
                    }
                }
                
                // Set default protocol if not specified
                if port.get("protocol").is_none() {
                    port["protocol"] = json!("TCP");
                }
            }
        }
    }
    
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

pub async fn create_endpoints(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(endpoints): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.endpoints().create(&namespace, endpoints).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create endpoints: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_endpoints(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(endpoints): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.endpoints().update(&namespace, &name, endpoints).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to update endpoints: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_endpoints(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.endpoints().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to delete endpoints: {}", e);
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
        Ok(deleted) => Ok(Json(deleted)),
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

pub async fn get_deployment_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.deployments().get(&namespace, &name).await {
        Ok(deployment) => Ok(Json(deployment)),
        Err(e) => {
            tracing::error!("Failed to get deployment status: {}", e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

pub async fn update_deployment_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(status_update): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Get the current deployment
    let mut deployment = match state.storage.deployments().get(&namespace, &name).await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Failed to get deployment: {}", e);
            return Err(StatusCode::NOT_FOUND);
        }
    };
    
    // Update the status
    deployment["status"] = status_update["status"].clone();
    
    // Update in storage
    match state.storage.deployments().update(&namespace, &name, deployment).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to update deployment status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_replicaset(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(replicaset): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.replicasets().create(&namespace, replicaset).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create replicaset: {}", e);
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

pub async fn update_replicaset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(replicaset): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.replicasets().update(&namespace, &name, replicaset).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to update replicaset: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn patch_replicaset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.replicasets().patch(&namespace, &name, patch).await {
        Ok(patched) => Ok(Json(patched)),
        Err(e) => {
            tracing::error!("Failed to patch replicaset: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_replicaset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.replicasets().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to delete replicaset: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn get_replicaset_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.replicasets().get_scale(&namespace, &name).await {
        Ok(scale) => Ok(Json(scale)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get replicaset scale: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn update_replicaset_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(scale): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let replicas = scale["spec"]["replicas"].as_i64().unwrap_or(1);
    
    match state.storage.replicasets().update_scale(&namespace, &name, replicas).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to update replicaset scale: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_replicaset_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(status_update): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let status = if let Some(s) = status_update.get("status") {
        s.clone()
    } else {
        status_update
    };
    
    match state.storage.replicasets().update_status(&namespace, &name, status).await {
        Ok(_) => {
            // Return the updated ReplicaSet
            match state.storage.replicasets().get(&namespace, &name).await {
                Ok(rs) => Ok(Json(rs)),
                Err(e) => {
                    tracing::error!("Failed to get updated replicaset: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to update replicaset status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Pod subresources
pub async fn get_pod_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.pods().get_status(&namespace, &name).await {
        Ok(pod) => Ok(Json(pod)),
        Err(e) if e.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get pod status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_pod_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(status_update): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let status = if let Some(s) = status_update.get("status") {
        s.clone()
    } else {
        status_update
    };
    
    match state.storage.pods().set_status(&namespace, &name, status).await {
        Ok(pod) => Ok(Json(pod)),
        Err(e) if e.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to update pod status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn patch_pod_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // For status patch, we expect the patch to contain a "status" field
    let status = if let Some(s) = patch.get("status") {
        s.clone()
    } else {
        patch
    };
    
    match state.storage.pods().set_status(&namespace, &name, status).await {
        Ok(pod) => Ok(Json(pod)),
        Err(e) if e.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to patch pod status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_pod_ephemeralcontainers(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(update): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let ephemeral_containers = update["spec"]["ephemeralContainers"].clone();
    
    if ephemeral_containers.is_null() {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    match state.storage.pods().update_ephemeral_containers(&namespace, &name, ephemeral_containers).await {
        Ok(pod) => Ok(Json(pod)),
        Err(e) if e.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to update ephemeral containers: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_pod_binding(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(binding): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let node_name = binding["target"]["name"]
        .as_str()
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    match state.storage.pods().bind_to_node(&namespace, &name, node_name).await {
        Ok(_) => Ok((StatusCode::CREATED, Json(json!({
            "apiVersion": "v1",
            "kind": "Binding",
            "metadata": {
                "name": name,
                "namespace": namespace
            },
            "target": {
                "kind": "Node",
                "name": node_name
            }
        })))),
        Err(e) if e.to_string().contains("not found") => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to bind pod: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn pod_exec(
    State(_state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(_params): Query<ExecParams>,
) -> Result<Json<Value>, StatusCode> {
    // WebSocket upgrade is required for exec functionality
    // Return 501 Not Implemented for now
    tracing::info!("Pod exec requested for {}/{} - WebSocket support not yet implemented", namespace, name);
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn pod_attach(
    State(_state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(_params): Query<AttachParams>,
) -> Result<Json<Value>, StatusCode> {
    // WebSocket upgrade is required for attach functionality
    // Return 501 Not Implemented for now
    tracing::info!("Pod attach requested for {}/{} - WebSocket support not yet implemented", namespace, name);
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn pod_portforward(
    State(_state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    // SPDY/WebSocket upgrade is required for port forwarding
    // Return 501 Not Implemented for now
    tracing::info!("Pod portforward requested for {}/{} - SPDY/WebSocket support not yet implemented", namespace, name);
    Err(StatusCode::NOT_IMPLEMENTED)
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
            // If container doesn't exist yet, return empty logs or 404
            if e.to_string().contains("No such container") || e.to_string().contains("404") {
                // Check pod phase - if pending/creating, return empty logs
                if let Some(phase) = pod["status"]["phase"].as_str() {
                    if phase == "Pending" || phase == "ContainerCreating" {
                        return Ok(String::new()); // Return empty logs for pending pods
                    }
                }
                tracing::warn!("Container {} not found", full_container_name);
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get logs for container {}: {}", full_container_name, e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
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

#[derive(Deserialize)]
pub struct ExecParams {
    container: Option<String>,
    command: Vec<String>,
    stdin: Option<bool>,
    stdout: Option<bool>,
    stderr: Option<bool>,
    tty: Option<bool>,
}

#[derive(Deserialize)]
pub struct AttachParams {
    container: Option<String>,
    stdin: Option<bool>,
    stdout: Option<bool>,
    stderr: Option<bool>,
    tty: Option<bool>,
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

// HorizontalPodAutoscaler handlers
pub async fn list_all_hpas(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.hpas().list(None).await {
        Ok(hpas) => Ok(Json(hpas)),
        Err(e) => {
            tracing::error!("Failed to list HPAs: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_hpas(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.hpas().list(Some(&namespace)).await {
        Ok(hpas) => Ok(Json(hpas)),
        Err(e) => {
            tracing::error!("Failed to list HPAs in namespace {}: {}", namespace, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_hpa(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(hpa): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.hpas().create(&namespace, hpa).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create HPA: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_hpa(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.hpas().get(&namespace, &name).await {
        Ok(hpa) => Ok(Json(hpa)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to get HPA: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn update_hpa(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(hpa): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.hpas().update(&namespace, &name, hpa).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to update HPA: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_hpa(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.hpas().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to delete HPA: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn get_hpa_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.hpas().get(&namespace, &name).await {
        Ok(hpa) => Ok(Json(hpa)),
        Err(e) => {
            tracing::error!("Failed to get HPA status: {}", e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

pub async fn update_hpa_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(status_update): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.hpas().update_status(&namespace, &name, status_update["status"].clone()).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                tracing::error!("Failed to update HPA status: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}
