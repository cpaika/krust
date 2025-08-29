#![allow(unused_imports)]
// Complete kubectl port-forward implementation with proper SPDY/WebSocket handling
use axum::{
    extract::{ws::WebSocket, Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use bollard::{
    container::LogOutput,
    exec::{CreateExecOptions, StartExecOptions, StartExecResults},
    Docker,
};
use tokio::io::AsyncWriteExt;
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

use super::server::AppState;

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

pub async fn handle_portforward_complete(
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

    // Get container info
    let container_id = get_container_id(&namespace, &name).await.ok_or_else(|| {
        error!("Failed to get container ID");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Parse ports from query if provided
    let ports_from_query = parse_port_mappings(&query.ports.unwrap_or_default());

    // Handle WebSocket upgrade with SPDY subprotocol
    Ok(ws
        .protocols(["SPDY/3.1+portforward.k8s.io"])
        .on_upgrade(move |socket| {
            handle_websocket_session(socket, container_id, namespace, name, ports_from_query)
        }))
}

async fn handle_websocket_session(
    socket: WebSocket,
    container_id: String,
    namespace: String,
    pod_name: String,
    mut ports: Vec<PortMapping>,
) {
    info!("Starting port-forward session for {}/{}", namespace, pod_name);
    
    let (mut ws_sender, mut ws_receiver) = socket.split();
    
    // Channel for coordinating stream handlers
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<StreamCommand>(32);
    
    // If no ports provided in query, use default port 8080
    // kubectl doesn't send port info in the WebSocket data for standard port-forward
    if ports.is_empty() {
        debug!("No ports in query, using default port 8080");
        ports.push(PortMapping {
            local_port: 8080,
            remote_port: 8080,
            data_stream: 0,
            error_stream: 1,
        });
        
        // Check if kubectl sends a control frame (0x8003 = SPDY control)
        if let Some(Ok(msg)) = ws_receiver.next().await {
            if let axum::extract::ws::Message::Binary(data) = msg {
                if data.len() >= 2 {
                    let value = u16::from_be_bytes([data[0], data[1]]);
                    if value == 0x8003 {
                        debug!("Received SPDY control frame marker (0x8003)");
                        // This is a control frame, not port data
                    }
                }
            }
        }
    }

    // Send initial acknowledgment frames
    for port in &ports {
        // Send empty frame for data stream
        let data_frame = create_spdy_frame(port.data_stream, &[]);
        if ws_sender.send(axum::extract::ws::Message::Binary(data_frame)).await.is_err() {
            error!("Failed to send data acknowledgment frame");
            return;
        }
        
        // Send empty frame for error stream
        let error_frame = create_spdy_frame(port.error_stream, &[]);
        if ws_sender.send(axum::extract::ws::Message::Binary(error_frame)).await.is_err() {
            error!("Failed to send error acknowledgment frame");
            return;
        }
        debug!("Sent acknowledgment frames for streams {} and {}", port.data_stream, port.error_stream);
    }
    
    info!("Sent all acknowledgment frames, setting up container connections for {} ports", ports.len());

    // Set up container connections
    let stream_handlers = Arc::new(RwLock::new(HashMap::new()));
    
    for port in &ports {
        let mut handler = StreamHandler::new(
            container_id.clone(),
            port.remote_port,
            port.data_stream,
            port.error_stream,
            cmd_tx.clone(),
        );
        
        if let Err(e) = handler.start().await {
            error!("Failed to start stream handler for port {}: {}", port.remote_port, e);
            // Send error on error stream
            let error_msg = format!("Failed to connect to port {}: {}", port.remote_port, e);
            let error_frame = create_spdy_frame(port.error_stream, error_msg.as_bytes());
            let _ = ws_sender.send(axum::extract::ws::Message::Binary(error_frame)).await;
            continue;
        }
        
        stream_handlers.write().await.insert(port.data_stream, handler);
    }

    // Spawn task to handle outgoing messages
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    let ws_sender_clone = ws_sender.clone();
    let outgoing_task = tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                StreamCommand::SendData { stream_id, data } => {
                    let frame = create_spdy_frame(stream_id, &data);
                    let mut sender = ws_sender_clone.lock().await;
                    if sender.send(axum::extract::ws::Message::Binary(frame)).await.is_err() {
                        break;
                    }
                }
                StreamCommand::SendError { stream_id, error } => {
                    let frame = create_spdy_frame(stream_id, error.as_bytes());
                    let mut sender = ws_sender_clone.lock().await;
                    if sender.send(axum::extract::ws::Message::Binary(frame)).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Handle incoming WebSocket messages
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                // Parse SPDY frame
                if let Ok((frame, _)) = parse_spdy_frame(&data) {
                    // Route to appropriate handler
                    let handlers = stream_handlers.read().await;
                    if let Some(handler) = handlers.get(&frame.stream_id) {
                        handler.send_data(frame.data).await;
                    }
                }
            }
            Ok(axum::extract::ws::Message::Close(_)) => {
                info!("WebSocket closed by client");
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
    let handlers = stream_handlers.read().await;
    for handler in handlers.values() {
        handler.stop().await;
    }
    
    info!("Port-forward session ended for {}/{}", namespace, pod_name);
}

// Stream handler for a single port
struct StreamHandler {
    container_id: String,
    port: u16,
    data_stream: u8,
    error_stream: u8,
    cmd_tx: mpsc::Sender<StreamCommand>,
    data_tx: Option<mpsc::Sender<Vec<u8>>>,
    stop_tx: Option<mpsc::Sender<()>>,
}

impl StreamHandler {
    fn new(
        container_id: String,
        port: u16,
        data_stream: u8,
        error_stream: u8,
        cmd_tx: mpsc::Sender<StreamCommand>,
    ) -> Self {
        Self {
            container_id,
            port,
            data_stream,
            error_stream,
            cmd_tx,
            data_tx: None,
            stop_tx: None,
        }
    }

    async fn start(&mut self) -> Result<(), String> {
        info!("Starting stream handler for port {}", self.port);
        
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| format!("Failed to connect to Docker: {}", e))?;

        // Create exec with socat for port forwarding
        let port_str = self.port.to_string();
        let cmd_str = format!("socat - TCP:127.0.0.1:{} 2>&1 || nc 127.0.0.1 {} 2>&1", port_str, port_str);
        info!("Exec command: sh -c '{}'", cmd_str);
        let exec_config = CreateExecOptions {
            cmd: Some(vec![
                "sh",
                "-c",
                &cmd_str,
            ]),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(false),
            ..Default::default()
        };

        let exec = docker
            .create_exec(&self.container_id, exec_config)
            .await
            .map_err(|e| format!("Failed to create exec: {}", e))?;

        let stream = docker
            .start_exec(&exec.id, Some(StartExecOptions::default()))
            .await
            .map_err(|e| format!("Failed to start exec: {}", e))?;

        // Set up bidirectional communication
        let (data_tx, mut data_rx) = mpsc::channel::<Vec<u8>>(32);
        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
        
        self.data_tx = Some(data_tx);
        self.stop_tx = Some(stop_tx);

        let cmd_tx = self.cmd_tx.clone();
        let data_stream = self.data_stream;
        let error_stream = self.error_stream;

        // Handle exec I/O
        tokio::spawn(async move {
            if let StartExecResults::Attached { mut output, mut input } = stream {
                // Spawn task to write to exec stdin
                let write_task = tokio::spawn(async move {
                    while let Some(data) = data_rx.recv().await {
                        if input.write_all(&data).await.is_err() {
                            break;
                        }
                        if input.flush().await.is_err() {
                            break;
                        }
                    }
                });

                // Read from exec stdout/stderr
                loop {
                    tokio::select! {
                        msg = output.next() => {
                            match msg {
                                Some(Ok(LogOutput::StdOut { message })) => {
                                    let _ = cmd_tx.send(StreamCommand::SendData {
                                        stream_id: data_stream,
                                        data: message.to_vec(),
                                    }).await;
                                }
                                Some(Ok(LogOutput::StdErr { message })) => {
                                    let _ = cmd_tx.send(StreamCommand::SendError {
                                        stream_id: error_stream,
                                        error: String::from_utf8_lossy(&message).to_string(),
                                    }).await;
                                }
                                _ => break,
                            }
                        }
                        _ = stop_rx.recv() => {
                            break;
                        }
                    }
                }

                write_task.abort();
            }
        });

        Ok(())
    }

    async fn send_data(&self, data: Vec<u8>) {
        if let Some(tx) = &self.data_tx {
            let _ = tx.send(data).await;
        }
    }

    async fn stop(&self) {
        if let Some(tx) = &self.stop_tx {
            let _ = tx.send(()).await;
        }
    }
}

#[derive(Debug)]
enum StreamCommand {
    SendData { stream_id: u8, data: Vec<u8> },
    SendError { stream_id: u8, error: String },
}

struct SpdyFrame {
    stream_id: u8,
    flags: u8,
    data: Vec<u8>,
}

fn create_spdy_frame(stream_id: u8, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4 + data.len());
    frame.push(stream_id);
    frame.push(0); // flags
    frame.extend_from_slice(&(data.len() as u16).to_be_bytes());
    frame.extend_from_slice(data);
    frame
}

fn parse_spdy_frame(data: &[u8]) -> Result<(SpdyFrame, usize), String> {
    if data.len() < 4 {
        return Err("Frame too short".to_string());
    }
    
    let stream_id = data[0];
    let flags = data[1];
    let length = u16::from_be_bytes([data[2], data[3]]) as usize;
    
    if data.len() < 4 + length {
        return Err("Incomplete frame".to_string());
    }
    
    Ok((
        SpdyFrame {
            stream_id,
            flags,
            data: data[4..4 + length].to_vec(),
        },
        4 + length,
    ))
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

async fn get_container_id(namespace: &str, name: &str) -> Option<String> {
    let docker = Docker::connect_with_local_defaults().ok()?;
    
    let mut filters = HashMap::new();
    filters.insert(
        "label".to_string(),
        vec![
            format!("io.kubernetes.pod.name={}", name),
            format!("io.kubernetes.pod.namespace={}", namespace),
        ],
    );
    
    // Find busybox container for exec (preferred) or main container
    let options = bollard::container::ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };
    
    let containers = docker.list_containers(Some(options)).await.ok()?;
    
    // Prefer busybox/proxy container for better command support
    for container in &containers {
        if let Some(labels) = &container.labels {
            if let Some(container_name) = labels.get("io.kubernetes.container.name") {
                if container_name == "proxy" || container_name.contains("busybox") {
                    return container.id.clone();
                }
            }
        }
    }
    
    // Fall back to first container
    containers.first()?.id.clone()
}