use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use tracing::{error, info};

use crate::api::server::AppState;

pub async fn create_ingress(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut ingress): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate kind
    if ingress.get("kind").and_then(|k| k.as_str()) != Some("Ingress") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ensure metadata exists
    if !ingress.get("metadata").is_some() {
        ingress["metadata"] = json!({});
    }

    info!(
        "Creating Ingress {} in namespace {}",
        ingress["metadata"]["name"].as_str().unwrap_or("unknown"),
        namespace
    );

    let store = state.storage.ingresses();
    match store.create(&namespace, ingress).await {
        Ok(created_ingress) => Ok((StatusCode::CREATED, Json(created_ingress))),
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                error!("Ingress already exists: {}", e);
                return Ok((StatusCode::CONFLICT, Json(json!({
                    "kind": "Status",
                    "apiVersion": "v1",
                    "metadata": {},
                    "status": "Failure",
                    "message": "Ingress already exists",
                    "reason": "AlreadyExists",
                    "code": 409
                }))));
            }
            error!("Failed to create Ingress: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_ingress(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting Ingress {} in namespace {}", name, namespace);
    
    let store = state.storage.ingresses();
    match store.get(&namespace, &name).await {
        Ok(ingress) => Ok(Json(ingress)),
        Err(e) if e.to_string().contains("not found") => {
            error!("Ingress {}/{} not found", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to get Ingress: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_ingresses_namespaced(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("Listing Ingresses in namespace {}", namespace);
    
    let store = state.storage.ingresses();
    match store.list(Some(&namespace)).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list Ingresses: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_ingresses_all_namespaces(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    info!("Listing Ingresses in all namespaces");
    
    let store = state.storage.ingresses();
    match store.list(None).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list Ingresses: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_ingress(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut ingress): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("Updating Ingress {} in namespace {}", name, namespace);
    
    // Validate kind
    if ingress.get("kind").and_then(|k| k.as_str()) != Some("Ingress") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ensure metadata exists with correct name and namespace
    if !ingress.get("metadata").is_some() {
        ingress["metadata"] = json!({});
    }
    ingress["metadata"]["name"] = json!(name);
    ingress["metadata"]["namespace"] = json!(namespace);
    
    let store = state.storage.ingresses();
    match store.update(&namespace, &name, ingress).await {
        Ok(updated_ingress) => Ok(Json(updated_ingress)),
        Err(e) if e.to_string().contains("not found") => {
            error!("Ingress {}/{} not found for update", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to update Ingress: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn patch_ingress(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("Patching Ingress {} in namespace {}", name, namespace);
    
    let store = state.storage.ingresses();
    
    // Get existing resource
    match store.get(&namespace, &name).await {
        Ok(mut existing) => {
            // Simple merge patch - merge the patch into existing
            merge_json(&mut existing, &patch);
            
            // Use the update method
            match store.update(&namespace, &name, existing).await {
                Ok(patched_ingress) => Ok(Json(patched_ingress)),
                Err(e) => {
                    error!("Failed to patch Ingress: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Err(e) if e.to_string().contains("not found") => {
            error!("Ingress {}/{} not found for patch", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to get Ingress for patch: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_ingress_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(status_update): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("Updating Ingress status {} in namespace {}", name, namespace);
    
    let store = state.storage.ingresses();
    
    // Get existing Ingress
    match store.get(&namespace, &name).await {
        Ok(mut existing) => {
            // Update the status field
            if let Some(status) = status_update.get("status") {
                existing["status"] = status.clone();
            } else {
                // If the entire body is the status
                existing["status"] = status_update;
            }
            
            // Use the update method to save the changes
            match store.update(&namespace, &name, existing).await {
                Ok(updated_ingress) => Ok(Json(updated_ingress)),
                Err(e) => {
                    error!("Failed to update Ingress status: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Err(e) if e.to_string().contains("not found") => {
            error!("Ingress {}/{} not found for status update", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to get Ingress for status update: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_ingress(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Deleting Ingress {} in namespace {}", name, namespace);
    
    let store = state.storage.ingresses();
    match store.delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) if e.to_string().contains("not found") => {
            error!("Ingress {}/{} not found", namespace, name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            error!("Failed to delete Ingress: {}", e);
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