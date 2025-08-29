// Simplified kubectl port-forward implementation based on actual protocol analysis
use axum::{
    extract::{ws::WebSocket, Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error, info, warn};

use super::server::AppState;
use crate::runtime::container::ContainerRuntime;

const PROTOCOL: &str = "SPDY/3.1+portforward.k8s.io";

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    pub ports: Option<String>,
}

pub async fn handle_kubectl_simple(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    _headers: HeaderMap,
) -> Result<Response, StatusCode> {
    info!("kubectl simple port-forward for {}/{}", namespace, name);

    // Verify pod
    let pod = state.storage.pods().get(&namespace, &name).await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let phase = pod.get("status")
        .and_then(|s| s.as_object())
        .and_then(|s| s.get("phase"))
        .and_then(|p| p.as_str())
        .unwrap_or("Unknown");

    if phase != "Running" {
        return Err(StatusCode::CONFLICT);
    }

    // Get port from pod spec
    let port = get_pod_port(&pod);
    info!("Using port {}", port);

    Ok(ws
        .protocols([PROTOCOL, "portforward.k8s.io"])
        .on_upgrade(move |socket| {
            handle_simple_session(socket, state.container_runtime.clone(), namespace, name, port)
        }))
}

async fn handle_simple_session(
    socket: WebSocket,
    runtime: Arc<ContainerRuntime>,
    namespace: String,
    pod_name: String,
    port: u16,
) {
    info!("Simple session for {}/{} port {}", namespace, pod_name, port);
    
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    
    // Channels for data flow
    let (to_container_tx, mut to_container_rx) = mpsc::channel::<Vec<u8>>(100);
    let (from_container_tx, mut from_container_rx) = mpsc::channel::<Vec<u8>>(100);
    
    // Send initial empty frames as acknowledgment
    // kubectl expects stream 0 (data) and stream 1 (error) acknowledgments
    for stream_id in [0u8, 1u8] {
        let frame = make_simple_frame(stream_id, &[]);
        let mut sender = ws_sender.lock().await;
        if sender.send(axum::extract::ws::Message::Binary(frame)).await.is_err() {
            error!("Failed to send acknowledgment");
            return;
        }
    }
    info!("Sent acknowledgments");
    
    // Get or create container
    let container = match runtime.get_container(&namespace, &pod_name).await {
        Some(c) => c,
        None => {
            match runtime.start_http_container(&namespace, &pod_name, port).await {
                Ok(c) => {
                    info!("Created container for port {}", port);
                    c
                }
                Err(e) => {
                    error!("Failed to create container: {}", e);
                    return;
                }
            }
        }
    };
    
    // Connect to container
    let tcp_stream = match container.connect_to_port(port).await {
        Ok(s) => {
            info!("Connected to container port {}", port);
            s
        }
        Err(e) => {
            error!("Failed to connect: {}", e);
            // Send error on stream 1
            let error_frame = make_simple_frame(1, e.to_string().as_bytes());
            let mut sender = ws_sender.lock().await;
            let _ = sender.send(axum::extract::ws::Message::Binary(error_frame)).await;
            return;
        }
    };
    
    let (read_half, write_half) = tcp_stream.into_split();
    let write_half = Arc::new(Mutex::new(write_half));
    
    // Spawn task: WebSocket -> Container
    let write_half_clone = write_half.clone();
    let ws_to_container = tokio::spawn(async move {
        while let Some(data) = to_container_rx.recv().await {
            let mut writer = write_half_clone.lock().await;
            if let Err(e) = writer.write_all(&data).await {
                error!("Write to container failed: {}", e);
                break;
            }
        }
    });
    
    // Spawn task: Container -> WebSocket
    let ws_sender_clone = ws_sender.clone();
    let container_to_ws = tokio::spawn(async move {
        while let Some(data) = from_container_rx.recv().await {
            let frame = make_simple_frame(0, &data);
            let mut sender = ws_sender_clone.lock().await;
            if sender.send(axum::extract::ws::Message::Binary(frame)).await.is_err() {
                break;
            }
        }
    });
    
    // Spawn task: Read from container
    let from_tx = from_container_tx.clone();
    let container_reader = tokio::spawn(async move {
        let mut read_half = read_half;
        let mut buffer = vec![0; 8192];
        loop {
            match read_half.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => {
                    debug!("Read {} bytes from container", n);
                    let _ = from_tx.send(buffer[..n].to_vec()).await;
                }
                Err(e) => {
                    error!("Read error: {}", e);
                    break;
                }
            }
        }
    });
    
    // Handle incoming WebSocket messages
    let mut init_count = 0;
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                debug!("Received {} bytes: {:02x?}", data.len(), &data[..data.len().min(20)]);
                
                // kubectl sends several initialization frames
                // We need to handle them gracefully
                if data.len() <= 4 && init_count < 10 {
                    init_count += 1;
                    debug!("Init frame {}: {:02x?}", init_count, data);
                    // Don't skip these - they might be important
                }
                
                // Parse simple frame: [stream_id][flags][length:2][data]
                if data.len() >= 4 {
                    let stream_id = data[0];
                    let flags = data[1];
                    let length = u16::from_be_bytes([data[2], data[3]]) as usize;
                    
                    debug!("Frame: stream={}, flags={:#x}, len={}", stream_id, flags, length);
                    
                    // Check if we have complete frame
                    if data.len() >= 4 + length {
                        if stream_id == 0 && length > 0 {
                            // Data for container
                            let payload = &data[4..4+length];
                            debug!("Forwarding {} bytes to container", payload.len());
                            let _ = to_container_tx.send(payload.to_vec()).await;
                        } else if stream_id == 0 && length == 0 && flags == 0 {
                            // Empty data frame - might be a keep-alive
                            debug!("Keep-alive frame");
                        }
                    }
                } else if data.len() == 2 && data == [0x00, 0x00] {
                    // Special init completion marker
                    info!("Received init completion marker");
                }
            }
            Ok(axum::extract::ws::Message::Close(_)) => {
                info!("WebSocket closed");
                break;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
    
    // Cleanup
    ws_to_container.abort();
    container_to_ws.abort();
    container_reader.abort();
    
    info!("Session ended");
}

fn make_simple_frame(stream_id: u8, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4 + data.len());
    frame.push(stream_id);
    frame.push(0); // flags
    frame.extend_from_slice(&(data.len() as u16).to_be_bytes());
    frame.extend_from_slice(data);
    frame
}

fn get_pod_port(pod: &serde_json::Value) -> u16 {
    if let Some(containers) = pod.get("spec")
        .and_then(|s| s.get("containers"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first()) {
        
        if let Some(ports) = containers.get("ports").and_then(|p| p.as_array()) {
            if let Some(first_port) = ports.first() {
                return first_port.get("containerPort")
                    .and_then(|p| p.as_u64())
                    .unwrap_or(80) as u16;
            }
        }
    }
    80
}