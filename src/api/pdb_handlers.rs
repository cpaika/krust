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

// List all PodDisruptionBudgets across namespaces
pub async fn list_all_pdbs(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.pdbs().list(None).await {
        Ok(pdbs) => Ok(Json(pdbs)),
        Err(e) => {
            tracing::error!("Failed to list all pod disruption budgets: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// List PodDisruptionBudgets in a namespace
pub async fn list_pdbs(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.pdbs().list(Some(&namespace)).await {
        Ok(pdbs) => Ok(Json(pdbs)),
        Err(e) => {
            tracing::error!("Failed to list pod disruption budgets: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_pdb(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(pdb): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    match state.storage.pdbs().create(&namespace, pdb).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(e) => {
            tracing::error!("Failed to create pod disruption budget: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_pdb(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.pdbs().get(&namespace, &name).await {
        Ok(Some(pdb)) => Ok(Json(pdb)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get pod disruption budget: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_pdb(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(pdb): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.pdbs().update(&namespace, &name, pdb).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to update pod disruption budget: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn patch_pdb(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // Get current PDB
    let current = match state.storage.pdbs().get(&namespace, &name).await {
        Ok(Some(pdb)) => pdb,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get pod disruption budget for patch: {}", e);
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

    // Update the PDB
    match state.storage.pdbs().update(&namespace, &name, patched).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to patch pod disruption budget: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_pdb(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.pdbs().delete(&namespace, &name).await {
        Ok(deleted) => Ok(Json(deleted)),
        Err(e) => {
            tracing::error!("Failed to delete pod disruption budget: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Get PDB status
pub async fn get_pdb_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    match state.storage.pdbs().get(&namespace, &name).await {
        Ok(Some(pdb)) => Ok(Json(pdb)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get pod disruption budget status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Update PDB status
pub async fn update_pdb_status(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(status_update): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let status = status_update["status"].clone();
    match state.storage.pdbs().update_status(&namespace, &name, status).await {
        Ok(updated) => Ok(Json(updated)),
        Err(e) => {
            tracing::error!("Failed to update pod disruption budget status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}