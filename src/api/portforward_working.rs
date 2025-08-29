#![allow(unused_imports)]
// Working kubectl port-forward implementation
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

pub async fn handle_portforward_working(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    headers: HeaderMap,
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

    // Parse ports from query
    let ports = if let Some(ports_str) = query.ports {
        parse_port_mappings(&ports_str)
    } else {
        // Default to port 8080 if not specified
        vec![PortMapping {
            local_port: 8080,
            remote_port: 8080,
            data_stream: 0,
            error_stream: 1,
        }]
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
    
    let (mut ws_sender, mut ws_receiver) = socket.split();
    
    // Send initial acknowledgment frames immediately
    for port in &ports {
        // Send empty frame for data stream
        let data_frame = create_spdy_frame(port.data_stream, &[]);
        if ws_sender.send(axum::extract::ws::Message::Binary(data_frame)).await.is_err() {
            error!("Failed to send initial data frame");
            return;
        }
        
        // Send empty frame for error stream
        let error_frame = create_spdy_frame(port.error_stream, &[]);
        if ws_sender.send(axum::extract::ws::Message::Binary(error_frame)).await.is_err() {
            error!("Failed to send initial error frame");
            return;
        }
        
        debug!("Sent acknowledgment frames for port {}", port.remote_port);
    }
    
    // Get or create container
    let container = match runtime.get_container(&namespace, &pod_name).await {
        Some(c) => c,
        None => {
            // Create a test container with HTTP service
            match runtime.start_http_container(&namespace, &pod_name, 8080).await {
                Ok(c) => {
                    info!("Created test container for {}/{}", namespace, pod_name);
                    c
                }
                Err(e) => {
                    error!("Failed to create container: {}", e);
                    return;
                }
            }
        }
    };
    
    // Set up stream handlers
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    let mut stream_handlers = HashMap::new();
    
    for port in &ports {
        let handler = StreamHandler::new(
            container.clone(),
            port.remote_port,
            port.data_stream,
            port.error_stream,
            ws_sender.clone(),
        );
        
        stream_handlers.insert(port.data_stream, handler);
    }
    
    // Handle incoming WebSocket messages
    info!("Ready to handle WebSocket messages");
    
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                debug!("Received binary message: {} bytes, first bytes: {:02x?}", data.len(), &data[..data.len().min(10)]);
                
                // Check if this is a SPDY control frame (0x8003)
                if data.len() == 2 && data[0] == 0x80 && data[1] == 0x03 {
                    debug!("Received SPDY control frame, ignoring");
                    continue;
                }
                
                // Parse SPDY data frame
                if let Some(frame) = parse_spdy_frame(&data) {
                    debug!("Received SPDY frame: stream={}, len={}", frame.stream_id, frame.data.len());
                    
                    // Route to appropriate handler
                    if let Some(handler) = stream_handlers.get_mut(&frame.stream_id) {
                        handler.send_data(frame.data).await;
                    } else {
                        debug!("No handler for stream {}", frame.stream_id);
                    }
                } else {
                    debug!("Failed to parse SPDY frame from {} bytes", data.len());
                }
            }
            Ok(axum::extract::ws::Message::Text(text)) => {
                debug!("Received text message: {}", text);
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
    for handler in stream_handlers.values_mut() {
        handler.close().await;
    }
    
    info!("Port-forward session ended for {}/{}", namespace, pod_name);
}

struct StreamHandler {
    container: Arc<crate::runtime::container::Container>,
    port: u16,
    data_stream: u8,
    error_stream: u8,
    ws_sender: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>,
    write_half: Option<OwnedWriteHalf>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl StreamHandler {
    fn new(
        container: Arc<crate::runtime::container::Container>,
        port: u16,
        data_stream: u8,
        error_stream: u8,
        ws_sender: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>,
    ) -> Self {
        Self {
            container,
            port,
            data_stream,
            error_stream,
            ws_sender,
            write_half: None,
            task_handle: None,
        }
    }
    
    async fn send_data(&mut self, data: Vec<u8>) {
        // Establish connection if needed
        if self.write_half.is_none() {
            match self.container.connect_to_port(self.port).await {
                Ok(stream) => {
                    info!("Connected to container port {}", self.port);
                    
                    // Split the stream for reading and writing
                    let (mut read_half, write_half) = stream.into_split();
                    let ws_sender = self.ws_sender.clone();
                    let data_stream = self.data_stream;
                    
                    let handle = tokio::spawn(async move {
                        let mut buffer = vec![0; 8192];
                        
                        loop {
                            match read_half.read(&mut buffer).await {
                                Ok(0) => {
                                    debug!("Container connection closed");
                                    break;
                                }
                                Ok(n) => {
                                    debug!("Read {} bytes from container", n);
                                    let frame = create_spdy_frame(data_stream, &buffer[..n]);
                                    let mut sender = ws_sender.lock().await;
                                    if sender.send(axum::extract::ws::Message::Binary(frame)).await.is_err() {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    error!("Read error from container: {}", e);
                                    break;
                                }
                            }
                        }
                    });
                    
                    self.task_handle = Some(handle);
                    self.write_half = Some(write_half);
                }
                Err(e) => {
                    error!("Failed to connect to container port {}: {}", self.port, e);
                    
                    // Send error to client
                    let error_msg = format!("Connection failed: {}", e);
                    let error_frame = create_spdy_frame(self.error_stream, error_msg.as_bytes());
                    let mut sender = self.ws_sender.lock().await;
                    let _ = sender.send(axum::extract::ws::Message::Binary(error_frame)).await;
                    return;
                }
            }
        }
        
        // Write data to container
        if let Some(ref mut write_half) = self.write_half {
            debug!("Writing {} bytes to container port {}", data.len(), self.port);
            if let Err(e) = write_half.write_all(&data).await {
                error!("Write error to container: {}", e);
                self.write_half = None;
            }
        }
    }
    
    async fn close(&mut self) {
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
        self.write_half = None;
    }
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