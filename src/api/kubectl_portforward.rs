// Exact kubectl port-forward protocol implementation
// Based on Kubernetes client-go and kubectl source code analysis

use axum::{
    extract::{ws::WebSocket, Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use bytes::{BufMut, BytesMut};
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

// kubectl port-forward protocol constants
const KUBECTL_PROTOCOL_V1: &str = "portforward.k8s.io";
const KUBECTL_SPDY_PROTOCOL: &str = "SPDY/3.1+portforward.k8s.io";
const PROTOCOL_VERSION: [u8; 2] = [0x80, 0x01]; // kubectl's version marker

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    pub ports: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PortMapping {
    pub local_port: u16,
    pub remote_port: u16,
    pub data_stream_id: u8,
    pub error_stream_id: u8,
}

impl PortMapping {
    pub fn new(local_port: u16, remote_port: u16) -> Self {
        // kubectl uses consecutive stream IDs
        // Even IDs for data, odd IDs for errors
        static mut NEXT_PORT_INDEX: u8 = 0;
        let index = unsafe {
            let current = NEXT_PORT_INDEX;
            NEXT_PORT_INDEX += 1;
            current
        };
        
        PortMapping {
            local_port,
            remote_port,
            data_stream_id: index * 2,
            error_stream_id: index * 2 + 1,
        }
    }
}

#[derive(Debug)]
pub struct KubectlFrame {
    pub stream_id: u8,
    pub flags: u8,
    pub data: Vec<u8>,
}

// Stream ID helpers matching kubectl's scheme
pub fn get_data_stream_id(port_index: u8) -> u8 {
    port_index * 2
}

pub fn get_error_stream_id(port_index: u8) -> u8 {
    port_index * 2 + 1
}

// Frame creation functions matching kubectl's exact format
pub fn create_kubectl_init_frame() -> Vec<u8> {
    PROTOCOL_VERSION.to_vec()
}

pub fn create_port_frame(local_port: u16, remote_port: u16) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4);
    frame.extend_from_slice(&local_port.to_be_bytes());
    frame.extend_from_slice(&remote_port.to_be_bytes());
    frame
}

pub fn create_kubectl_data_frame(stream_id: u8, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4 + data.len());
    frame.push(stream_id);
    frame.push(0); // flags
    frame.extend_from_slice(&(data.len() as u16).to_be_bytes());
    frame.extend_from_slice(data);
    frame
}

pub fn create_kubectl_error_frame(port_index: u8, error_msg: &[u8]) -> Vec<u8> {
    let stream_id = get_error_stream_id(port_index);
    create_kubectl_data_frame(stream_id, error_msg)
}

pub fn create_kubectl_close_frame(stream_id: u8) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4);
    frame.push(stream_id);
    frame.push(0x01); // FIN flag
    frame.extend_from_slice(&0u16.to_be_bytes()); // length = 0
    frame
}

pub fn parse_kubectl_frame(data: &[u8]) -> Option<KubectlFrame> {
    if data.len() < 4 {
        return None;
    }
    
    let stream_id = data[0];
    let flags = data[1];
    let length = u16::from_be_bytes([data[2], data[3]]) as usize;
    
    if data.len() < 4 + length {
        return None;
    }
    
    Some(KubectlFrame {
        stream_id,
        flags,
        data: data[4..4 + length].to_vec(),
    })
}

pub fn wrap_for_websocket(stream_id: u8, data: &[u8]) -> Vec<u8> {
    create_kubectl_data_frame(stream_id, data)
}

// Main handler
pub async fn handle_kubectl_portforward(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    _headers: HeaderMap,
) -> Result<Response, StatusCode> {
    info!("kubectl port-forward request for pod {}/{}", namespace, name);

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

    // Parse ports
    let port_mappings = parse_port_mappings(&pod, query.ports);
    info!("Port mappings: {:?}", port_mappings);

    // Handle WebSocket upgrade with kubectl SPDY protocol
    Ok(ws
        .protocols([KUBECTL_SPDY_PROTOCOL, KUBECTL_PROTOCOL_V1])
        .on_upgrade(move |socket| {
            handle_kubectl_session(
                socket,
                state.container_runtime.clone(),
                namespace,
                name,
                port_mappings,
            )
        }))
}

async fn handle_kubectl_session(
    socket: WebSocket,
    runtime: Arc<ContainerRuntime>,
    namespace: String,
    pod_name: String,
    port_mappings: Vec<PortMapping>,
) {
    info!(
        "Starting kubectl session for {}/{} with {} port mappings",
        namespace, pod_name, port_mappings.len()
    );

    let protocol = socket.protocol();
    info!("Negotiated protocol: {:?}", protocol);

    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));

    // Track connections per stream
    let connections: Arc<RwLock<HashMap<u8, Option<OwnedWriteHalf>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // Channels for bidirectional communication
    let (to_container_tx, mut to_container_rx) = mpsc::channel::<(u8, Vec<u8>)>(100);
    let (from_container_tx, mut from_container_rx) = mpsc::channel::<(u8, Vec<u8>)>(100);

    // Send initial acknowledgment for each port
    for mapping in &port_mappings {
        // Send empty data frame as acknowledgment
        let ack_frame = create_kubectl_data_frame(mapping.data_stream_id, &[]);
        let mut sender = ws_sender.lock().await;
        if sender
            .send(axum::extract::ws::Message::Binary(ack_frame))
            .await
            .is_err()
        {
            error!("Failed to send data stream acknowledgment");
            return;
        }
        
        // Send empty error frame as acknowledgment
        let err_ack = create_kubectl_data_frame(mapping.error_stream_id, &[]);
        if sender
            .send(axum::extract::ws::Message::Binary(err_ack))
            .await
            .is_err()
        {
            error!("Failed to send error stream acknowledgment");
            return;
        }
        
        info!("Sent acknowledgments for port {}:{}", mapping.local_port, mapping.remote_port);
    }

    // Spawn task to handle outgoing messages
    let ws_sender_clone = ws_sender.clone();
    let outgoing_task = tokio::spawn(async move {
        while let Some((stream_id, data)) = from_container_rx.recv().await {
            let frame = create_kubectl_data_frame(stream_id, &data);
            let mut sender = ws_sender_clone.lock().await;
            if sender
                .send(axum::extract::ws::Message::Binary(frame))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Spawn task to handle container connections
    let port_mappings_clone = port_mappings.clone();
    let connections_clone = connections.clone();
    let runtime_clone = runtime.clone();
    let from_container_tx_clone = from_container_tx.clone();
    let namespace_clone = namespace.clone();
    let pod_name_clone = pod_name.clone();
    
    let container_task = tokio::spawn(async move {
        while let Some((stream_id, data)) = to_container_rx.recv().await {
            // Find the port mapping for this stream
            let mapping = port_mappings_clone
                .iter()
                .find(|m| m.data_stream_id == stream_id);
                
            if let Some(port_mapping) = mapping {
                // Get or create connection
                let mut conns = connections_clone.write().await;
                let conn = conns.entry(stream_id).or_insert(None);
                
                if conn.is_none() {
                    // Get or create container
                    let container = match runtime_clone.get_container(&namespace_clone, &pod_name_clone).await {
                        Some(c) => c,
                        None => {
                            match runtime_clone
                                .start_http_container(&namespace_clone, &pod_name_clone, port_mapping.remote_port)
                                .await
                            {
                                Ok(c) => {
                                    info!("Created container for port {}", port_mapping.remote_port);
                                    c
                                }
                                Err(e) => {
                                    error!("Failed to create container: {}", e);
                                    // Send error on error stream
                                    let error_msg = format!("Failed to create container: {}", e);
                                    let _ = from_container_tx_clone
                                        .send((port_mapping.error_stream_id, error_msg.into_bytes()))
                                        .await;
                                    continue;
                                }
                            }
                        }
                    };
                    
                    // Connect to container port
                    match container.connect_to_port(port_mapping.remote_port).await {
                        Ok(stream) => {
                            info!("Connected to container port {}", port_mapping.remote_port);
                            let (read_half, write_half) = stream.into_split();
                            *conn = Some(write_half);
                            
                            // Spawn reader for this connection
                            let from_tx = from_container_tx_clone.clone();
                            let data_stream = port_mapping.data_stream_id;
                            
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
                                            error!("Read error: {}", e);
                                            break;
                                        }
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to connect to port {}: {}", port_mapping.remote_port, e);
                            let error_msg = format!("Connection failed: {}", e);
                            let _ = from_container_tx_clone
                                .send((port_mapping.error_stream_id, error_msg.into_bytes()))
                                .await;
                        }
                    }
                }
                
                // Write data to container
                if let Some(ref mut write_half) = conn {
                    if !data.is_empty() {
                        debug!("Writing {} bytes to container", data.len());
                        if let Err(e) = write_half.write_all(&data).await {
                            error!("Write error: {}", e);
                            *conn = None;
                        }
                    }
                }
            }
        }
    });

    // Handle incoming WebSocket messages
    info!("Ready to handle kubectl messages");
    
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                debug!("Received binary message: {} bytes", data.len());
                
                // Check for kubectl initialization
                if data.len() == 2 && data == PROTOCOL_VERSION {
                    info!("Received kubectl protocol version");
                    continue;
                }
                
                // Parse kubectl frame
                if let Some(frame) = parse_kubectl_frame(&data) {
                    debug!(
                        "Parsed kubectl frame: stream={}, flags={:#x}, len={}",
                        frame.stream_id, frame.flags, frame.data.len()
                    );
                    
                    // Check if it's a close frame
                    if frame.flags & 0x01 != 0 && frame.data.is_empty() {
                        info!("Received close frame for stream {}", frame.stream_id);
                        // Clean up connection
                        let mut conns = connections.write().await;
                        conns.remove(&frame.stream_id);
                        continue;
                    }
                    
                    // Forward data to container
                    let _ = to_container_tx.send((frame.stream_id, frame.data)).await;
                } else {
                    warn!("Failed to parse kubectl frame from {} bytes", data.len());
                }
            }
            Ok(axum::extract::ws::Message::Close(reason)) => {
                info!("WebSocket closed: {:?}", reason);
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
    
    info!("kubectl session ended for {}/{}", namespace, pod_name);
}

fn parse_port_mappings(pod: &serde_json::Value, query_ports: Option<String>) -> Vec<PortMapping> {
    // Reset port index counter
    unsafe {
        let ptr = &mut NEXT_PORT_INDEX as *mut u8;
        *ptr = 0;
    }
    
    // Try query string first (format: "8080:80,8443:443")
    if let Some(ports_str) = query_ports {
        let mut mappings = Vec::new();
        for port_spec in ports_str.split(',') {
            let parts: Vec<&str> = port_spec.trim().split(':').collect();
            let mapping = match parts.len() {
                1 => {
                    if let Ok(port) = parts[0].parse::<u16>() {
                        Some(PortMapping::new(port, port))
                    } else {
                        None
                    }
                }
                2 => {
                    if let (Ok(local), Ok(remote)) = 
                        (parts[0].parse::<u16>(), parts[1].parse::<u16>()) {
                        Some(PortMapping::new(local, remote))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            
            if let Some(m) = mapping {
                mappings.push(m);
            }
        }
        
        if !mappings.is_empty() {
            return mappings;
        }
    }
    
    // Fall back to pod spec
    if let Some(containers) = pod
        .get("spec")
        .and_then(|s| s.get("containers"))
        .and_then(|c| c.as_array())
    {
        for container in containers {
            if let Some(ports) = container.get("ports").and_then(|p| p.as_array()) {
                let mut mappings = Vec::new();
                for port in ports {
                    if let Some(container_port) = port
                        .get("containerPort")
                        .and_then(|p| p.as_u64())
                    {
                        mappings.push(PortMapping::new(container_port as u16, container_port as u16));
                    }
                }
                if !mappings.is_empty() {
                    return mappings;
                }
            }
        }
    }
    
    // Default to port 80
    vec![PortMapping::new(80, 80)]
}

// Static mutable for port index tracking (kubectl style)
static mut NEXT_PORT_INDEX: u8 = 0;