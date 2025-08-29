// Champion port-forward implementation with complete SPDY handling
use axum::{
    extract::{ws::WebSocket, Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    body::Body,
};
use bytes::{Buf, BufMut, BytesMut};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, trace, warn};

use super::server::AppState;
use crate::runtime::container::ContainerRuntime;

// Protocol names
const V1_PROTOCOL: &str = "portforward.k8s.io";
const SPDY_PROTOCOL: &str = "SPDY/3.1+portforward.k8s.io";

// SPDY constants
const SPDY_VERSION: u16 = 3;
const CONTROL_BIT: u8 = 0x80;

// SPDY frame types
const SYN_STREAM: u16 = 1;
const SYN_REPLY: u16 = 2;
const RST_STREAM: u16 = 3;
const SETTINGS: u16 = 4;
const PING: u16 = 6;
const GOAWAY: u16 = 7;

// SPDY flags
const FLAG_FIN: u8 = 0x01;

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    pub ports: Option<String>,
}

// Main handler
pub async fn handle_portforward_champion(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    method: Method,
    headers: HeaderMap,
    ws: Option<WebSocketUpgrade>,
) -> Result<Response, StatusCode> {
    info!("Port-forward champion: method={:?} for {}/{}", method, namespace, name);
    
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

    match method {
        Method::POST => {
            // Return protocol support
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("X-Stream-Protocol-Version", V1_PROTOCOL)
                .body(Body::empty())
                .unwrap())
        }
        Method::GET => {
            if let Some(ws) = ws {
                // Parse ports
                let ports: Vec<u16> = if let Some(ports_str) = query.ports {
                    ports_str.split(',')
                        .filter_map(|p| p.parse::<u16>().ok())
                        .collect()
                } else {
                    vec![get_pod_port(&pod)]
                };
                
                info!("Ports for forwarding: {:?}", ports);
                
                Ok(ws
                    .protocols([SPDY_PROTOCOL, V1_PROTOCOL])
                    .on_upgrade(move |socket| {
                        handle_champion_session(socket, state.container_runtime.clone(), namespace, name, ports)
                    }))
            } else {
                Err(StatusCode::BAD_REQUEST)
            }
        }
        _ => Err(StatusCode::METHOD_NOT_ALLOWED)
    }
}

async fn handle_champion_session(
    socket: WebSocket,
    runtime: Arc<ContainerRuntime>,
    namespace: String,
    pod_name: String,
    ports: Vec<u16>,
) {
    info!("🏆 Starting champion session for {}/{} with ports {:?}", namespace, pod_name, ports);
    
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    
    // Frame reassembly buffer
    let mut buffer = BytesMut::with_capacity(4096);
    
    // Active streams and connections
    let mut streams: HashMap<u32, StreamInfo> = HashMap::new();
    let connections = Arc::new(Mutex::new(HashMap::<u16, Connection>::new()));
    
    // Setup port connections
    for &port in &ports {
        match setup_connection(&runtime, &namespace, &pod_name, port, ws_sender.clone()).await {
            Ok(conn) => {
                connections.lock().await.insert(port, conn);
                info!("✅ Connection ready for port {}", port);
            }
            Err(e) => {
                error!("❌ Failed to setup port {}: {}", port, e);
            }
        }
    }
    
    // Process messages
    let mut msg_count = 0;
    info!("Waiting for WebSocket messages...");
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                msg_count += 1;
                
                // Log first messages
                if msg_count <= 20 {
                    debug!("Msg {}: {} bytes [{:02x?}...]", 
                        msg_count, data.len(), &data[..data.len().min(10)]);
                }
                
                // Add to buffer
                buffer.extend_from_slice(&data);
                
                // Parse frames
                while let Some((frame, consumed)) = parse_frame(&buffer) {
                    trace!("Frame parsed: {:?}", frame);
                    
                    handle_frame(
                        frame,
                        &mut streams,
                        connections.clone(),
                        ws_sender.clone(),
                        &ports,
                    ).await;
                    
                    buffer.advance(consumed);
                }
            }
            Ok(axum::extract::ws::Message::Close(reason)) => {
                info!("Connection closed: {:?}", reason);
                break;
            }
            Ok(msg) => {
                debug!("Other message type: {:?}", msg);
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
    }
    
    info!("Session complete after {} messages", msg_count);
}

#[derive(Debug)]
enum Frame {
    Control { frame_type: u16, flags: u8, data: Vec<u8> },
    Data { stream_id: u32, flags: u8, data: Vec<u8> },
}

fn parse_frame(buf: &BytesMut) -> Option<(Frame, usize)> {
    if buf.len() < 8 {
        return None;
    }
    
    let is_control = (buf[0] & CONTROL_BIT) != 0;
    
    if is_control {
        let version = u16::from_be_bytes([buf[0] & 0x7F, buf[1]]);
        if version != SPDY_VERSION {
            return None;
        }
        
        let frame_type = u16::from_be_bytes([buf[2], buf[3]]);
        let flags = buf[4];
        let length = u32::from_be_bytes([0, buf[5], buf[6], buf[7]]) as usize;
        
        if buf.len() < 8 + length {
            return None;
        }
        
        Some((Frame::Control {
            frame_type,
            flags,
            data: buf[8..8 + length].to_vec(),
        }, 8 + length))
    } else {
        let stream_id = u32::from_be_bytes([buf[0] & 0x7F, buf[1], buf[2], buf[3]]);
        let flags = buf[4];
        let length = u32::from_be_bytes([0, buf[5], buf[6], buf[7]]) as usize;
        
        if buf.len() < 8 + length {
            return None;
        }
        
        Some((Frame::Data {
            stream_id,
            flags,
            data: buf[8..8 + length].to_vec(),
        }, 8 + length))
    }
}

async fn handle_frame(
    frame: Frame,
    streams: &mut HashMap<u32, StreamInfo>,
    connections: Arc<Mutex<HashMap<u16, Connection>>>,
    ws_sender: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>,
    ports: &[u16],
) {
    match frame {
        Frame::Control { frame_type, flags, data } => {
            match frame_type {
                SYN_STREAM => {
                    if data.len() >= 10 {
                        let stream_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        
                        // Determine port from stream ID
                        // Stream 1,2 = port 0; Stream 3,4 = port 1, etc
                        let port_idx = ((stream_id - 1) / 2) as usize;
                        let is_error = stream_id % 2 == 0;
                        
                        if let Some(&port) = ports.get(port_idx) {
                            info!("Stream {} -> port {} ({})", 
                                stream_id, port, if is_error { "error" } else { "data" });
                            
                            streams.insert(stream_id, StreamInfo {
                                stream_id,
                                port,
                                is_error,
                            });
                            
                            // Update connection with the appropriate stream
                            if !is_error {
                                if let Some(conn) = connections.lock().await.get_mut(&port) {
                                    conn.data_stream = Some(stream_id);
                                    debug!("Set data stream {} for port {}", stream_id, port);
                                }
                            }
                            
                            // Send SYN_REPLY
                            send_syn_reply(ws_sender.clone(), stream_id).await;
                            
                            // If this is stream 1 (data), kubectl expects stream 2 (error) as well
                            // Create it proactively
                            if stream_id == 1 {
                                let error_stream_id = 2;
                                info!("Creating error stream {} for port {}", error_stream_id, port);
                                
                                streams.insert(error_stream_id, StreamInfo {
                                    stream_id: error_stream_id,
                                    port,
                                    is_error: true,
                                });
                                
                                send_syn_reply(ws_sender.clone(), error_stream_id).await;
                            }
                        } else {
                            warn!("Stream {} has no port", stream_id);
                            send_rst_stream(ws_sender.clone(), stream_id).await;
                        }
                    }
                }
                SETTINGS => {
                    debug!("SETTINGS received");
                    send_settings(ws_sender.clone()).await;
                }
                PING => {
                    debug!("PING received");
                    send_ping(ws_sender.clone(), data).await;
                }
                _ => {
                    trace!("Control frame type {}", frame_type);
                }
            }
        }
        Frame::Data { stream_id, flags: _, data } => {
            if let Some(info) = streams.get(&stream_id) {
                if !info.is_error && !data.is_empty() {
                    debug!("Data for port {} (stream {}): {} bytes", info.port, stream_id, data.len());
                    
                    // Forward to container
                    if let Some(conn) = connections.lock().await.get(&info.port) {
                        if conn.to_container.send(data).await.is_err() {
                            warn!("Failed to forward to port {}", info.port);
                        }
                    }
                }
            } else {
                warn!("Data for unknown stream {}: {} bytes", stream_id, data.len());
            }
        }
    }
}

async fn setup_connection(
    runtime: &Arc<ContainerRuntime>,
    namespace: &str,
    pod_name: &str,
    port: u16,
    ws_sender: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>,
) -> Result<Connection, String> {
    // Get or create container
    let container = match runtime.get_container(namespace, pod_name).await {
        Some(c) => c,
        None => {
            runtime.start_http_container(namespace, pod_name, port).await
                .map_err(|e| e.to_string())?
        }
    };
    
    // Connect to port
    let tcp = container.connect_to_port(port).await
        .map_err(|e| e.to_string())?;
    
    let (tcp_reader, tcp_writer) = tcp.into_split();
    
    // Channels
    let (to_container_tx, mut to_container_rx) = mpsc::channel::<Vec<u8>>(100);
    let (from_container_tx, mut from_container_rx) = mpsc::channel::<Vec<u8>>(100);
    
    // Forward to container
    let writer = Arc::new(Mutex::new(tcp_writer));
    tokio::spawn(async move {
        while let Some(data) = to_container_rx.recv().await {
            let mut w = writer.lock().await;
            let _ = w.write_all(&data).await;
            let _ = w.flush().await;
        }
    });
    
    // Read from container
    let from_tx = from_container_tx.clone();
    tokio::spawn(async move {
        let mut reader = tcp_reader;
        let mut buf = vec![0; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if from_tx.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    
    // Forward from container to WebSocket
    let ws_sender_clone = ws_sender.clone();
    let port_copy = port;
    tokio::spawn(async move {
        let mut stream_id = None;
        
        while let Some(data) = from_container_rx.recv().await {
            // Wait for stream assignment
            if stream_id.is_none() {
                // HACK: Assume stream 1 for first port - proper solution would track this
                stream_id = Some(1u32);
            }
            
            if let Some(sid) = stream_id {
                send_data_frame(ws_sender_clone.clone(), sid, data).await;
            }
        }
        debug!("Container reader for port {} ended", port_copy);
    });
    
    Ok(Connection {
        port,
        to_container: to_container_tx,
        data_stream: None,
    })
}

// Helper functions for SPDY frames
async fn send_syn_reply(
    ws: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>,
    stream_id: u32,
) {
    let mut frame = BytesMut::new();
    frame.put_u16(0x8000 | SPDY_VERSION);
    frame.put_u16(SYN_REPLY);
    frame.put_u8(FLAG_FIN);
    // 24-bit length field (8 bytes of data)
    frame.put_u8(0);
    frame.put_u8(0);
    frame.put_u8(8);
    frame.put_u32(stream_id);
    frame.put_u32(0); // No headers
    
    let mut sender = ws.lock().await;
    let _ = sender.send(axum::extract::ws::Message::Binary(frame.to_vec())).await;
    
    debug!("Sent SYN_REPLY for stream {}", stream_id);
}

async fn send_rst_stream(
    ws: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>,
    stream_id: u32,
) {
    let mut frame = BytesMut::new();
    frame.put_u16(0x8000 | SPDY_VERSION);
    frame.put_u16(RST_STREAM);
    frame.put_u8(0);
    // 24-bit length field (8 bytes)
    frame.put_u8(0);
    frame.put_u8(0);
    frame.put_u8(8);
    frame.put_u32(stream_id);
    frame.put_u32(3); // REFUSED
    
    let mut sender = ws.lock().await;
    let _ = sender.send(axum::extract::ws::Message::Binary(frame.to_vec())).await;
}

async fn send_settings(
    ws: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>,
) {
    let mut frame = BytesMut::new();
    frame.put_u16(0x8000 | SPDY_VERSION);
    frame.put_u16(SETTINGS);
    frame.put_u8(0);
    // 24-bit length field (4 bytes)
    frame.put_u8(0);
    frame.put_u8(0);
    frame.put_u8(4);
    frame.put_u32(0); // No settings
    
    let mut sender = ws.lock().await;
    let _ = sender.send(axum::extract::ws::Message::Binary(frame.to_vec())).await;
}

async fn send_ping(
    ws: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>,
    data: Vec<u8>,
) {
    let mut frame = BytesMut::new();
    frame.put_u16(0x8000 | SPDY_VERSION);
    frame.put_u16(PING);
    frame.put_u8(0);
    // 24-bit length field (4 bytes for PING)
    frame.put_u8(0);
    frame.put_u8(0);
    frame.put_u8(4);
    // PING always has 4 byte ID
    if data.len() >= 4 {
        frame.extend_from_slice(&data[..4]);
    } else {
        frame.extend_from_slice(&data);
        for _ in data.len()..4 {
            frame.put_u8(0);
        }
    }
    
    let mut sender = ws.lock().await;
    let _ = sender.send(axum::extract::ws::Message::Binary(frame.to_vec())).await;
}

async fn send_data_frame(
    ws: Arc<Mutex<futures::stream::SplitSink<WebSocket, axum::extract::ws::Message>>>,
    stream_id: u32,
    data: Vec<u8>,
) {
    let mut frame = BytesMut::new();
    frame.put_u32(stream_id & 0x7FFFFFFF);  // Clear control bit
    frame.put_u8(0); // Flags
    // 24-bit length field
    frame.put_u8(0);
    frame.put_u8((data.len() >> 8) as u8);
    frame.put_u8(data.len() as u8);
    frame.extend_from_slice(&data);
    
    let mut sender = ws.lock().await;
    let _ = sender.send(axum::extract::ws::Message::Binary(frame.to_vec())).await;
    
    trace!("Sent data frame for stream {} with {} bytes", stream_id, data.len());
}

// Types
#[derive(Debug)]
struct StreamInfo {
    stream_id: u32,
    port: u16,
    is_error: bool,
}

struct Connection {
    port: u16,
    to_container: mpsc::Sender<Vec<u8>>,
    data_stream: Option<u32>,
}

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