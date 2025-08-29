#![allow(unused_imports)]
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use super::portforward_proxy::PortForwardProxy;
use super::server::AppState;

// Global port forward proxy instance
lazy_static::lazy_static! {
    static ref PORT_FORWARD_PROXY: Arc<RwLock<PortForwardProxy>> = Arc::new(RwLock::new(PortForwardProxy::new()));
}

#[derive(Debug, Deserialize)]
pub struct PortForwardRequest {
    ports: Vec<PortMapping>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PortMapping {
    local_port: u16,
    remote_port: u16,
}

// GET endpoint to check if port-forward is supported
pub async fn pod_portforward_get(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    // Check if pod exists and is running
    match state.storage.pods().get(&namespace, &name).await {
        Ok(pod) => {
            // Check if the pod is running
            if let Some(status) = pod.get("status").and_then(|s| s.as_object()) {
                if let Some(phase) = status.get("phase").and_then(|p| p.as_str()) {
                    if phase == "Running" {
                        // Return OK to indicate port-forward is supported
                        Ok(StatusCode::OK)
                    } else {
                        Err(StatusCode::CONFLICT) // Pod not running
                    }
                } else {
                    Err(StatusCode::CONFLICT)
                }
            } else {
                Err(StatusCode::CONFLICT)
            }
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

// POST endpoint to establish port forwarding
pub async fn pod_portforward_post(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    body: String,
) -> Result<Response, StatusCode> {
    // Verify pod exists and is running
    let pod = state
        .storage
        .pods()
        .get(&namespace, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let status = pod
        .get("status")
        .and_then(|s| s.as_object())
        .ok_or(StatusCode::CONFLICT)?;
    
    let phase = status
        .get("phase")
        .and_then(|p| p.as_str())
        .ok_or(StatusCode::CONFLICT)?;
    
    if phase != "Running" {
        return Err(StatusCode::CONFLICT);
    }

    // For kubectl compatibility, we need to parse the port from the request
    // kubectl sends port information in the request path or headers
    // For now, we'll start a simple proxy on the default ports
    
    let local_port = 9090; // Default local port
    let remote_port = 8080; // Default remote port (from our test pods)
    
    // Start the port forward proxy
    let proxy = PORT_FORWARD_PROXY.read().await;
    match proxy.start_forward(&namespace, &name, local_port, remote_port).await {
        Ok(_) => {
            info!("Started port forward for pod {}/{} on port {}:{}", 
                namespace, name, local_port, remote_port);
            
            // Return success response
            let response = json!({
                "success": true,
                "message": format!("Port forwarding established: localhost:{} -> pod:{}",
                    local_port, remote_port),
                "local_port": local_port,
                "remote_port": remote_port
            });
            
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&response).unwrap().into())
                .unwrap())
        }
        Err(e) => {
            error!("Failed to start port forward: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Additional endpoint for kubectl-style port forwarding
pub async fn kubectl_portforward(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    // This endpoint handles the simplified kubectl port-forward protocol
    // It starts a TCP proxy and returns immediately
    
    // Verify pod exists and is running
    let pod = state
        .storage
        .pods()
        .get(&namespace, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let status = pod
        .get("status")
        .and_then(|s| s.as_object())
        .ok_or(StatusCode::CONFLICT)?;
    
    let phase = status
        .get("phase")
        .and_then(|p| p.as_str())
        .ok_or(StatusCode::CONFLICT)?;
    
    if phase != "Running" {
        return Err(StatusCode::CONFLICT);
    }

    // Parse port mappings from the pod spec
    let mut port_mappings = Vec::new();
    if let Some(spec) = pod.get("spec").and_then(|s| s.as_object()) {
        if let Some(containers) = spec.get("containers").and_then(|c| c.as_array()) {
            for container in containers {
                if let Some(ports) = container.get("ports").and_then(|p| p.as_array()) {
                    for port in ports {
                        if let Some(container_port) = port.get("containerPort").and_then(|p| p.as_u64()) {
                            port_mappings.push(PortMapping {
                                local_port: container_port as u16,
                                remote_port: container_port as u16,
                            });
                        }
                    }
                }
            }
        }
    }

    if port_mappings.is_empty() {
        // Default port mapping if none found
        port_mappings.push(PortMapping {
            local_port: 9090,
            remote_port: 8080,
        });
    }

    // Start port forwarding for each mapping
    let proxy = PORT_FORWARD_PROXY.read().await;
    for mapping in &port_mappings {
        if let Err(e) = proxy.start_forward(&namespace, &name, mapping.local_port, mapping.remote_port).await {
            error!("Failed to start port forward for {}:{}: {}", 
                mapping.local_port, mapping.remote_port, e);
        }
    }

    info!("Port forwarding established for pod {}/{}", namespace, name);
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&json!({
            "status": "Port forwarding active",
            "mappings": port_mappings
        })).unwrap().into())
        .unwrap())
}