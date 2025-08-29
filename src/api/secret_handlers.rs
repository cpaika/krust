use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use tracing::{error, info, warn};

use crate::api::server::AppState;

pub async fn create_secret(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut secret): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // Validate API version and kind
    let api_version = secret.get("apiVersion")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            warn!("Missing apiVersion in secret creation request");
            StatusCode::BAD_REQUEST
        })?;
    
    if api_version != "v1" {
        info!("Correcting Secret API version from {} to v1", api_version);
        secret["apiVersion"] = json!("v1");
    }
    
    let kind = secret.get("kind")
        .and_then(|k| k.as_str())
        .ok_or_else(|| {
            warn!("Missing kind in secret creation request");
            StatusCode::BAD_REQUEST
        })?;
    
    if kind != "Secret" {
        warn!("Wrong kind '{}' for secret endpoint", kind);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Ensure metadata exists
    if !secret.get("metadata").is_some() {
        secret["metadata"] = json!({});
    }
    
    // Handle stringData conversion to data
    if let Some(string_data) = secret.get("stringData").and_then(|s| s.as_object()) {
        let mut data = secret.get("data")
            .and_then(|d| d.as_object())
            .cloned()
            .unwrap_or_else(|| serde_json::Map::new());
        
        for (key, value) in string_data {
            if let Some(str_val) = value.as_str() {
                // Convert string to base64
                let encoded = STANDARD.encode(str_val);
                data.insert(key.clone(), json!(encoded));
            }
        }
        
        secret["data"] = json!(data);
        // Remove stringData from the stored secret
        secret.as_object_mut().unwrap().remove("stringData");
    }
    
    // Get the secret name for logging after mutations are done
    let secret_name = secret["metadata"]["name"].as_str().unwrap_or("unknown");
    
    // Validate data field contains valid base64
    if let Some(data) = secret.get("data").and_then(|d| d.as_object()) {
        for (key, value) in data {
            if let Some(str_val) = value.as_str() {
                // Validate base64 encoding
                if STANDARD.decode(str_val).is_err() {
                    warn!("Invalid base64 in data field for key '{}' in secret '{}'", key, secret_name);
                    return Err(StatusCode::BAD_REQUEST);
                }
            } else {
                warn!("Non-string value in data field for key '{}' in secret '{}'", key, secret_name);
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }
    
    // Validate secret type and required fields
    let secret_type = secret.get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("Opaque");
    
    match secret_type {
        "kubernetes.io/tls" => {
            // TLS secrets must have tls.crt and tls.key
            let data = secret.get("data").and_then(|d| d.as_object());
            if let Some(data) = data {
                if !data.contains_key("tls.crt") || !data.contains_key("tls.key") {
                    warn!("TLS secret '{}' missing required keys (tls.crt, tls.key)", secret_name);
                    return Err(StatusCode::BAD_REQUEST);
                }
            } else {
                warn!("TLS secret '{}' has no data", secret_name);
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        "kubernetes.io/dockerconfigjson" => {
            // Docker config secrets must have .dockerconfigjson key
            let data = secret.get("data").and_then(|d| d.as_object());
            if let Some(data) = data {
                if !data.contains_key(".dockerconfigjson") {
                    warn!("Docker config secret '{}' missing required key (.dockerconfigjson)", secret_name);
                    return Err(StatusCode::BAD_REQUEST);
                }
            } else {
                warn!("Docker config secret '{}' has no data", secret_name);
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        "kubernetes.io/basic-auth" => {
            // Basic auth secrets should have username and/or password
            let data = secret.get("data").and_then(|d| d.as_object());
            if let Some(data) = data {
                if !data.contains_key("username") && !data.contains_key("password") {
                    warn!("Basic auth secret '{}' should have username or password", secret_name);
                    // We'll allow it but log warning
                }
            }
        }
        _ => {
            // Opaque or custom types - no specific validation
        }
    }
    
    // Check size limit (1MB for secrets in Kubernetes)
    let secret_json = serde_json::to_string(&secret).unwrap_or_default();
    if secret_json.len() > 1024 * 1024 {
        warn!("Secret '{}' exceeds 1MB size limit", secret_name);
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    info!("Creating Secret {} in namespace {}", secret_name, namespace);

    match state.storage.secrets().create(&namespace, secret).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                error!("Secret already exists: {}", e);
                Err(StatusCode::CONFLICT)
            } else {
                error!("Failed to create Secret: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn get_secret(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting Secret {} in namespace {}", name, namespace);

    match state.storage.secrets().get(&namespace, &name).await {
        Ok(secret) => Ok(Json(secret)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to get Secret: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn list_secrets(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("Listing Secrets in namespace {}", namespace);

    match state.storage.secrets().list(Some(&namespace)).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list Secrets: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_all_secrets(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    info!("Listing all Secrets");

    match state.storage.secrets().list(None).await {
        Ok(list) => Ok(Json(list)),
        Err(e) => {
            error!("Failed to list all Secrets: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_secret(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut secret): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Validate API version and kind
    let api_version = secret.get("apiVersion")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            warn!("Missing apiVersion in secret update request");
            StatusCode::BAD_REQUEST
        })?;
    
    if api_version != "v1" {
        secret["apiVersion"] = json!("v1");
    }
    
    if secret.get("kind").and_then(|k| k.as_str()) != Some("Secret") {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Check if secret is immutable
    let existing_secret = match state.storage.secrets().get(&namespace, &name).await {
        Ok(secret) => secret,
        Err(e) if e.to_string().contains("not found") => {
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            error!("Failed to get secret for update: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    
    // If the existing secret is immutable, reject the update
    if existing_secret.get("immutable").and_then(|i| i.as_bool()).unwrap_or(false) {
        warn!("Attempt to update immutable secret '{}'", name);
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    info!("Updating Secret {} in namespace {}", name, namespace);

    match state.storage.secrets().update(&namespace, &name, secret).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else if e.to_string().contains("immutable") {
                Err(StatusCode::UNPROCESSABLE_ENTITY)
            } else {
                error!("Failed to update Secret: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn patch_secret(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("Patching Secret {} in namespace {}", name, namespace);

    match state.storage.secrets().patch(&namespace, &name, patch).await {
        Ok(patched) => Ok(Json(patched)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else if e.to_string().contains("immutable") {
                Err(StatusCode::UNPROCESSABLE_ENTITY)
            } else {
                error!("Failed to patch Secret: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub async fn delete_secret(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    info!("Deleting Secret {} in namespace {}", name, namespace);

    match state.storage.secrets().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to delete Secret: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}