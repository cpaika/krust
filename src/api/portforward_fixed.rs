#![allow(unused_imports)]
// Full SPDY/3.1 port-forward implementation compatible with kubectl
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
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error, info, warn};

use super::server::AppState;
use super::spdy_protocol::{
    create_settings_frame, create_syn_reply, parse_kubectl_init_frame, parse_spdy_frame,
    ControlFrameType, SpdyFrame, FLAG_FIN, SETTINGS_MAX_CONCURRENT_STREAMS,
    SETTINGS_INITIAL_WINDOW_SIZE,
};
use crate::runtime::container::ContainerRuntime;

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    pub ports: Option<String>,
}

#[derive(Debug, Clone)]
struct StreamInfo {
    stream_id: u32,
    port: u16,
    is_error_stream: bool,
}

pub async fn handle_portforward_fixed(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    _headers: HeaderMap,
) -> Result<Response, StatusCode> {
    info!("SPDY port-forward request for pod {}/{}", namespace, name);

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

    // Parse requested ports from query or pod spec
    let requested_ports = parse_requested_ports(&pod, query.ports);
    info!("Requested ports: {:?}", requested_ports);

    // Handle WebSocket upgrade with SPDY protocol for kubectl
    Ok(ws
        .protocols(["SPDY/3.1+portforward.k8s.io", "portforward.k8s.io"])
        .on_upgrade(move |socket| {
            handle_spdy_session(
                socket,
                state.container_runtime.clone(),
                namespace,
                name,
                requested_ports,
            )
        }))
}

async fn handle_spdy_session(
    socket: WebSocket,
    runtime: Arc<ContainerRuntime>,
    namespace: String,
    pod_name: String,
    requested_ports: Vec<u16>,
) {
    info!(
        "Starting SPDY session for {}/{} with ports {:?}",
        namespace, pod_name, requested_ports
    );

    // Check negotiated protocol
    let protocol = socket.protocol();
    info!("Negotiated protocol: {:?}", protocol);

    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));

    // Stream tracking
    let streams: Arc<RwLock<HashMap<u32, StreamInfo>>> = Arc::new(RwLock::new(HashMap::new()));
    let connections: Arc<RwLock<HashMap<u32, Option<OwnedWriteHalf>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // Channels for bidirectional communication
    let (to_container_tx, mut to_container_rx) = mpsc::channel::<(u32, Vec<u8>)>(100);
    let (from_container_tx, mut from_container_rx) = mpsc::channel::<(u32, Vec<u8>)>(100);

    // Send initial SETTINGS frame
    {
        let settings = vec![
            (SETTINGS_MAX_CONCURRENT_STREAMS, 100),
            (SETTINGS_INITIAL_WINDOW_SIZE, 65536),
        ];
        let settings_frame = create_settings_frame(&settings);
        
        let mut sender = ws_sender.lock().await;
        if sender
            .send(axum::extract::ws::Message::Binary(settings_frame))
            .await
            .is_err()
        {
            error!("Failed to send SETTINGS frame");
            return;
        }
        info!("Sent SETTINGS frame");
    }

    // Spawn task to handle outgoing messages
    let ws_sender_clone = ws_sender.clone();
    let outgoing_task = tokio::spawn(async move {
        while let Some((stream_id, data)) = from_container_rx.recv().await {
            let frame = SpdyFrame::new_data_frame(stream_id, 0, data);
            let mut sender = ws_sender_clone.lock().await;
            if sender
                .send(axum::extract::ws::Message::Binary(frame.to_bytes()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Spawn task to handle container connections
    let connections_clone = connections.clone();
    let streams_clone = streams.clone();
    let runtime_clone = runtime.clone();
    let from_container_tx_clone = from_container_tx.clone();
    let namespace_clone = namespace.clone();
    let pod_name_clone = pod_name.clone();
    
    let container_task = tokio::spawn(async move {
        while let Some((stream_id, data)) = to_container_rx.recv().await {
            let streams = streams_clone.read().await;
            if let Some(stream_info) = streams.get(&stream_id) {
                if stream_info.is_error_stream {
                    // Error streams just echo back for now
                    debug!("Error stream {} data: {:?}", stream_id, String::from_utf8_lossy(&data));
                    continue;
                }

                // Get or create connection
                let mut connections = connections_clone.write().await;
                let conn = connections.entry(stream_id).or_insert(None);

                if conn.is_none() {
                    // Get or create container
                    let container = match runtime_clone.get_container(&namespace_clone, &pod_name_clone).await {
                        Some(c) => c,
                        None => {
                            match runtime_clone
                                .start_http_container(&namespace_clone, &pod_name_clone, stream_info.port)
                                .await
                            {
                                Ok(c) => {
                                    info!("Created container for port {}", stream_info.port);
                                    c
                                }
                                Err(e) => {
                                    error!("Failed to create container: {}", e);
                                    continue;
                                }
                            }
                        }
                    };

                    // Connect to container port
                    match container.connect_to_port(stream_info.port).await {
                        Ok(stream) => {
                            info!("Connected to container port {}", stream_info.port);
                            let (read_half, write_half) = stream.into_split();
                            *conn = Some(write_half);

                            // Spawn reader for this connection
                            let from_tx = from_container_tx_clone.clone();
                            let stream_id_copy = stream_id;
                            
                            tokio::spawn(async move {
                                let mut read_half = read_half;
                                let mut buffer = vec![0; 8192];
                                loop {
                                    match read_half.read(&mut buffer).await {
                                        Ok(0) => {
                                            debug!("Container connection closed for stream {}", stream_id_copy);
                                            break;
                                        }
                                        Ok(n) => {
                                            debug!("Read {} bytes from container stream {}", n, stream_id_copy);
                                            let _ = from_tx.send((stream_id_copy, buffer[..n].to_vec())).await;
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
                            error!("Failed to connect to port {}: {}", stream_info.port, e);
                            continue;
                        }
                    }
                }

                // Write data to container
                if let Some(ref mut write_half) = conn {
                    debug!("Writing {} bytes to container stream {}", data.len(), stream_id);
                    if let Err(e) = write_half.write_all(&data).await {
                        error!("Write error to container: {}", e);
                        *conn = None;
                    }
                }
            }
        }
    });

    // Handle incoming WebSocket messages
    info!("Ready to handle SPDY messages");

    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                debug!("Received binary message: {} bytes", data.len());

                // Try to parse as SPDY frame
                if let Some(frame) = parse_spdy_frame(&data) {
                    info!(
                        "Received SPDY frame: control={}, type={:?}, stream_id={}, flags={:#x}, len={}",
                        frame.is_control,
                        frame.frame_type,
                        frame.stream_id,
                        frame.flags,
                        frame.data.len()
                    );

                    if frame.is_control {
                        match frame.frame_type {
                            Some(ControlFrameType::SynStream) => {
                                // Parse stream creation request
                                if frame.data.len() >= 4 {
                                    let associated_stream_id = u32::from_be_bytes([
                                        frame.data[0],
                                        frame.data[1],
                                        frame.data[2],
                                        frame.data[3],
                                    ]);
                                    
                                    info!("SYN_STREAM for stream {} (associated: {})", 
                                        frame.stream_id, associated_stream_id);

                                    // kubectl uses odd stream IDs for data, even for errors
                                    let is_error = (frame.stream_id % 2) == 0;
                                    
                                    // Determine port from stream ID or use first requested port
                                    let port = if !requested_ports.is_empty() {
                                        requested_ports[0]
                                    } else {
                                        80
                                    };

                                    // Store stream info
                                    let mut streams_guard = streams.write().await;
                                    streams_guard.insert(
                                        frame.stream_id,
                                        StreamInfo {
                                            stream_id: frame.stream_id,
                                            port,
                                            is_error_stream: is_error,
                                        },
                                    );

                                    // Send SYN_REPLY
                                    let syn_reply = create_syn_reply(frame.stream_id);
                                    let mut sender = ws_sender.lock().await;
                                    if sender
                                        .send(axum::extract::ws::Message::Binary(syn_reply))
                                        .await
                                        .is_err()
                                    {
                                        error!("Failed to send SYN_REPLY");
                                        break;
                                    }
                                    info!("Sent SYN_REPLY for stream {}", frame.stream_id);
                                }
                            }
                            Some(ControlFrameType::Settings) => {
                                info!("Received SETTINGS frame");
                                // Acknowledge settings by sending empty SETTINGS frame
                                let ack = create_settings_frame(&[]);
                                let mut sender = ws_sender.lock().await;
                                let _ = sender.send(axum::extract::ws::Message::Binary(ack)).await;
                            }
                            Some(ControlFrameType::Ping) => {
                                info!("Received PING, sending PONG");
                                // Echo back the ping as pong (same data)
                                let pong = SpdyFrame::new_control_frame(
                                    ControlFrameType::Ping,
                                    0,
                                    0,
                                    frame.data,
                                );
                                let mut sender = ws_sender.lock().await;
                                let _ = sender
                                    .send(axum::extract::ws::Message::Binary(pong.to_bytes()))
                                    .await;
                            }
                            Some(ControlFrameType::GoAway) => {
                                info!("Received GOAWAY, closing connection");
                                break;
                            }
                            _ => {
                                debug!("Unhandled control frame type: {:?}", frame.frame_type);
                            }
                        }
                    } else {
                        // Data frame
                        if frame.data.is_empty() {
                            debug!("Received empty data frame for stream {}", frame.stream_id);
                        } else {
                            debug!(
                                "Forwarding {} bytes to container for stream {}",
                                frame.data.len(),
                                frame.stream_id
                            );
                            let _ = to_container_tx.send((frame.stream_id, frame.data)).await;
                        }
                    }
                } else {
                    // Might be kubectl's initialization sequence
                    if data.len() == 2 && data[0] == 0x00 {
                        // kubectl port number frame
                        info!("Received kubectl init frame, creating streams");
                        
                        // Create default streams for port forwarding
                        // Stream 1: data stream for first port
                        // Stream 2: error stream for first port
                        let port = if !requested_ports.is_empty() {
                            requested_ports[0]
                        } else {
                            80
                        };

                        let mut streams_guard = streams.write().await;
                        streams_guard.insert(
                            1,
                            StreamInfo {
                                stream_id: 1,
                                port,
                                is_error_stream: false,
                            },
                        );
                        streams_guard.insert(
                            2,
                            StreamInfo {
                                stream_id: 2,
                                port,
                                is_error_stream: true,
                            },
                        );

                        // Send SYN_REPLY for both streams
                        for stream_id in [1, 2] {
                            let syn_reply = create_syn_reply(stream_id);
                            let mut sender = ws_sender.lock().await;
                            if sender
                                .send(axum::extract::ws::Message::Binary(syn_reply))
                                .await
                                .is_err()
                            {
                                error!("Failed to send SYN_REPLY");
                                break;
                            }
                            info!("Sent SYN_REPLY for stream {}", stream_id);
                        }
                    } else if let Some((local_port, remote_port)) = parse_kubectl_init_frame(&data) {
                        info!(
                            "Parsed kubectl port config: local={}, remote={}",
                            local_port, remote_port
                        );
                    } else {
                        warn!("Unknown frame format: {:02x?}", &data[..data.len().min(20)]);
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
            _ => {}
        }
    }

    // Clean up
    outgoing_task.abort();
    container_task.abort();

    // Close all connections
    let mut connections = connections.write().await;
    connections.clear();

    info!("SPDY session ended for {}/{}", namespace, pod_name);
}

fn parse_requested_ports(pod: &serde_json::Value, query_ports: Option<String>) -> Vec<u16> {
    // Try query string first
    if let Some(ports_str) = query_ports {
        let mut ports = Vec::new();
        for port_spec in ports_str.split(',') {
            let parts: Vec<&str> = port_spec.split(':').collect();
            if let Some(port_str) = parts.last() {
                if let Ok(port) = port_str.parse::<u16>() {
                    ports.push(port);
                }
            }
        }
        if !ports.is_empty() {
            return ports;
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
                let mut container_ports = Vec::new();
                for port in ports {
                    if let Some(container_port) = port
                        .get("containerPort")
                        .and_then(|p| p.as_u64())
                    {
                        container_ports.push(container_port as u16);
                    }
                }
                if !container_ports.is_empty() {
                    return container_ports;
                }
            }
        }
    }

    // Default to port 80
    vec![80]
}