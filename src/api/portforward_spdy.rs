#![allow(unused_imports)]
use axum::{
    extract::{ws::WebSocket, Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bollard::{
    container::{LogOutput, LogsOptions}, 
    exec::{CreateExecOptions, StartExecOptions, StartExecResults},
    Docker
};
use bytes::{Bytes, BytesMut};
use futures::{stream::SplitSink, SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error, info, warn};

use super::server::AppState;

const STDIN_STREAM_ID: u8 = 0;
const STDOUT_STREAM_ID: u8 = 1;
const STDERR_STREAM_ID: u8 = 2;

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    pub ports: Option<String>,
}

#[derive(Debug, Clone)]
struct PortMapping {
    local_port: u16,
    remote_port: u16,
    stream_index: u8,
}

#[derive(Debug)]
struct ContainerConnection {
    id: String,
    ip: String,
    namespace: String,
    pod_name: String,
    proxy_container_id: Option<String>,  // Busybox container for exec
}

pub async fn portforward_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    info!("Port-forward request for pod {}/{}", namespace, name);
    debug!("Query params: {:?}", query);
    debug!("Headers: {:?}", headers);
    
    // Check for upgrade headers - kubectl sends these
    let is_spdy = headers
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("SPDY") || v.contains("spdy"))
        .unwrap_or(false);
    
    let is_websocket = headers
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_lowercase() == "websocket")
        .unwrap_or(false);

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

    let status = pod
        .get("status")
        .and_then(|s| s.as_object())
        .ok_or_else(|| {
            warn!("Pod has no status");
            StatusCode::CONFLICT
        })?;
    
    let phase = status
        .get("phase")
        .and_then(|p| p.as_str())
        .ok_or_else(|| {
            warn!("Pod has no phase");
            StatusCode::CONFLICT
        })?;
    
    if phase != "Running" {
        warn!("Pod is not running, phase: {}", phase);
        return Err(StatusCode::CONFLICT);
    }

    // Parse ports from query - kubectl may send them in WebSocket data instead
    let ports_from_query = parse_port_mappings(&query.ports.unwrap_or_default());
    
    // For WebSocket protocol, kubectl sends ports after connection is established
    // So we'll allow empty ports here and get them from the WebSocket data
    if ports_from_query.is_empty() {
        info!("No ports in query, will get from WebSocket protocol");
    }

    info!(
        "Setting up port-forward for pod {}/{} with ports: {:?}",
        namespace, name, ports_from_query
    );

    // Get container connection info
    let container = get_container_connection(&namespace, &name)
        .await
        .ok_or_else(|| {
            error!("Failed to get container info");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Handle WebSocket upgrade with SPDY subprotocol
    // kubectl expects the "SPDY/3.1+portforward.k8s.io" subprotocol
    Ok(ws
        .protocols(["SPDY/3.1+portforward.k8s.io"])
        .on_upgrade(move |socket| {
            handle_portforward_websocket(socket, container, ports_from_query, is_spdy)
        }))
}

fn parse_port_mappings(ports_str: &str) -> Vec<PortMapping> {
    let mut mappings = Vec::new();
    let mut stream_index = 0u8;
    
    for port_spec in ports_str.split(',') {
        let parts: Vec<&str> = port_spec.split(':').collect();
        let mapping = match parts.len() {
            1 => {
                if let Ok(port) = parts[0].parse::<u16>() {
                    Some(PortMapping {
                        local_port: port,
                        remote_port: port,
                        stream_index: stream_index * 2, // Each port uses 2 streams (data + error)
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
                        stream_index: stream_index * 2,
                    })
                } else {
                    None
                }
            }
            _ => None,
        };
        
        if let Some(mapping) = mapping {
            mappings.push(mapping);
            stream_index += 1;
        }
    }
    
    mappings
}

async fn get_container_connection(namespace: &str, name: &str) -> Option<ContainerConnection> {
    let docker = Docker::connect_with_local_defaults().ok()?;
    
    // Find containers by pod labels
    let mut filters = HashMap::new();
    filters.insert(
        "label".to_string(),
        vec![
            format!("io.kubernetes.pod.name={}", name),
            format!("io.kubernetes.pod.namespace={}", namespace),
        ],
    );
    
    let options = bollard::container::ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };
    
    let containers = docker.list_containers(Some(options)).await.ok()?;
    
    // Find the main container and proxy container
    let mut main_container_id = None;
    let mut proxy_container_id = None;
    let mut container_ip = None;
    
    for container in containers {
        if let Some(labels) = &container.labels {
            if let Some(container_name) = labels.get("io.kubernetes.container.name") {
                if container_name == "proxy" || container_name.contains("busybox") {
                    proxy_container_id = container.id.clone();
                } else if container_name == "web" {
                    main_container_id = container.id.clone();
                    // Get IP from the main container
                    if let Some(id) = &main_container_id {
                        if let Ok(inspect) = docker.inspect_container(id, None).await {
                            if let Some(network_settings) = inspect.network_settings {
                                if let Some(networks) = network_settings.networks {
                                    if let Some(bridge) = networks.get("bridge") {
                                        container_ip = bridge.ip_address.clone();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // If no proxy container found, use the main container for both
    let container_id = main_container_id?;
    let ip = container_ip?;
    
    Some(ContainerConnection {
        id: container_id.clone(),
        ip,
        namespace: namespace.to_string(),
        pod_name: name.to_string(),
        proxy_container_id: proxy_container_id.or_else(|| Some(container_id)),
    })
}

async fn handle_portforward_websocket(
    ws: WebSocket,
    container: ContainerConnection,
    mut ports: Vec<PortMapping>,
    is_spdy: bool,
) {
    info!(
        "Handling port-forward WebSocket for {}/{} (SPDY: {})",
        container.namespace, container.pod_name, is_spdy
    );
    
    let (ws_sender, mut ws_receiver) = ws.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    
    // Create channels for each port
    let mut port_handlers: HashMap<u8, mpsc::Sender<Bytes>> = HashMap::new();
    let mut tcp_tasks = Vec::new();
    
    // If no ports were provided in query, we need to handle dynamic port setup
    // kubectl doesn't send ports separately - it just starts sending SPDY frames
    // We'll create port handlers dynamically as we receive data for new streams
    let ports_provided = !ports.is_empty();
    
    if !ports_provided {
        info!("No ports provided in query, will handle streams dynamically");
        // For now, assume a default port mapping for 8080
        // In a real implementation, we'd parse this from the SPDY control frames
        ports.push(PortMapping {
            local_port: 8080,
            remote_port: 8080,
            stream_index: 0,
        });
    }
    
    // Now set up TCP proxies for the ports
    for port_mapping in &ports {
        // Create channel for data stream
        let (tx, mut rx) = mpsc::channel::<Bytes>(100);
        port_handlers.insert(port_mapping.stream_index, tx);
        
        // Also create channel for error stream (stream_index + 1)
        let (err_tx, mut err_rx) = mpsc::channel::<Bytes>(100);
        port_handlers.insert(port_mapping.stream_index + 1, err_tx);
        
        // Spawn a task to handle error stream (just log for now)
        tokio::spawn(async move {
            while let Some(data) = err_rx.recv().await {
                debug!("Error stream data: {:?}", String::from_utf8_lossy(&data));
            }
        });
        
        // Send initial empty frames to acknowledge the streams
        // kubectl expects this to know the streams are ready
        {
            let mut sender = ws_sender.lock().await;
            // Send empty data frame for data stream
            let data_frame = create_spdy_frame(port_mapping.stream_index, &[]);
            if let Err(e) = sender.send(axum::extract::ws::Message::Binary(data_frame)).await {
                error!("Failed to send initial data frame: {}", e);
                continue;  // Continue with next port instead of returning
            }
            
            // Send empty frame for error stream
            let error_frame = create_spdy_frame(port_mapping.stream_index + 1, &[]);
            if let Err(e) = sender.send(axum::extract::ws::Message::Binary(error_frame)).await {
                error!("Failed to send initial error frame: {}", e);
                continue;  // Continue with next port instead of returning
            }
            info!("Sent initial frames for streams {} and {}", port_mapping.stream_index, port_mapping.stream_index + 1);
        }
        
        // For now, skip the actual TCP proxy to simplify debugging
        // Just consume the channel to prevent blocking
        let ws_sender_clone = ws_sender.clone();
        let stream_index = port_mapping.stream_index;
        let task = tokio::spawn(async move {
            // Simple echo server for testing
            while let Some(data) = rx.recv().await {
                debug!("Received {} bytes on stream {}", data.len(), stream_index);
                // Echo the data back
                let frame = create_spdy_frame(stream_index, &data);
                let mut sender = ws_sender_clone.lock().await;
                let _ = sender.send(axum::extract::ws::Message::Binary(frame)).await;
            }
        });
        tcp_tasks.push(task);
    }
    
    info!("Set up {} port handlers", port_handlers.len());
    
    // Handle remaining WebSocket messages
    info!("Ready to handle WebSocket messages");
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                debug!("Received binary message with {} bytes", data.len());
                // Parse SPDY frame or raw data
                if is_spdy || !port_handlers.is_empty() {
                    handle_spdy_frame(data, &port_handlers).await;
                } else {
                    // For non-SPDY, route to first port
                    if let Some(tx) = port_handlers.get(&0) {
                        let _ = tx.send(Bytes::from(data)).await;
                    }
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
            _ => {
                debug!("Received other message type");
            }
        }
    }
    
    // Clean up
    for task in tcp_tasks {
        task.abort();
    }
    
    info!("Port-forward session ended for {}/{}", container.namespace, container.pod_name);
}

async fn handle_spdy_frame(data: Vec<u8>, port_handlers: &HashMap<u8, mpsc::Sender<Bytes>>) {
    // kubectl uses a slightly different SPDY frame format
    // The stream ID can be just the first byte for simple cases
    // Let's try to handle both formats
    
    if data.is_empty() {
        return;
    }
    
    // For kubectl's WebSocket SPDY, the data might already be the payload
    // without the SPDY frame header, or it might include it
    
    // Check if this looks like a SPDY frame with header
    if data.len() >= 4 {
        // Try to parse as SPDY frame
        let stream_id = data[0];
        let _flags = data[1];
        
        // Check if the length field makes sense
        if data.len() >= 4 {
            let length = u16::from_be_bytes([data[2], data[3]]) as usize;
            
            if data.len() == 4 + length {
                // This looks like a proper SPDY frame
                let payload = &data[4..];
                
                // Route to appropriate port handler
                if let Some(tx) = port_handlers.get(&stream_id) {
                    let _ = tx.send(Bytes::from(payload.to_vec())).await;
                } else {
                    debug!("No handler for stream {}", stream_id);
                }
                return;
            }
        }
    }
    
    // If not a standard SPDY frame, treat the first byte as stream ID
    // and the rest as payload (kubectl sometimes does this)
    if data.len() > 1 {
        let stream_id = data[0];
        let payload = &data[1..];
        
        if let Some(tx) = port_handlers.get(&stream_id) {
            let _ = tx.send(Bytes::from(payload.to_vec())).await;
        } else {
            // Try stream 0 as default
            if let Some(tx) = port_handlers.get(&0) {
                let _ = tx.send(Bytes::from(data)).await;
            } else {
                debug!("No handler for stream {} or default", stream_id);
            }
        }
    }
}

async fn handle_port_tcp_proxy(
    port_mapping: PortMapping,
    container_ip: String,
    mut rx: mpsc::Receiver<Bytes>,
    ws_sender: Arc<Mutex<SplitSink<WebSocket, axum::extract::ws::Message>>>,
) {
    let container_endpoint = format!("{}:{}", container_ip, port_mapping.remote_port);
    
    info!(
        "Starting TCP proxy for port {} -> {}",
        port_mapping.local_port, container_endpoint
    );
    
    // On macOS, Docker containers run in a VM and their IPs aren't directly accessible
    // We need to use Docker port forwarding or exec
    // For now, let's try using Docker exec with nc (netcat) for the connection
    
    // First, try direct connection (works on Linux)
    let tcp_stream = match TcpStream::connect(&container_endpoint).await {
        Ok(stream) => stream,
        Err(_) => {
            // If direct connection fails, try using Docker exec
            // This is a workaround for macOS Docker Desktop
            info!("Direct connection failed, trying Docker exec workaround");
            
            // For now, we'll just return an error
            // A full implementation would use Docker's exec API to create a tunnel
            error!("Failed to connect to container {} (Docker on macOS requires port mapping)", container_endpoint);
            let error_msg = format!("Connection failed: Docker on macOS requires port mapping");
            let error_frame = create_spdy_frame(
                port_mapping.stream_index + 1, // Error stream
                error_msg.as_bytes(),
            );
            let mut sender = ws_sender.lock().await;
            let _ = sender
                .send(axum::extract::ws::Message::Binary(error_frame))
                .await;
            return;
        }
    };
    
    let (mut tcp_reader, mut tcp_writer) = tcp_stream.into_split();
    
    // Spawn task to read from TCP and send to WebSocket
    let ws_sender_clone = ws_sender.clone();
    let stream_index = port_mapping.stream_index;
    let tcp_to_ws = tokio::spawn(async move {
        let mut buffer = vec![0u8; 8192];
        loop {
            match tcp_reader.read(&mut buffer).await {
                Ok(0) => {
                    debug!("TCP connection closed");
                    break;
                }
                Ok(n) => {
                    let frame = create_spdy_frame(stream_index, &buffer[..n]);
                    let mut sender = ws_sender_clone.lock().await;
                    if sender
                        .send(axum::extract::ws::Message::Binary(frame))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    error!("TCP read error: {}", e);
                    break;
                }
            }
        }
    });
    
    // Handle WebSocket to TCP
    while let Some(data) = rx.recv().await {
        if tcp_writer.write_all(&data).await.is_err() {
            break;
        }
    }
    
    // Clean up
    tcp_to_ws.abort();
}

fn create_spdy_frame(stream_id: u8, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4 + data.len());
    frame.push(stream_id);
    frame.push(0); // flags
    frame.extend_from_slice(&(data.len() as u16).to_be_bytes());
    frame.extend_from_slice(data);
    frame
}

// Docker exec-based port forwarding for macOS compatibility
async fn handle_port_tcp_proxy_exec(
    port_mapping: PortMapping,
    container_id: String,
    mut rx: mpsc::Receiver<Bytes>,
    ws_sender: Arc<Mutex<SplitSink<WebSocket, axum::extract::ws::Message>>>,
) {
    info!(
        "Starting Docker exec proxy for port {} -> container:{}",
        port_mapping.local_port, port_mapping.remote_port
    );
    
    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => {
            error!("Failed to connect to Docker: {}", e);
            return;
        }
    };
    
    // Create exec instance to forward data to the container port
    // Use nc (netcat) which is available in busybox
    let port_str = port_mapping.remote_port.to_string();
    let exec_config = bollard::exec::CreateExecOptions {
        cmd: Some(vec![
            "nc",
            "127.0.0.1",
            &port_str,
        ]),
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        tty: Some(false),
        ..Default::default()
    };
    
    info!("Creating exec in container {} with nc to port {}", container_id, port_str);
    let exec = match docker.create_exec(&container_id, exec_config).await {
        Ok(e) => e,
        Err(e) => {
            error!("Failed to create exec in container {}: {}", container_id, e);
            // Send error back via error stream
            let error_msg = format!("Exec creation failed: {}", e);
            let error_frame = create_spdy_frame(
                port_mapping.stream_index + 1,
                error_msg.as_bytes(),
            );
            let mut sender = ws_sender.lock().await;
            let _ = sender
                .send(axum::extract::ws::Message::Binary(error_frame))
                .await;
            return;
        }
    };
    
    // Start the exec with attach
    let start_config = Some(bollard::exec::StartExecOptions {
        detach: false,
        ..Default::default()
    });
    
    info!("Starting exec {}", exec.id);
    let stream = match docker.start_exec(&exec.id, start_config).await {
        Ok(s) => {
            info!("Exec started successfully");
            s
        },
        Err(e) => {
            error!("Failed to start exec {}: {}", exec.id, e);
            // Send error back via error stream
            let error_msg = format!("Exec start failed: {}", e);
            let error_frame = create_spdy_frame(
                port_mapping.stream_index + 1,
                error_msg.as_bytes(),
            );
            let mut sender = ws_sender.lock().await;
            let _ = sender
                .send(axum::extract::ws::Message::Binary(error_frame))
                .await;
            return;
        }
    };
    
    // Handle bidirectional data flow
    let ws_sender_clone = ws_sender.clone();
    let stream_index = port_mapping.stream_index;
    
    // Spawn task to read from exec output and send to WebSocket
    let exec_to_ws = tokio::spawn(async move {
        if let StartExecResults::Attached { mut output, .. } = stream {
            while let Some(msg) = output.next().await {
                match msg {
                    Ok(LogOutput::StdOut { message }) => {
                        let frame = create_spdy_frame(stream_index, &message);
                        let mut sender = ws_sender_clone.lock().await;
                        if sender
                            .send(axum::extract::ws::Message::Binary(frame))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(LogOutput::StdErr { message }) => {
                        // Send stderr to error stream
                        let frame = create_spdy_frame(stream_index + 1, &message);
                        let mut sender = ws_sender_clone.lock().await;
                        let _ = sender
                            .send(axum::extract::ws::Message::Binary(frame))
                            .await;
                    }
                    _ => {}
                }
            }
        }
    });
    
    // Handle WebSocket to exec stdin
    // Note: This is simplified - a full implementation would need proper stdin handling
    while let Some(data) = rx.recv().await {
        // In a full implementation, we'd write to the exec's stdin
        debug!("Would send {} bytes to container", data.len());
    }
    
    exec_to_ws.abort();
}