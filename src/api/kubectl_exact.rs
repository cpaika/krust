// Exact kubectl port-forward protocol implementation
// Based on analysis of kubectl client behavior
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
use super::spdy_protocol::{parse_spdy_frame, create_syn_reply, create_settings_frame, SpdyFrame, FLAG_FIN, SETTINGS_MAX_CONCURRENT_STREAMS, SETTINGS_INITIAL_WINDOW_SIZE};

// kubectl uses these exact protocol strings
const SPDY_PROTOCOL: &str = "SPDY/3.1+portforward.k8s.io";
const BASE_PROTOCOL: &str = "portforward.k8s.io";

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    pub ports: Option<String>,
}

// Stream IDs as used by kubectl
const DATA_STREAM_ID: u8 = 0;
const ERROR_STREAM_ID: u8 = 1;

pub async fn handle_kubectl_exact(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    info!("kubectl exact port-forward for {}/{}", namespace, name);
    
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

    // Accept the WebSocket with correct protocols
    Ok(ws
        .protocols([SPDY_PROTOCOL, BASE_PROTOCOL])
        .on_upgrade(move |socket| {
            handle_exact_session(socket, state.container_runtime.clone(), namespace, name, port)
        }))
}

async fn handle_exact_session(
    socket: WebSocket,
    runtime: Arc<ContainerRuntime>,
    namespace: String,
    pod_name: String,
    port: u16,
) {
    info!("Starting exact kubectl session for {}/{} port {}", namespace, pod_name, port);
    
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
                    send_error_frame(ws_sender.clone(), &format!("Failed to create container: {}", e)).await;
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
            send_error_frame(ws_sender.clone(), &format!("Failed to connect: {}", e)).await;
            return;
        }
    };
    
    let (tcp_reader, tcp_writer) = tcp_stream.into_split();
    let tcp_writer = Arc::new(Mutex::new(tcp_writer));
    
    // Channels for bidirectional communication
    let (to_container_tx, mut to_container_rx) = mpsc::channel::<Vec<u8>>(100);
    let (from_container_tx, mut from_container_rx) = mpsc::channel::<Vec<u8>>(100);
    
    // Send initial SETTINGS frame
    info!("Sending initial SETTINGS frame");
    let settings = vec![
        (SETTINGS_MAX_CONCURRENT_STREAMS, 100),
        (SETTINGS_INITIAL_WINDOW_SIZE, 65536),
    ];
    let settings_frame = create_settings_frame(&settings);
    {
        let mut sender = ws_sender.lock().await;
        if sender.send(axum::extract::ws::Message::Binary(settings_frame)).await.is_err() {
            error!("Failed to send initial SETTINGS");
            return;
        }
    }
    
    info!("SETTINGS sent, ready for streams");
    
    // Task: Forward data from container to WebSocket
    let ws_sender_clone = ws_sender.clone();
    let container_to_ws = tokio::spawn(async move {
        // Use stream ID 1 for server->client data (odd = client-initiated stream)
        let stream_id = 1u32;
        while let Some(data) = from_container_rx.recv().await {
            trace!("Sending {} bytes to kubectl on stream {}", data.len(), stream_id);
            let frame = SpdyFrame::new_data_frame(stream_id, 0, data);
            let mut sender = ws_sender_clone.lock().await;
            if sender.send(axum::extract::ws::Message::Binary(frame.to_bytes())).await.is_err() {
                debug!("Failed to send to WebSocket, closing reader");
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
            // Flush to ensure data is sent immediately
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
                        debug!("Channel closed, stopping reader");
                        break;
                    }
                }
                Err(e) => {
                    debug!("Read error from container: {}", e);
                    break;
                }
            }
        }
        debug!("Container reader task ended");
    });
    
    // Main loop: Handle incoming WebSocket messages
    let mut saw_first_data = false;
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                trace!("Received {} bytes from kubectl", data.len());
                
                // Parse SPDY frame
                if let Some(frame) = parse_spdy_frame(&data) {
                    trace!("SPDY Frame: control={}, stream={}, flags={:#x}, type={:?}, data_len={}", 
                        frame.is_control, frame.stream_id, frame.flags, frame.frame_type, frame.data.len());
                    
                    if frame.is_control {
                        // Handle control frames
                        match frame.frame_type {
                            Some(super::spdy_protocol::ControlFrameType::SynStream) => {
                                info!("Received SYN_STREAM for stream {}", frame.stream_id);
                                // Send SYN_REPLY to acknowledge the stream
                                let syn_reply = create_syn_reply(frame.stream_id);
                                let mut sender = ws_sender.lock().await;
                                if sender.send(axum::extract::ws::Message::Binary(syn_reply)).await.is_err() {
                                    debug!("Failed to send SYN_REPLY");
                                    break;
                                }
                            }
                            Some(super::spdy_protocol::ControlFrameType::Settings) => {
                                debug!("Received SETTINGS frame");
                                // Send our own settings
                                let settings = vec![
                                    (SETTINGS_MAX_CONCURRENT_STREAMS, 100),
                                    (SETTINGS_INITIAL_WINDOW_SIZE, 65536),
                                ];
                                let settings_frame = create_settings_frame(&settings);
                                let mut sender = ws_sender.lock().await;
                                if sender.send(axum::extract::ws::Message::Binary(settings_frame)).await.is_err() {
                                    debug!("Failed to send SETTINGS");
                                    break;
                                }
                            }
                            Some(super::spdy_protocol::ControlFrameType::Ping) => {
                                trace!("Received PING, sending PONG");
                                // Echo the ping back as pong
                                let mut sender = ws_sender.lock().await;
                                if sender.send(axum::extract::ws::Message::Binary(data)).await.is_err() {
                                    debug!("Failed to send PONG");
                                    break;
                                }
                            }
                            Some(super::spdy_protocol::ControlFrameType::RstStream) => {
                                warn!("Received RST_STREAM for stream {}", frame.stream_id);
                            }
                            Some(super::spdy_protocol::ControlFrameType::GoAway) => {
                                info!("Received GOAWAY, closing connection");
                                break;
                            }
                            _ => {
                                debug!("Unhandled control frame type: {:?}", frame.frame_type);
                            }
                        }
                    } else {
                        // Data frame
                        if frame.stream_id % 2 == 1 {
                            // Odd stream IDs are client-initiated (data stream)
                            if !frame.data.is_empty() {
                                if !saw_first_data {
                                    saw_first_data = true;
                                    info!("First data frame on stream {}, {} bytes", frame.stream_id, frame.data.len());
                                    debug!("First data bytes: {:02x?}", &frame.data[..frame.data.len().min(20)]);
                                }
                                
                                if to_container_tx.send(frame.data.clone()).await.is_err() {
                                    debug!("Failed to queue data for container");
                                    break;
                                }
                            }
                        } else {
                            // Even stream IDs are server-initiated (error stream)
                            if !frame.data.is_empty() {
                                let error_msg = String::from_utf8_lossy(&frame.data);
                                warn!("Error stream {}: {}", frame.stream_id, error_msg);
                            }
                        }
                    }
                } else {
                    warn!("Failed to parse SPDY frame from {} bytes: {:02x?}", 
                        data.len(), &data[..data.len().min(20)]);
                }
            }
            Ok(axum::extract::ws::Message::Close(_)) => {
                info!("kubectl closed connection gracefully");
                break;
            }
            Ok(axum::extract::ws::Message::Ping(data)) => {
                // Respond to ping with pong
                trace!("Received ping, sending pong");
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

// Create a SPDY data frame
fn make_kubectl_frame(stream_id: u8, data: &[u8]) -> Vec<u8> {
    let frame = SpdyFrame::new_data_frame(
        stream_id as u32,
        if data.is_empty() { FLAG_FIN } else { 0 },
        data.to_vec(),
    );
    frame.to_bytes()
}

// Send acknowledgment frame (empty data frame with FIN flag)
async fn send_ack_frame(ws_sender: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>, stream_id: u8) -> bool {
    let frame = SpdyFrame::new_data_frame(stream_id as u32, FLAG_FIN, vec![]);
    let mut sender = ws_sender.lock().await;
    sender.send(axum::extract::ws::Message::Binary(frame.to_bytes())).await.is_ok()
}

// Send error frame
async fn send_error_frame(ws_sender: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>, error: &str) {
    let frame = make_kubectl_frame(ERROR_STREAM_ID, error.as_bytes());
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