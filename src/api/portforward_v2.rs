#![allow(unused_imports)]
use axum::{
    extract::{ws::WebSocket, Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bollard::Docker;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use super::server::AppState;

#[derive(Debug, Deserialize)]
pub struct PortForwardQuery {
    ports: Option<String>,
}

pub async fn pod_portforward(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(query): Query<PortForwardQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    // Verify pod exists and is running
    let pod = state
        .storage
        .pods()
        .get(&namespace, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let status = pod
        .get("status")
        .and_then(|s| s.as_object())
        .ok_or(StatusCode::CONFLICT)?;
    
    let phase = status
        .get("phase")
        .and_then(|p| p.as_str())
        .ok_or(StatusCode::CONFLICT)?;
    
    if phase != "Running" {
        return Err(StatusCode::CONFLICT);
    }

    // Parse ports from query parameter
    let ports = parse_ports(&query.ports.unwrap_or_default());
    if ports.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    info!("Setting up port-forward for pod {}/{} with ports {:?}", namespace, name, ports);

    // Get container info
    let container_info = get_container_info(&namespace, &name).await
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(ws.on_upgrade(move |socket| {
        handle_port_forward(socket, container_info, ports)
    }))
}

fn parse_ports(ports_str: &str) -> Vec<(u16, u16)> {
    // Parse port specification: "8080" or "8080:80" or "8080,9090:90"
    ports_str
        .split(',')
        .filter_map(|p| {
            let parts: Vec<&str> = p.split(':').collect();
            match parts.len() {
                1 => parts[0].parse::<u16>().ok().map(|port| (port, port)),
                2 => {
                    let local = parts[0].parse::<u16>().ok()?;
                    let remote = parts[1].parse::<u16>().ok()?;
                    Some((local, remote))
                }
                _ => None,
            }
        })
        .collect()
}

#[derive(Clone)]
struct ContainerInfo {
    id: String,
    ip: String,
}

async fn get_container_info(namespace: &str, name: &str) -> Option<ContainerInfo> {
    let docker = Docker::connect_with_local_defaults().ok()?;
    
    // Find container by pod labels
    let mut filters = std::collections::HashMap::new();
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
    
    Some(ContainerInfo {
        id: container_id,
        ip,
    })
}

async fn handle_port_forward(
    ws: WebSocket,
    container_info: ContainerInfo,
    ports: Vec<(u16, u16)>,
) {
    let (mut ws_sender, mut ws_receiver) = ws.split();
    
    // For kubectl port-forward, we need to handle the SPDY protocol
    // But for a simpler implementation, we'll use a basic protocol
    
    // Create TCP proxy for each port
    let mut handles = vec![];
    
    for (local_port, remote_port) in ports {
        let container_ip = container_info.ip.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = proxy_port(local_port, remote_port, &container_ip).await {
                error!("Port proxy failed for {}:{}: {}", local_port, remote_port, e);
            }
        });
        handles.push(handle);
    }
    
    // Keep WebSocket alive
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Close(_)) => {
                info!("Port-forward WebSocket closed");
                break;
            }
            Ok(_) => {
                // Handle other messages if needed
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
    }
    
    // Clean up proxies
    for handle in handles {
        handle.abort();
    }
}

async fn proxy_port(local_port: u16, remote_port: u16, container_ip: &str) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", local_port)).await?;
    info!("Listening on localhost:{} -> {}:{}", local_port, container_ip, remote_port);
    
    loop {
        let (local_stream, _) = listener.accept().await?;
        let container_endpoint = format!("{}:{}", container_ip, remote_port);
        
        tokio::spawn(async move {
            if let Err(e) = handle_connection(local_stream, &container_endpoint).await {
                debug!("Connection handling error: {}", e);
            }
        });
    }
}

async fn handle_connection(
    mut local: TcpStream,
    container_endpoint: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut remote = TcpStream::connect(container_endpoint).await?;
    
    let (mut local_read, mut local_write) = local.split();
    let (mut remote_read, mut remote_write) = remote.split();
    
    // Bidirectional copy
    let client_to_server = tokio::io::copy(&mut local_read, &mut remote_write);
    let server_to_client = tokio::io::copy(&mut remote_read, &mut local_write);
    
    tokio::select! {
        _ = client_to_server => {},
        _ = server_to_client => {},
    }
    
    Ok(())
}