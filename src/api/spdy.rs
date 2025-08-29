// SPDY/3.1 protocol implementation for kubectl port-forward
// kubectl uses SPDY/3.1, not WebSockets, for port-forwarding

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, Request, Response, StatusCode},
    response::IntoResponse,
};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

use super::server::AppState;

// SPDY frame types
const DATA_FRAME: u8 = 0x00;
const SYN_STREAM: u8 = 0x01;
const SYN_REPLY: u8 = 0x02;
const RST_STREAM: u8 = 0x03;
const SETTINGS: u8 = 0x04;
const PING: u8 = 0x06;
const GOAWAY: u8 = 0x07;
const HEADERS: u8 = 0x08;
const WINDOW_UPDATE: u8 = 0x09;

// Stream IDs for port-forwarding
const ERROR_STREAM_ID: u32 = 0;
const STDIN_STREAM_ID: u32 = 1;
const STDOUT_STREAM_ID: u32 = 2;

#[derive(Debug, Clone)]
pub struct SpdyFrame {
    pub stream_id: u32,
    pub flags: u8,
    pub data: Vec<u8>,
}

impl SpdyFrame {
    pub fn new_data(stream_id: u32, data: Vec<u8>) -> Self {
        Self {
            stream_id,
            flags: 0,
            data,
        }
    }

    pub fn new_error(error_msg: String) -> Self {
        Self {
            stream_id: ERROR_STREAM_ID,
            flags: 0,
            data: error_msg.into_bytes(),
        }
    }

    // Serialize frame to bytes for sending
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        
        // Data frame format (simplified for port-forward):
        // [stream_id:4][flags:1][length:3][data:length]
        buf.put_u32(self.stream_id);
        buf.put_u8(self.flags);
        buf.put_u8(((self.data.len() >> 16) & 0xFF) as u8);
        buf.put_u16((self.data.len() & 0xFFFF) as u16);
        buf.extend_from_slice(&self.data);
        
        buf
    }

    // Parse frame from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < 8 {
            return Err("Frame too short".to_string());
        }

        let mut cursor = &data[..];
        let stream_id = cursor.get_u32();
        let flags = cursor.get_u8();
        let len_high = cursor.get_u8() as usize;
        let len_low = cursor.get_u16() as usize;
        let length = (len_high << 16) | len_low;

        if data.len() < 8 + length {
            return Err("Frame data incomplete".to_string());
        }

        let frame_data = data[8..8 + length].to_vec();

        Ok(SpdyFrame {
            stream_id,
            flags,
            data: frame_data,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    pub ports: Option<String>,
}

#[derive(Debug, Clone)]
struct PortMapping {
    local_port: u16,
    remote_port: u16,
    stream_base: u32,  // Base stream ID for this port
}

pub async fn spdy_portforward_handler(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    req: Request<Body>,
) -> Result<Response<Body>, StatusCode> {
    let headers = req.headers();
    
    info!("SPDY port-forward request for pod {}/{}", namespace, name);
    debug!("Query params: {:?}", query);
    debug!("Headers: {:?}", headers);
    
    // Check for SPDY upgrade
    let upgrade_header = headers
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    
    if !upgrade_header.contains("SPDY/3.1") {
        warn!("Missing SPDY/3.1 upgrade header");
        return Err(StatusCode::BAD_REQUEST);
    }

    // Parse ports
    let ports = parse_port_mappings(&query.ports.unwrap_or_default());
    if ports.is_empty() {
        warn!("No ports specified");
        return Err(StatusCode::BAD_REQUEST);
    }

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

    // Get container connection info
    let container = get_container_connection(&namespace, &name)
        .await
        .ok_or_else(|| {
            error!("Failed to get container info");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!("Setting up SPDY port-forward for pod {}/{} with ports: {:?}", 
         namespace, name, ports);

    // Create response with upgrade
    let response = Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::UPGRADE, "SPDY/3.1")
        .header(header::CONNECTION, "Upgrade")
        .body(Body::empty())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Spawn handler for the upgraded connection
    tokio::spawn(handle_spdy_connection(container, ports));

    Ok(response)
}

fn parse_port_mappings(ports_str: &str) -> Vec<PortMapping> {
    let mut mappings = Vec::new();
    let mut stream_base = 0u32;
    
    for port_spec in ports_str.split(',') {
        let parts: Vec<&str> = port_spec.split(':').collect();
        let mapping = match parts.len() {
            1 => {
                if let Ok(port) = parts[0].parse::<u16>() {
                    Some(PortMapping {
                        local_port: port,
                        remote_port: port,
                        stream_base,
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
                        stream_base,
                    })
                } else {
                    None
                }
            }
            _ => None,
        };
        
        if let Some(mapping) = mapping {
            mappings.push(mapping);
            stream_base += 2; // Each port uses 2 streams (data + error)
        }
    }
    
    mappings
}

#[derive(Clone)]
struct ContainerConnection {
    ip: String,
    namespace: String,
    pod_name: String,
}

async fn get_container_connection(namespace: &str, name: &str) -> Option<ContainerConnection> {
    let docker = bollard::Docker::connect_with_local_defaults().ok()?;
    
    // Find container by pod labels
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
    let container_id = container.id.clone()?;
    
    // Get container IP
    let inspect = docker.inspect_container(&container_id, None).await.ok()?;
    let ip = inspect
        .network_settings?
        .networks?
        .get("bridge")?
        .ip_address
        .clone()?;
    
    Some(ContainerConnection {
        ip,
        namespace: namespace.to_string(),
        pod_name: name.to_string(),
    })
}

async fn handle_spdy_connection(
    container: ContainerConnection,
    ports: Vec<PortMapping>,
) {
    info!("Handling SPDY connection for {}/{}", container.namespace, container.pod_name);
    
    // For each port, create TCP connection to container
    for port_mapping in ports {
        let container_ip = container.ip.clone();
        tokio::spawn(handle_port_stream(port_mapping, container_ip));
    }
}

async fn handle_port_stream(port_mapping: PortMapping, container_ip: String) {
    let endpoint = format!("{}:{}", container_ip, port_mapping.remote_port);
    
    match TcpStream::connect(&endpoint).await {
        Ok(mut stream) => {
            info!("Connected to container port {}", port_mapping.remote_port);
            
            // Handle bidirectional streaming
            let (mut read_half, mut write_half) = tokio::io::split(stream);
            
            // Read from container and send SPDY frames
            let read_task = tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                loop {
                    match read_half.read(&mut buffer).await {
                        Ok(0) => break,
                        Ok(n) => {
                            let frame = SpdyFrame::new_data(
                                port_mapping.stream_base + STDOUT_STREAM_ID,
                                buffer[..n].to_vec(),
                            );
                            // Send frame to client (would need connection handle here)
                            debug!("Read {} bytes from container", n);
                        }
                        Err(e) => {
                            error!("Read error: {}", e);
                            break;
                        }
                    }
                }
            });
            
            // Write to container from SPDY frames
            let write_task = tokio::spawn(async move {
                // Receive frames and write to container (would need receiver here)
                debug!("Write task started for port {}", port_mapping.remote_port);
            });
            
            tokio::select! {
                _ = read_task => {},
                _ = write_task => {},
            }
        }
        Err(e) => {
            error!("Failed to connect to container port {}: {}", port_mapping.remote_port, e);
            let error_frame = SpdyFrame::new_error(format!("Connection failed: {}", e));
            // Send error frame to client
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spdy_frame_serialization() {
        let frame = SpdyFrame::new_data(1, b"test data".to_vec());
        let bytes = frame.to_bytes();
        
        assert_eq!(bytes.len(), 8 + 9); // Header + data
        assert_eq!(&bytes[0..4], &[0, 0, 0, 1]); // Stream ID
        assert_eq!(bytes[4], 0); // Flags
        assert_eq!(&bytes[5..8], &[0, 0, 9]); // Length
        assert_eq!(&bytes[8..], b"test data");
    }

    #[test]
    fn test_spdy_frame_deserialization() {
        let data = vec![
            0, 0, 0, 2,  // Stream ID = 2
            0,            // Flags = 0
            0, 0, 5,      // Length = 5
            b'h', b'e', b'l', b'l', b'o',  // Data
        ];
        
        let frame = SpdyFrame::from_bytes(&data).unwrap();
        assert_eq!(frame.stream_id, 2);
        assert_eq!(frame.flags, 0);
        assert_eq!(frame.data, b"hello");
    }

    #[test]
    fn test_spdy_frame_error() {
        let error_frame = SpdyFrame::new_error("Test error".to_string());
        assert_eq!(error_frame.stream_id, ERROR_STREAM_ID);
        assert_eq!(error_frame.data, b"Test error");
    }

    #[test]
    fn test_parse_port_mappings() {
        // Single port
        let ports = parse_port_mappings("8080");
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].local_port, 8080);
        assert_eq!(ports[0].remote_port, 8080);
        assert_eq!(ports[0].stream_base, 0);

        // Port mapping
        let ports = parse_port_mappings("8080:80");
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].local_port, 8080);
        assert_eq!(ports[0].remote_port, 80);

        // Multiple ports
        let ports = parse_port_mappings("8080:80,9090:90");
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].local_port, 8080);
        assert_eq!(ports[0].remote_port, 80);
        assert_eq!(ports[0].stream_base, 0);
        assert_eq!(ports[1].local_port, 9090);
        assert_eq!(ports[1].remote_port, 90);
        assert_eq!(ports[1].stream_base, 2);

        // Invalid port
        let ports = parse_port_mappings("invalid");
        assert_eq!(ports.len(), 0);
    }

    #[test]
    fn test_frame_too_short() {
        let data = vec![0, 0, 0, 1]; // Only 4 bytes
        let result = SpdyFrame::from_bytes(&data);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Frame too short");
    }

    #[test]
    fn test_frame_incomplete_data() {
        let data = vec![
            0, 0, 0, 1,  // Stream ID
            0,            // Flags
            0, 0, 10,     // Length = 10
            b'h', b'i',   // Only 2 bytes of data, but length says 10
        ];
        let result = SpdyFrame::from_bytes(&data);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Frame data incomplete");
    }

    #[tokio::test]
    async fn test_port_mapping_stream_assignment() {
        let ports = parse_port_mappings("8080,8081,8082");
        assert_eq!(ports.len(), 3);
        
        // Each port should get unique stream IDs
        assert_eq!(ports[0].stream_base, 0);
        assert_eq!(ports[1].stream_base, 2);
        assert_eq!(ports[2].stream_base, 4);
    }
}