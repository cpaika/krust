// Port-forward v2 implementation with proper stream handling
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
use std::collections::HashMap;

use super::server::AppState;
use crate::runtime::container::ContainerRuntime;

// Protocol name
const V1_PROTOCOL: &str = "portforward.k8s.io";

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    pub ports: Option<String>,
}

// Stream types in kubectl port-forward protocol
const STDIN_STREAM: u8 = 0;    // Not used in port-forward
const STDOUT_STREAM: u8 = 1;   // Not used in port-forward  
const STDERR_STREAM: u8 = 2;   // Not used in port-forward
const ERROR_STREAM: u8 = 3;    // Error messages
const RESIZE_STREAM: u8 = 4;   // Terminal resize events

pub async fn handle_portforward_v2(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    info!("Port-forward v2 for {}/{}", namespace, name);
    
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

    // Parse port from query
    let ports: Vec<u16> = if let Some(ports_str) = query.ports {
        ports_str.split(',')
            .filter_map(|p| p.parse::<u16>().ok())
            .collect()
    } else {
        vec![get_pod_port(&pod)]
    };
    
    if ports.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    info!("Using ports {:?} for forwarding", ports);

    // Accept WebSocket with v1 protocol
    Ok(ws
        .protocols([V1_PROTOCOL])
        .on_upgrade(move |socket| {
            handle_v2_session(socket, state.container_runtime.clone(), namespace, name, ports)
        }))
}

async fn handle_v2_session(
    socket: WebSocket,
    runtime: Arc<ContainerRuntime>,
    namespace: String,
    pod_name: String,
    ports: Vec<u16>,
) {
    info!("Starting v2 session for {}/{} ports {:?}", namespace, pod_name, ports);
    
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    
    // Track streams for each port
    // kubectl uses stream IDs: (port_index * 2) for data, (port_index * 2 + 1) for error
    let mut port_streams: HashMap<u8, PortForwardStream> = HashMap::new();
    
    // Set up streams for each port
    for (index, &port) in ports.iter().enumerate() {
        let data_stream_id = (index * 2) as u8;
        let error_stream_id = (index * 2 + 1) as u8;
        
        info!("Setting up port {} with streams: data={}, error={}", 
            port, data_stream_id, error_stream_id);
        
        // Create or get container
        let container = match runtime.get_container(&namespace, &pod_name).await {
            Some(c) => c,
            None => {
                match runtime.start_http_container(&namespace, &pod_name, port).await {
                    Ok(c) => {
                        info!("Created new container for port {}", port);
                        c
                    }
                    Err(e) => {
                        error!("Failed to create container: {}", e);
                        send_error_message(ws_sender.clone(), error_stream_id, 
                            &format!("Failed to create container: {}", e)).await;
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
                send_error_message(ws_sender.clone(), error_stream_id,
                    &format!("Failed to connect to port {}: {}", port, e)).await;
                continue;
            }
        };
        
        let (tcp_reader, tcp_writer) = tcp_stream.into_split();
        
        // Create channels for this port
        let (to_container_tx, to_container_rx) = mpsc::channel::<Vec<u8>>(100);
        let (from_container_tx, mut from_container_rx) = mpsc::channel::<Vec<u8>>(100);
        
        // Store stream info
        port_streams.insert(data_stream_id, PortForwardStream {
            port,
            data_stream_id,
            error_stream_id,
            to_container: to_container_tx.clone(),
        });
        
        // Task: Forward data from container to WebSocket
        let ws_sender_clone = ws_sender.clone();
        let container_to_ws = tokio::spawn(async move {
            while let Some(data) = from_container_rx.recv().await {
                trace!("Sending {} bytes from port {} to kubectl on stream {}", 
                    data.len(), port, data_stream_id);
                
                // Create frame with stream ID prefix
                let mut frame = vec![data_stream_id];
                frame.extend_from_slice(&data);
                
                let mut sender = ws_sender_clone.lock().await;
                if sender.send(axum::extract::ws::Message::Binary(frame)).await.is_err() {
                    debug!("Failed to send to WebSocket for port {}", port);
                    break;
                }
            }
            debug!("Container-to-WS task ended for port {}", port);
        });
        
        // Task: Forward data from channel to container
        let tcp_writer = Arc::new(Mutex::new(tcp_writer));
        let tcp_writer_clone = tcp_writer.clone();
        let ws_to_container = tokio::spawn(async move {
            let mut rx = to_container_rx;
            while let Some(data) = rx.recv().await {
                trace!("Writing {} bytes to container port {}", data.len(), port);
                let mut writer = tcp_writer_clone.lock().await;
                if let Err(e) = writer.write_all(&data).await {
                    debug!("Failed to write to container port {}: {}", port, e);
                    break;
                }
                if let Err(e) = writer.flush().await {
                    debug!("Failed to flush port {}: {}", port, e);
                    break;
                }
            }
            debug!("WS-to-container task ended for port {}", port);
        });
        
        // Task: Read from container
        let container_reader = tokio::spawn(async move {
            let mut reader = tcp_reader;
            let mut buffer = vec![0; 8192];
            loop {
                match reader.read(&mut buffer).await {
                    Ok(0) => {
                        debug!("Container closed connection for port {}", port);
                        break;
                    }
                    Ok(n) => {
                        trace!("Read {} bytes from container port {}", n, port);
                        if from_container_tx.send(buffer[..n].to_vec()).await.is_err() {
                            debug!("Channel closed for port {}", port);
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("Read error from container port {}: {}", port, e);
                        break;
                    }
                }
            }
            debug!("Container reader task ended for port {}", port);
        });
    }
    
    // Main loop: Handle incoming WebSocket messages
    let mut message_count = 0;
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                if data.is_empty() {
                    trace!("Received empty binary message");
                    continue;
                }
                
                message_count += 1;
                
                // Log first few messages in detail
                if message_count <= 10 {
                    info!("Message {}: {} bytes, first bytes: {:02x?}", 
                        message_count, data.len(), &data[..data.len().min(20)]);
                }
                
                // Parse stream ID from first byte
                let stream_id = data[0];
                
                // Check if this is a data stream we're handling
                if let Some(stream) = port_streams.get(&stream_id) {
                    if data.len() > 1 {
                        // Forward data to container
                        let payload = &data[1..];
                        trace!("Data for port {} (stream {}): {} bytes", 
                            stream.port, stream_id, payload.len());
                        
                        if stream.to_container.send(payload.to_vec()).await.is_err() {
                            debug!("Failed to queue data for port {}", stream.port);
                        }
                    } else {
                        // Empty data frame - might be initialization
                        trace!("Empty data frame for stream {}", stream_id);
                    }
                } else if stream_id % 2 == 1 {
                    // Odd stream IDs are error streams
                    trace!("Received on error stream {}", stream_id);
                    if data.len() > 1 {
                        let error_msg = String::from_utf8_lossy(&data[1..]);
                        warn!("Error stream {}: {}", stream_id, error_msg);
                    }
                } else {
                    // Unknown stream
                    debug!("Received data for unknown stream {}", stream_id);
                }
            }
            Ok(axum::extract::ws::Message::Close(_)) => {
                info!("kubectl closed connection");
                break;
            }
            Ok(axum::extract::ws::Message::Ping(data)) => {
                trace!("Received ping, sending pong");
                let mut sender = ws_sender.lock().await;
                let _ = sender.send(axum::extract::ws::Message::Pong(data)).await;
            }
            Ok(axum::extract::ws::Message::Pong(_)) => {
                trace!("Received pong");
            }
            Ok(axum::extract::ws::Message::Text(text)) => {
                debug!("Received unexpected text message: {}", text);
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
    }
    
    info!("WebSocket loop ended after {} messages", message_count);
}

struct PortForwardStream {
    port: u16,
    data_stream_id: u8,
    error_stream_id: u8,
    to_container: mpsc::Sender<Vec<u8>>,
}

async fn send_error_message(
    ws_sender: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>,
    stream_id: u8,
    message: &str,
) {
    let mut frame = vec![stream_id];
    frame.extend_from_slice(message.as_bytes());
    
    let mut sender = ws_sender.lock().await;
    let _ = sender.send(axum::extract::ws::Message::Binary(frame)).await;
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