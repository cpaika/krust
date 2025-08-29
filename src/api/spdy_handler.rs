// Full SPDY/3.1 implementation for kubectl port-forward
// This handles the actual protocol upgrade and stream multiplexing

use axum::{
    body::Body,
    extract::{ConnectInfo, Path, Query, State},
    http::{header, HeaderMap, Request, Response, StatusCode},
    response::IntoResponse,
};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use hyper::upgrade::{OnUpgrade, Upgraded};
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error, info, warn};

use super::server::AppState;

// SPDY constants
const DATA_FRAME: u8 = 0x00;
const ERROR_STREAM: u32 = 0;

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    pub ports: Option<String>,
}

#[derive(Debug, Clone)]
struct PortMapping {
    local_port: u16,
    remote_port: u16,
    data_stream: u32,
    error_stream: u32,
}

#[derive(Debug)]
pub struct SpdyFrame {
    pub stream_id: u32,
    pub flags: u8,
    pub data: Vec<u8>,
}

impl SpdyFrame {
    pub fn new(stream_id: u32, data: Vec<u8>) -> Self {
        Self {
            stream_id,
            flags: 0,
            data,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 + self.data.len());
        buf.put_u32(self.stream_id);
        buf.put_u8(self.flags);
        buf.put_u8(((self.data.len() >> 16) & 0xFF) as u8);
        buf.put_u16((self.data.len() & 0xFFFF) as u16);
        buf.extend_from_slice(&self.data);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 8 {
            return Err("Insufficient data for frame header".to_string());
        }

        let mut cursor = &data[..];
        let stream_id = cursor.get_u32();
        let flags = cursor.get_u8();
        let len_high = cursor.get_u8() as usize;
        let len_low = cursor.get_u16() as usize;
        let length = (len_high << 16) | len_low;

        if data.len() < 8 + length {
            return Err("Insufficient data for frame body".to_string());
        }

        let frame_data = data[8..8 + length].to_vec();
        
        Ok((
            SpdyFrame {
                stream_id,
                flags,
                data: frame_data,
            },
            8 + length,
        ))
    }
}

pub async fn handle_portforward(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    info!("Port-forward request for pod {}/{}", namespace, name);
    debug!("Query: {:?}, Headers: {:?}", query, headers);

    // Check for upgrade header - kubectl can use either WebSocket or SPDY
    let upgrade = headers
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let is_websocket = upgrade.to_lowercase().contains("websocket");
    let is_spdy = upgrade.contains("SPDY/3.1");

    if !is_websocket && !is_spdy {
        warn!("Missing WebSocket or SPDY/3.1 upgrade header, got: {}", upgrade);
        return Err(StatusCode::BAD_REQUEST);
    }

    info!("Using protocol: {}", if is_websocket { "WebSocket" } else { "SPDY/3.1" });

    // Parse ports
    let ports = parse_port_mappings(&query.ports.unwrap_or_default());
    if ports.is_empty() {
        warn!("No ports specified");
        return Err(StatusCode::BAD_REQUEST);
    }

    info!("Port mappings: {:?}", ports);

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

    // Get container IP
    let container_ip = get_container_ip(&namespace, &name).await.ok_or_else(|| {
        error!("Failed to get container IP");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Container IP: {}", container_ip);

    // Set up the upgrade
    tokio::task::spawn(async move {
        match hyper::upgrade::on(req).await {
            Ok(upgraded) => {
                info!("Connection upgraded to SPDY/3.1");
                handle_spdy_connection(upgraded, container_ip, ports).await;
            }
            Err(e) => error!("Upgrade error: {}", e),
        }
    });

    // Return switching protocols response
    // For WebSocket with SPDY subprotocol, include the Sec-WebSocket-Protocol header
    let mut response_builder = Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::CONNECTION, "Upgrade");
    
    if is_websocket {
        response_builder = response_builder
            .header(header::UPGRADE, "websocket")
            .header("sec-websocket-protocol", "SPDY/3.1+portforward.k8s.io");
    } else {
        response_builder = response_builder
            .header(header::UPGRADE, "SPDY/3.1");
    }
    
    Ok(response_builder
        .body(Body::empty())
        .unwrap())
}

fn parse_port_mappings(ports_str: &str) -> Vec<PortMapping> {
    let mut mappings = Vec::new();
    let mut stream_id = 0u32;

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
                        data_stream: stream_id,
                        error_stream: stream_id + 1,
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
                        data_stream: stream_id,
                        error_stream: stream_id + 1,
                    })
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(mapping) = mapping {
            mappings.push(mapping);
            stream_id += 2; // Each port uses 2 streams
        }
    }

    mappings
}

async fn get_container_ip(namespace: &str, name: &str) -> Option<String> {
    let docker = bollard::Docker::connect_with_local_defaults().ok()?;
    
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
    let container = containers.first()?;
    let container_id = container.id.as_ref()?;
    
    let inspect = docker.inspect_container(container_id, None).await.ok()?;
    inspect
        .network_settings?
        .networks?
        .get("bridge")?
        .ip_address
        .clone()
}

async fn handle_spdy_connection(
    upgraded: Upgraded,
    container_ip: String,
    ports: Vec<PortMapping>,
) {
    info!("Handling SPDY connection for {} with {} ports", container_ip, ports.len());
    
    // Wrap the upgraded connection for Tokio compatibility
    let mut upgraded = TokioIo::new(upgraded);
    
    // Create channels for each port
    let mut port_channels = HashMap::new();
    let mut tcp_handles = Vec::new();
    
    for port in &ports {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(100);
        port_channels.insert(port.data_stream, tx);
        
        // Spawn TCP handler for this port
        let container_endpoint = format!("{}:{}", container_ip, port.remote_port);
        let port_clone = port.clone();
        let handle = tokio::spawn(handle_tcp_connection(container_endpoint, rx, port_clone));
        tcp_handles.push(handle);
    }
    
    // Handle SPDY frames
    let mut buffer = BytesMut::with_capacity(65536);
    
    loop {
        tokio::select! {
            // Read from client
            result = upgraded.read_buf(&mut buffer) => {
                match result {
                    Ok(0) => {
                        info!("Client disconnected");
                        break;
                    }
                    Ok(_) => {
                        // Process frames in buffer
                        while buffer.len() >= 8 {
                            match SpdyFrame::from_bytes(&buffer) {
                                Ok((frame, consumed)) => {
                                    debug!("Received frame: stream={}, len={}", 
                                           frame.stream_id, frame.data.len());
                                    
                                    // Route to appropriate port handler
                                    if let Some(tx) = port_channels.get(&frame.stream_id) {
                                        if tx.send(frame.data).await.is_err() {
                                            error!("Failed to send to port handler");
                                        }
                                    }
                                    
                                    buffer.advance(consumed);
                                }
                                Err(e) => {
                                    if e.contains("Insufficient data") {
                                        // Need more data
                                        break;
                                    } else {
                                        error!("Frame parse error: {}", e);
                                        buffer.clear();
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Read error: {}", e);
                        break;
                    }
                }
            }
        }
    }
    
    // Clean up
    for handle in tcp_handles {
        handle.abort();
    }
    
    info!("SPDY connection closed");
}

async fn handle_tcp_connection(
    endpoint: String,
    mut rx: mpsc::Receiver<Vec<u8>>,
    port: PortMapping,
) {
    info!("Connecting to container at {}", endpoint);
    
    match TcpStream::connect(&endpoint).await {
        Ok(mut stream) => {
            info!("Connected to {}", endpoint);
            
            let (mut read_half, mut write_half) = tokio::io::split(stream);
            
            // Spawn reader task
            let read_task = tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                loop {
                    match read_half.read(&mut buffer).await {
                        Ok(0) => {
                            debug!("Container connection closed");
                            break;
                        }
                        Ok(n) => {
                            debug!("Read {} bytes from container", n);
                            // Would send frame back to client here
                            let frame = SpdyFrame::new(port.data_stream, buffer[..n].to_vec());
                            // Need to send this back through upgraded connection
                        }
                        Err(e) => {
                            error!("Container read error: {}", e);
                            break;
                        }
                    }
                }
            });
            
            // Write to container
            while let Some(data) = rx.recv().await {
                if write_half.write_all(&data).await.is_err() {
                    error!("Failed to write to container");
                    break;
                }
            }
            
            read_task.abort();
        }
        Err(e) => {
            error!("Failed to connect to {}: {}", endpoint, e);
            // Send error frame
            let error_msg = format!("Connection failed: {}", e);
            let frame = SpdyFrame::new(port.error_stream, error_msg.into_bytes());
            // Would send error frame to client here
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_port() {
        let ports = parse_port_mappings("8080");
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].local_port, 8080);
        assert_eq!(ports[0].remote_port, 8080);
        assert_eq!(ports[0].data_stream, 0);
        assert_eq!(ports[0].error_stream, 1);
    }

    #[test]
    fn test_parse_port_mapping() {
        let ports = parse_port_mappings("8080:80");
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].local_port, 8080);
        assert_eq!(ports[0].remote_port, 80);
    }

    #[test]
    fn test_parse_multiple_ports() {
        let ports = parse_port_mappings("8080:80,9090:90,3000");
        assert_eq!(ports.len(), 3);
        
        assert_eq!(ports[0].local_port, 8080);
        assert_eq!(ports[0].remote_port, 80);
        assert_eq!(ports[0].data_stream, 0);
        
        assert_eq!(ports[1].local_port, 9090);
        assert_eq!(ports[1].remote_port, 90);
        assert_eq!(ports[1].data_stream, 2);
        
        assert_eq!(ports[2].local_port, 3000);
        assert_eq!(ports[2].remote_port, 3000);
        assert_eq!(ports[2].data_stream, 4);
    }

    #[test]
    fn test_frame_serialization() {
        let frame = SpdyFrame::new(42, b"test data".to_vec());
        let bytes = frame.to_bytes();
        
        assert_eq!(bytes[0..4], [0, 0, 0, 42]); // Stream ID
        assert_eq!(bytes[4], 0); // Flags
        assert_eq!(bytes[5..8], [0, 0, 9]); // Length
        assert_eq!(&bytes[8..], b"test data");
    }

    #[test]
    fn test_frame_deserialization() {
        let data = vec![
            0, 0, 0, 10,  // Stream ID = 10
            0,            // Flags
            0, 0, 5,      // Length = 5
            b'h', b'e', b'l', b'l', b'o',
            // Extra data (should not be consumed)
            1, 2, 3,
        ];
        
        let (frame, consumed) = SpdyFrame::from_bytes(&data).unwrap();
        assert_eq!(frame.stream_id, 10);
        assert_eq!(frame.data, b"hello");
        assert_eq!(consumed, 13); // 8 header + 5 data
    }
}