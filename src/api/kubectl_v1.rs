// Simple v1 protocol implementation for kubectl port-forward
// This handles the base "portforward.k8s.io" protocol (not SPDY)
use axum::{
    extract::{ws::WebSocket, Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, trace, warn};

use super::server::AppState;
use crate::runtime::container::ContainerRuntime;

// Protocol name
const V1_PROTOCOL: &str = "portforward.k8s.io";

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    pub ports: Option<String>,
}

pub async fn handle_kubectl_v1(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    info!("kubectl v1 port-forward for {}/{}", namespace, name);
    
    // Log headers for debugging
    for (name, value) in headers.iter() {
        debug!("Header: {:?} = {:?}", name, value);
    }

    // Verify pod exists and is running
    let pod = state.storage.pods().get(&namespace, &name).await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let phase = pod.get("status")
        .and_then(|s| s.as_object())
        .and_then(|s| s.get("phase"))
        .and_then(|p| p.as_str())
        .unwrap_or("Unknown");

    if phase != "Running" {
        warn!("Pod {}/{} is not running (phase: {})", namespace, name, phase);
        return Err(StatusCode::CONFLICT);
    }

    // Extract port from query or pod spec
    let port = if let Some(ports_str) = query.ports {
        ports_str.split(',')
            .next()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or_else(|| get_pod_port(&pod))
    } else {
        get_pod_port(&pod)
    };
    
    info!("Using port {} for forwarding", port);

    // Accept WebSocket with v1 protocol only
    Ok(ws
        .protocols([V1_PROTOCOL])
        .on_upgrade(move |socket| {
            handle_v1_session(socket, state.container_runtime.clone(), namespace, name, port)
        }))
}

async fn handle_v1_session(
    socket: WebSocket,
    runtime: Arc<ContainerRuntime>,
    namespace: String,
    pod_name: String,
    port: u16,
) {
    info!("Starting v1 kubectl session for {}/{} port {}", namespace, pod_name, port);
    
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    
    // Create or get container
    let container = match runtime.get_container(&namespace, &pod_name).await {
        Some(c) => {
            info!("Using existing container");
            c
        }
        None => {
            match runtime.start_http_container(&namespace, &pod_name, port).await {
                Ok(c) => {
                    info!("Created new container for port {}", port);
                    c
                }
                Err(e) => {
                    error!("Failed to create container: {}", e);
                    return;
                }
            }
        }
    };
    
    // Connect to container port
    let tcp_stream = match container.connect_to_port(port).await {
        Ok(s) => {
            info!("Connected to container port {}", port);
            s
        }
        Err(e) => {
            error!("Failed to connect to port {}: {}", port, e);
            return;
        }
    };
    
    let (tcp_reader, tcp_writer) = tcp_stream.into_split();
    let tcp_writer = Arc::new(Mutex::new(tcp_writer));
    
    // Channels for bidirectional communication
    let (to_container_tx, mut to_container_rx) = mpsc::channel::<Vec<u8>>(100);
    let (from_container_tx, mut from_container_rx) = mpsc::channel::<Vec<u8>>(100);
    
    // Task: Forward data from container to WebSocket
    let ws_sender_clone = ws_sender.clone();
    let container_to_ws = tokio::spawn(async move {
        while let Some(data) = from_container_rx.recv().await {
            trace!("Sending {} bytes to kubectl", data.len());
            // Prefix with stream 0 (data stream)
            let mut frame = vec![0u8];
            frame.extend_from_slice(&data);
            
            let mut sender = ws_sender_clone.lock().await;
            if sender.send(axum::extract::ws::Message::Binary(frame)).await.is_err() {
                debug!("Failed to send to WebSocket");
                break;
            }
        }
        debug!("Container-to-WS task ended");
    });
    
    // Task: Forward data from channel to container
    let tcp_writer_clone = tcp_writer.clone();
    let ws_to_container = tokio::spawn(async move {
        while let Some(data) = to_container_rx.recv().await {
            trace!("Writing {} bytes to container", data.len());
            let mut writer = tcp_writer_clone.lock().await;
            if let Err(e) = writer.write_all(&data).await {
                debug!("Failed to write to container: {}", e);
                break;
            }
            if let Err(e) = writer.flush().await {
                debug!("Failed to flush: {}", e);
                break;
            }
        }
        debug!("WS-to-container task ended");
    });
    
    // Task: Read from container
    let container_reader = tokio::spawn(async move {
        let mut reader = tcp_reader;
        let mut buffer = vec![0; 8192];
        loop {
            match reader.read(&mut buffer).await {
                Ok(0) => {
                    debug!("Container closed connection");
                    break;
                }
                Ok(n) => {
                    trace!("Read {} bytes from container", n);
                    if from_container_tx.send(buffer[..n].to_vec()).await.is_err() {
                        debug!("Channel closed");
                        break;
                    }
                }
                Err(e) => {
                    debug!("Read error: {}", e);
                    break;
                }
            }
        }
        debug!("Container reader task ended");
    });
    
    // Main loop: Handle incoming WebSocket messages
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                if data.is_empty() {
                    continue;
                }
                
                trace!("Received {} bytes from kubectl", data.len());
                
                // v1 protocol: first byte is stream ID
                let stream_id = data[0];
                
                // Stream 0 = data, Stream 1 = error
                if stream_id == 0 && data.len() > 1 {
                    // Data stream - forward to container
                    let payload = &data[1..];
                    trace!("Data stream: {} bytes", payload.len());
                    if to_container_tx.send(payload.to_vec()).await.is_err() {
                        debug!("Failed to queue data");
                        break;
                    }
                } else if stream_id == 1 && data.len() > 1 {
                    // Error stream
                    let error_msg = String::from_utf8_lossy(&data[1..]);
                    warn!("Error stream: {}", error_msg);
                } else if stream_id == 255 {
                    // Special: might be a resize event or control message
                    trace!("Control message on stream 255");
                }
            }
            Ok(axum::extract::ws::Message::Close(_)) => {
                info!("kubectl closed connection");
                break;
            }
            Ok(axum::extract::ws::Message::Ping(data)) => {
                trace!("Received ping");
                let mut sender = ws_sender.lock().await;
                let _ = sender.send(axum::extract::ws::Message::Pong(data)).await;
            }
            Ok(axum::extract::ws::Message::Pong(_)) => {
                trace!("Received pong");
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {
                trace!("Unexpected message type");
            }
        }
    }
    
    info!("WebSocket loop ended, cleaning up");
    
    // Clean up tasks
    container_to_ws.abort();
    ws_to_container.abort();
    container_reader.abort();
    
    info!("Session cleanup complete");
}

// Extract port from pod specification  
fn get_pod_port(pod: &serde_json::Value) -> u16 {
    pod.get("spec")
        .and_then(|s| s.get("containers"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|container| container.get("ports"))
        .and_then(|p| p.as_array())
        .and_then(|ports| ports.first())
        .and_then(|port| port.get("containerPort"))
        .and_then(|p| p.as_u64())
        .map(|p| p as u16)
        .unwrap_or(80)
}