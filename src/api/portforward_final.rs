// Final port-forward implementation that handles both POST and GET
use axum::{
    extract::{ws::WebSocket, Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, Method, StatusCode, Request},
    response::{IntoResponse, Response},
    body::Body,
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

// Protocol names
const V1_PROTOCOL: &str = "portforward.k8s.io";
const SPDY_PROTOCOL: &str = "SPDY/3.1+portforward.k8s.io";

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    pub ports: Option<String>,
}

// Combined handler for both POST (protocol check) and GET (WebSocket upgrade)
pub async fn handle_portforward_all(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    method: Method,
    headers: HeaderMap,
    ws: Option<WebSocketUpgrade>,
) -> Result<Response, StatusCode> {
    info!("Port-forward request: method={:?} for {}/{}", method, namespace, name);
    
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

    match method {
        Method::POST => {
            // POST is used by kubectl to check protocols
            info!("Handling POST - returning protocol info");
            let response = Response::builder()
                .status(StatusCode::OK)
                .header("X-Stream-Protocol-Version", V1_PROTOCOL)
                .body(Body::empty())
                .unwrap();
            Ok(response)
        }
        Method::GET => {
            // GET is used for actual WebSocket upgrade
            if let Some(ws) = ws {
                handle_portforward_ws(ws, state, namespace, name, query, headers).await
            } else {
                Err(StatusCode::BAD_REQUEST)
            }
        }
        _ => {
            warn!("Unsupported method: {:?}", method);
            Err(StatusCode::METHOD_NOT_ALLOWED)
        }
    }
}

// WebSocket handler
pub async fn handle_portforward_ws(
    ws: WebSocketUpgrade,
    state: AppState,
    namespace: String,
    name: String,
    query: PortForwardQuery,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    info!("Port-forward WebSocket upgrade for {}/{}", namespace, name);
    
    for (hname, value) in headers.iter() {
        trace!("Header: {:?} = {:?}", hname, value);
    }

    // Parse ports from query
    let ports: Vec<u16> = if let Some(ports_str) = query.ports {
        ports_str.split(',')
            .filter_map(|p| p.parse::<u16>().ok())
            .collect()
    } else {
        // Get default port from pod spec
        let pod = state.storage.pods().get(&namespace, &name).await
            .map_err(|_| StatusCode::NOT_FOUND)?;
        vec![get_pod_port(&pod)]
    };
    
    if ports.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    info!("Using ports {:?} for forwarding", ports);

    // Accept WebSocket with v1 protocol
    Ok(ws
        .protocols([V1_PROTOCOL, SPDY_PROTOCOL])
        .on_upgrade(move |socket| {
            handle_ws_session(socket, state.container_runtime.clone(), namespace, name, ports)
        }))
}

async fn handle_ws_session(
    socket: WebSocket,
    runtime: Arc<ContainerRuntime>,
    namespace: String,
    pod_name: String,
    ports: Vec<u16>,
) {
    info!("Starting WebSocket session for {}/{} ports {:?}", namespace, pod_name, ports);
    
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    
    // Track active port connections
    let mut port_connections: HashMap<u8, PortConnection> = HashMap::new();
    
    // Set up connection for each port
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
                        send_error_frame(ws_sender.clone(), error_stream_id, 
                            &format!("Failed to create container: {}", e)).await;
                        continue;
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
                send_error_frame(ws_sender.clone(), error_stream_id,
                    &format!("Failed to connect to port {}: {}", port, e)).await;
                continue;
            }
        };
        
        let (tcp_reader, tcp_writer) = tcp_stream.into_split();
        
        // Create channels for this port
        let (to_container_tx, to_container_rx) = mpsc::channel::<Vec<u8>>(100);
        let (from_container_tx, mut from_container_rx) = mpsc::channel::<Vec<u8>>(100);
        
        // Store connection info
        port_connections.insert(data_stream_id, PortConnection {
            port,
            data_stream_id,
            error_stream_id,
            to_container: to_container_tx.clone(),
        });
        
        // Send initial empty frames to acknowledge streams (important!)
        send_empty_frame(ws_sender.clone(), data_stream_id).await;
        send_empty_frame(ws_sender.clone(), error_stream_id).await;
        
        // Task: Forward data from container to WebSocket
        let ws_sender_clone = ws_sender.clone();
        tokio::spawn(async move {
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
        tokio::spawn(async move {
            let mut rx = to_container_rx;
            while let Some(data) = rx.recv().await {
                trace!("Writing {} bytes to container port {}", data.len(), port);
                let mut writer = tcp_writer.lock().await;
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
        tokio::spawn(async move {
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
                
                // Log first few messages for debugging
                if message_count <= 10 {
                    info!("Message {}: {} bytes, first bytes: {:02x?}", 
                        message_count, data.len(), &data[..data.len().min(20)]);
                }
                
                // Parse stream ID from first byte
                let stream_id = data[0];
                
                // Check if this is a data stream we're handling
                if let Some(conn) = port_connections.get(&stream_id) {
                    if data.len() > 1 {
                        // Forward data to container
                        let payload = &data[1..];
                        trace!("Data for port {} (stream {}): {} bytes", 
                            conn.port, stream_id, payload.len());
                        
                        if conn.to_container.send(payload.to_vec()).await.is_err() {
                            debug!("Failed to queue data for port {}", conn.port);
                        }
                    } else {
                        // Empty data frame - acknowledgment
                        trace!("Empty frame for stream {} (port {})", stream_id, conn.port);
                    }
                } else if stream_id % 2 == 1 {
                    // Odd stream IDs are error streams
                    if data.len() > 1 {
                        let error_msg = String::from_utf8_lossy(&data[1..]);
                        debug!("Error stream {}: {}", stream_id, error_msg);
                    } else {
                        trace!("Empty error stream frame for stream {}", stream_id);
                    }
                } else {
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
    
    info!("WebSocket session ended after {} messages", message_count);
}

struct PortConnection {
    port: u16,
    data_stream_id: u8,
    error_stream_id: u8,
    to_container: mpsc::Sender<Vec<u8>>,
}

async fn send_empty_frame(
    ws_sender: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>,
    stream_id: u8,
) {
    let frame = vec![stream_id];
    let mut sender = ws_sender.lock().await;
    let _ = sender.send(axum::extract::ws::Message::Binary(frame)).await;
}

async fn send_error_frame(
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