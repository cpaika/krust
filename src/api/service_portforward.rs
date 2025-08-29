use axum::{
    extract::{ws::WebSocket, Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;
use tracing::{error, info, warn};

use super::portforward_spdy;
use super::server::AppState;

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    ports: Option<String>,
}

/// Handle port-forward requests to services
/// This finds the backing pods for a service and forwards to one of them
pub async fn service_portforward_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    info!("Service port-forward request for service {}/{}", namespace, name);
    
    // Get the service to find selector
    let service = state
        .storage
        .services()
        .get(&namespace, &name)
        .await
        .map_err(|e| {
            error!("Failed to get service: {}", e);
            StatusCode::NOT_FOUND
        })?;

    // Get selector from service spec
    let selector = service
        .get("spec")
        .and_then(|s| s.get("selector"))
        .and_then(|s| s.as_object())
        .ok_or_else(|| {
            warn!("Service has no selector");
            StatusCode::BAD_REQUEST
        })?;

    if selector.is_empty() {
        warn!("Service has empty selector");
        return Err(StatusCode::BAD_REQUEST);
    }

    // Find pods matching the selector
    let all_pods = state
        .storage
        .pods()
        .list(Some(&namespace))
        .await
        .map_err(|e| {
            error!("Failed to list pods: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Filter pods by selector labels
    let all_pods_array = all_pods.as_array()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let matching_pods: Vec<_> = all_pods_array
        .iter()
        .filter(|pod| {
            if let Some(labels) = pod.get("metadata")
                .and_then(|m| m.get("labels"))
                .and_then(|l| l.as_object())
            {
                // Check if all selector labels match
                selector.iter().all(|(key, value)| {
                    labels.get(key).map(|v| v == value).unwrap_or(false)
                })
            } else {
                false
            }
        })
        .filter(|pod| {
            // Only running pods
            pod.get("status")
                .and_then(|s| s.get("phase"))
                .and_then(|p| p.as_str())
                .map(|phase| phase == "Running")
                .unwrap_or(false)
        })
        .collect();

    if matching_pods.is_empty() {
        warn!("No running pods found for service {}/{}", namespace, name);
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // Pick the first matching pod
    let selected_pod = matching_pods[0];
    let pod_name = selected_pod
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    info!("Forwarding service {}/{} to pod {}/{}", namespace, name, namespace, pod_name);

    // Delegate to the pod port-forward handler
    portforward_spdy::portforward_handler(
        ws,
        State(state),
        Path((namespace.clone(), pod_name.to_string())),
        Query(portforward_spdy::PortForwardQuery { ports: query.ports }),
        headers,
    )
    .await
}