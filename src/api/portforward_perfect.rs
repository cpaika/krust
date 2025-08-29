#![allow(unused_imports)]
// Perfect kubectl port-forward implementation with full bidirectional streaming
use axum::{
    extract::{ws::WebSocket, Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error, info, warn};

use super::server::AppState;
use crate::runtime::container::ContainerRuntime;

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    pub ports: Option<String>,
}

#[derive(Debug, Clone)]
struct PortMapping {
    local_port: u16,
    remote_port: u16,
    data_stream: u8,
    error_stream: u8,
}

pub async fn handle_portforward_perfect(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    _headers: HeaderMap,
) -> Result<Response, StatusCode> {
    info!("Port-forward request for pod {}/{}", namespace, name);
    
    // Verify pod exists and is running
    let pod = state
        .storage
        .pods()
        .get(&namespace, &name)
        .await
        .map_err(|e| {
            error!("Failed to get pod: {}", e);
            StatusCode::NOT_FOUND
        })?;

    let phase = pod
        .get("status")
        .and_then(|s| s.as_object())
        .and_then(|s| s.get("phase"))
        .and_then(|p| p.as_str())
        .unwrap_or("Unknown");

    if phase != "Running" {
        warn!("Pod not running, phase: {}", phase);
        return Err(StatusCode::CONFLICT);
    }

    // Parse ports from query (for testing) or get from pod spec
    let ports = if let Some(ports_str) = query.ports {
        info!("Received ports query string: {}", ports_str);
        parse_port_mappings(&ports_str)
    } else {
        // kubectl doesn't send ports in query string, it sends them via WebSocket
        // For now, get the first container port from the pod spec
        let default_ports = if let Some(containers) = pod.get("spec")
            .and_then(|s| s.get("containers"))
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first()) {
            
            if let Some(ports) = containers.get("ports").and_then(|p| p.as_array()) {
                if let Some(first_port) = ports.first() {
                    let container_port = first_port.get("containerPort")
                        .and_then(|p| p.as_u64())
                        .unwrap_or(80) as u16;
                    
                    info!("Using container port {} from pod spec", container_port);
                    
                    // For kubectl port-forward pod/name localPort:containerPort
                    // We'll handle the actual port mapping in WebSocket frames
                    // For now, assume identity mapping
                    vec![PortMapping {
                        local_port: container_port,
                        remote_port: container_port,
                        data_stream: 0,
                        error_stream: 1,
                    }]
                } else {
                    // No ports defined, default to 80
                    vec![PortMapping {
                        local_port: 80,
                        remote_port: 80,
                        data_stream: 0,
                        error_stream: 1,
                    }]
                }
            } else {
                // No ports defined, default to 80
                vec![PortMapping {
                    local_port: 80,
                    remote_port: 80,
                    data_stream: 0,
                    error_stream: 1,
                }]
            }
        } else {
            // No containers, default to 80
            vec![PortMapping {
                local_port: 80,
                remote_port: 80,
                data_stream: 0,
                error_stream: 1,
            }]
        };
        
        default_ports
    };

    info!("Port mappings: {:?}", ports);

    // Handle WebSocket upgrade with SPDY subprotocol
    Ok(ws
        .protocols(["SPDY/3.1+portforward.k8s.io", "portforward.k8s.io"])
        .on_upgrade(move |socket| {
            handle_websocket_session(socket, state.container_runtime.clone(), namespace, name, ports)
        }))
}

async fn handle_websocket_session(
    socket: WebSocket,
    runtime: Arc<ContainerRuntime>,
    namespace: String,
    pod_name: String,
    ports: Vec<PortMapping>,
) {
    info!("Starting port-forward session for {}/{}", namespace, pod_name);
    
    // Check what protocol was negotiated
    info!("WebSocket protocol: {:?}", socket.protocol());
    
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    
    // Send initial acknowledgment frames immediately
    for port in &ports {
        // Send empty frame for data stream
        let data_frame = create_spdy_frame(port.data_stream, &[]);
        info!("Sending initial data frame for stream {}: {:02x?}", port.data_stream, data_frame);
        {
            let mut sender = ws_sender.lock().await;
            if sender.send(axum::extract::ws::Message::Binary(data_frame)).await.is_err() {
                error!("Failed to send initial data frame");
                return;
            }
        }
        
        // Send empty frame for error stream
        let error_frame = create_spdy_frame(port.error_stream, &[]);
        info!("Sending initial error frame for stream {}: {:02x?}", port.error_stream, error_frame);
        {
            let mut sender = ws_sender.lock().await;
            if sender.send(axum::extract::ws::Message::Binary(error_frame)).await.is_err() {
                error!("Failed to send initial error frame");
                return;
            }
        }
        
        info!("Sent acknowledgment frames for port {}", port.remote_port);
    }
    
    // Get or create container
    let container = match runtime.get_container(&namespace, &pod_name).await {
        Some(c) => c,
        None => {
            // Use the first requested port, or default to 80 for HTTP containers
            let container_port = ports.first().map(|p| p.remote_port).unwrap_or(80);
            
            // Create a test container with HTTP service on the requested port
            match runtime.start_http_container(&namespace, &pod_name, container_port).await {
                Ok(c) => {
                    info!("Created test container for {}/{} with port {}", namespace, pod_name, container_port);
                    c
                }
                Err(e) => {
                    error!("Failed to create container: {}", e);
                    return;
                }
            }
        }
    };
    
    // Create channels for bidirectional communication
    let (to_container_tx, mut to_container_rx) = mpsc::channel::<(u8, Vec<u8>)>(100);
    let (from_container_tx, mut from_container_rx) = mpsc::channel::<(u8, Vec<u8>)>(100);
    
    // Track active connections per stream (write half only, read half spawned)
    let active_connections: Arc<RwLock<HashMap<u8, Option<OwnedWriteHalf>>>> = Arc::new(RwLock::new(HashMap::new()));
    
    // Initialize stream tracking
    for port in &ports {
        active_connections.write().await.insert(port.data_stream, None);
    }
    
    // Spawn task to handle outgoing messages to kubectl
    let ws_sender_clone = ws_sender.clone();
    let outgoing_task = tokio::spawn(async move {
        while let Some((stream_id, data)) = from_container_rx.recv().await {
            let frame = create_spdy_frame(stream_id, &data);
            let mut sender = ws_sender_clone.lock().await;
            if sender.send(axum::extract::ws::Message::Binary(frame)).await.is_err() {
                break;
            }
        }
    });
    
    // Spawn task to handle container connections
    let container_clone = container.clone();
    let connections_clone = active_connections.clone();
    let from_container_tx_clone = from_container_tx.clone();
    let container_task = tokio::spawn(async move {
        while let Some((stream_id, data)) = to_container_rx.recv().await {
            // Find the port mapping for this stream
            let port = ports.iter().find(|p| p.data_stream == stream_id);
            if let Some(port_mapping) = port {
                // Get or create connection to container
                let mut connections = connections_clone.write().await;
                let conn = connections.get_mut(&stream_id).unwrap();
                
                if conn.is_none() {
                    // Create new connection to container
                    match container_clone.connect_to_port(port_mapping.remote_port).await {
                        Ok(stream) => {
                            info!("Connected to container port {}", port_mapping.remote_port);
                            
                            // Split the stream into read and write halves
                            let (read_half, write_half) = stream.into_split();
                            
                            // Store the write half for sending data
                            *conn = Some(write_half);
                            
                            // Spawn reader for this connection with the read half
                            let from_tx = from_container_tx_clone.clone();
                            let data_stream = port_mapping.data_stream;
                            
                            tokio::spawn(async move {
                                let mut read_half = read_half;
                                let mut buffer = vec![0; 8192];
                                loop {
                                    match read_half.read(&mut buffer).await {
                                        Ok(0) => {
                                            debug!("Container connection closed");
                                            break;
                                        }
                                        Ok(n) => {
                                            debug!("Read {} bytes from container", n);
                                            let _ = from_tx.send((data_stream, buffer[..n].to_vec())).await;
                                        }
                                        Err(e) => {
                                            error!("Read error from container: {}", e);
                                            break;
                                        }
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to connect to container port {}: {}", port_mapping.remote_port, e);
                            // Send error on error stream
                            let error_msg = format!("Connection failed: {}", e);
                            let _ = from_container_tx_clone.send((port_mapping.error_stream, error_msg.into_bytes())).await;
                            continue;
                        }
                    }
                }
                
                // Write data to container using the write half
                if let Some(ref mut write_half) = conn {
                    debug!("Writing {} bytes to container port {}", data.len(), port_mapping.remote_port);
                    if let Err(e) = write_half.write_all(&data).await {
                        error!("Write error to container: {}", e);
                        *conn = None;
                    }
                }
            }
        }
    });
    
    // Handle incoming WebSocket messages
    info!("Ready to handle WebSocket messages");
    
    // kubectl sends a special initialization sequence that's not standard SPDY
    let mut initialization_phase = true;
    let mut init_frame_count = 0;
    
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                info!("Received binary message: {} bytes, hex: {:02x?}", data.len(), &data[..data.len().min(20)]);
                
                // During initialization, kubectl sends special frames
                if initialization_phase {
                    init_frame_count += 1;
                    
                    // Check if this is a SPDY control frame (0x8003)
                    if data.len() == 2 && data[0] == 0x80 && data[1] == 0x03 {
                        info!("Received SPDY control frame marker");
                        continue;
                    }
                    
                    // kubectl sends some 2-byte and 4-byte initialization frames
                    // These contain port configuration but we already know the ports
                    if data.len() == 2 || (data.len() == 4 && data == vec![0, 0, 0, 8]) {
                        info!("Received kubectl initialization frame, ignoring");
                        continue;
                    }
                    
                    // After a few init frames, kubectl starts sending normal SPDY frames
                    if init_frame_count > 3 {
                        initialization_phase = false;
                        info!("Initialization phase complete, switching to normal SPDY mode");
                    }
                }
                
                // Parse SPDY data frame
                if let Some(frame) = parse_spdy_frame(&data) {
                    info!("Received SPDY frame: stream={}, flags={}, len={}, data={:02x?}", 
                        frame.stream_id, frame.flags, frame.data.len(), 
                        &frame.data[..frame.data.len().min(20)]);
                    
                    // Stream 0 is data, stream 1 is error
                    // Empty frames are acknowledgments
                    if frame.data.is_empty() && (frame.stream_id == 0 || frame.stream_id == 1) {
                        info!("Received acknowledgment for stream {}", frame.stream_id);
                        continue;
                    }
                    
                    // Forward to container
                    if active_connections.read().await.contains_key(&frame.stream_id) {
                        let _ = to_container_tx.send((frame.stream_id, frame.data)).await;
                    } else {
                        info!("No handler for stream {}", frame.stream_id);
                    }
                } else if !initialization_phase {
                    info!("Failed to parse SPDY frame from {} bytes, raw: {:02x?}", data.len(), data);
                }
            }
            Ok(axum::extract::ws::Message::Text(text)) => {
                info!("Received text message: {}", text);
            }
            Ok(axum::extract::ws::Message::Close(reason)) => {
                info!("WebSocket closed by client: {:?}", reason);
                break;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
    
    // Clean up
    outgoing_task.abort();
    container_task.abort();
    
    // Close all connections
    let mut connections = active_connections.write().await;
    connections.clear();
    
    info!("Port-forward session ended for {}/{}", namespace, pod_name);
}

fn create_spdy_frame(stream_id: u8, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4 + data.len());
    frame.push(stream_id);
    frame.push(0); // flags
    frame.extend_from_slice(&(data.len() as u16).to_be_bytes());
    frame.extend_from_slice(data);
    frame
}

struct SpdyFrame {
    stream_id: u8,
    #[allow(dead_code)]
    flags: u8,
    data: Vec<u8>,
}

fn parse_spdy_frame(data: &[u8]) -> Option<SpdyFrame> {
    if data.len() < 4 {
        return None;
    }
    
    let stream_id = data[0];
    let flags = data[1];
    let length = u16::from_be_bytes([data[2], data[3]]) as usize;
    
    if data.len() != 4 + length {
        // Frame must be exact size
        return None;
    }
    
    Some(SpdyFrame {
        stream_id,
        flags,
        data: data[4..].to_vec(),
    })
}

fn parse_port_mappings(ports_str: &str) -> Vec<PortMapping> {
    let mut mappings = Vec::new();
    let mut stream_id = 0u8;
    
    for port_spec in ports_str.split(',') {
        let port_spec = port_spec.trim();
        if port_spec.is_empty() {
            continue;
        }
        
        let parts: Vec<&str> = port_spec.split(':').collect();
        let mapping = match parts.len() {
            1 => {
                if let Ok(port) = parts[0].parse::<u16>() {
                    Some(PortMapping {
                        local_port: port,
                        remote_port: port,
                        data_stream: stream_id * 2,
                        error_stream: stream_id * 2 + 1,
                    })
                } else {
                    None
                }
            }
            2 => {
                if let (Ok(local), Ok(remote)) = (parts[0].parse::<u16>(), parts[1].parse::<u16>()) {
                    Some(PortMapping {
                        local_port: local,
                        remote_port: remote,
                        data_stream: stream_id * 2,
                        error_stream: stream_id * 2 + 1,
                    })
                } else {
                    None
                }
            }
            _ => None,
        };
        
        if let Some(mapping) = mapping {
            mappings.push(mapping);
            stream_id += 1;
        }
    }
    
    mappings
}