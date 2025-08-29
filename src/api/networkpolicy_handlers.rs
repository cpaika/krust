use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use tracing::{error, info};

use crate::api::server::AppState;

pub async fn create_networkpolicy(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut policy): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate kind
    if policy.get("kind").and_then(|k| k.as_str()) != Some("NetworkPolicy") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ensure metadata exists
    if !policy.get("metadata").is_some() {
        policy["metadata"] = json!({});
    }

    info!(
        "Creating NetworkPolicy {} in namespace {}",
        policy["metadata"]["name"].as_str().unwrap_or("unknown"),
        namespace
    );

    let store = state.storage.networkpolicies();
    match store.create(&namespace, policy).await {
        Ok(created_policy) => Ok((StatusCode::CREATED, Json(created_policy))),
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                error!("NetworkPolicy already exists: {}", e);
                return Ok((StatusCode::CONFLICT, Json(json!({
                    "kind": "Status",
                    "apiVersion": "v1",
                    "metadata": {},
                    "status": "Failure",
                    "message": "NetworkPolicy already exists",
                    "reason": "AlreadyExists",
                    "code": 409
                }))));
            }
            error!("Failed to create NetworkPolicy: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_networkpolicy(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting NetworkPolicy {} in namespace {}", name, namespace);
    
    let store = state.storage.networkpolicies();
    match store.get(&namespace, &name).await {
        Ok(policy) => Ok(Json(policy)),
        Err(e) if e.to_string().contains("not found") => {
            error!("NetworkPolicy {}/{} not found", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to get NetworkPolicy: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_networkpolicies_namespaced(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("Listing NetworkPolicies in namespace {}", namespace);
    
    let store = state.storage.networkpolicies();
    match store.list(Some(&namespace)).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list NetworkPolicies: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_networkpolicies_all_namespaces(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    info!("Listing NetworkPolicies in all namespaces");
    
    let store = state.storage.networkpolicies();
    match store.list(None).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list NetworkPolicies: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_networkpolicy(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut policy): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("Updating NetworkPolicy {} in namespace {}", name, namespace);
    
    // Validate kind
    if policy.get("kind").and_then(|k| k.as_str()) != Some("NetworkPolicy") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ensure metadata exists with correct name and namespace
    if !policy.get("metadata").is_some() {
        policy["metadata"] = json!({});
    }
    policy["metadata"]["name"] = json!(name);
    policy["metadata"]["namespace"] = json!(namespace);
    
    let store = state.storage.networkpolicies();
    
    // First check if the resource exists
    match store.get(&namespace, &name).await {
        Ok(_) => {
            // Resource exists, proceed with update
            // NetworkPolicy store doesn't have an update method, so we need to delete and recreate
            match store.delete(&namespace, &name).await {
                Ok(_) => {
                    match store.create(&namespace, policy).await {
                        Ok(updated_policy) => Ok(Json(updated_policy)),
                        Err(e) => {
                            error!("Failed to update NetworkPolicy: {}", e);
                            Err(StatusCode::INTERNAL_SERVER_ERROR)
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to delete NetworkPolicy for update: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Err(e) if e.to_string().contains("not found") => {
            error!("NetworkPolicy {}/{} not found for update", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to check NetworkPolicy existence: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn patch_networkpolicy(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("Patching NetworkPolicy {} in namespace {}", name, namespace);
    
    let store = state.storage.networkpolicies();
    
    // Get existing resource
    match store.get(&namespace, &name).await {
        Ok(mut existing) => {
            // Simple merge patch - merge the patch into existing
            merge_json(&mut existing, &patch);
            
            // Delete and recreate with merged data
            match store.delete(&namespace, &name).await {
                Ok(_) => {
                    match store.create(&namespace, existing).await {
                        Ok(patched_policy) => Ok(Json(patched_policy)),
                        Err(e) => {
                            error!("Failed to patch NetworkPolicy: {}", e);
                            Err(StatusCode::INTERNAL_SERVER_ERROR)
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to delete NetworkPolicy for patch: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Err(e) if e.to_string().contains("not found") => {
            error!("NetworkPolicy {}/{} not found for patch", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to get NetworkPolicy for patch: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_networkpolicy(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Deleting NetworkPolicy {} in namespace {}", name, namespace);
    
    let store = state.storage.networkpolicies();
    match store.delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) if e.to_string().contains("not found") => {
            error!("NetworkPolicy {}/{} not found", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to delete NetworkPolicy: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Helper function to merge JSON objects
fn merge_json(target: &mut Value, patch: &Value) {
    if let (Some(target_obj), Some(patch_obj)) = (target.as_object_mut(), patch.as_object()) {
        for (key, value) in patch_obj {
            if let Some(target_value) = target_obj.get_mut(key) {
                if target_value.is_object() && value.is_object() {
                    merge_json(target_value, value);
                } else {
                    *target_value = value.clone();
                }
            } else {
                target_obj.insert(key.clone(), value.clone());
            }
        }
    }
}