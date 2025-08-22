// Simple TCP proxy server for port forwarding
// This runs as a separate task and proxies connections to containers

use bollard::Docker;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tracing::{error, info};

pub struct PortForwardProxy {
    forwards: Arc<RwLock<HashMap<String, ForwardInfo>>>,
}

#[derive(Clone)]
struct ForwardInfo {
    pod_name: String,
    namespace: String,
    container_ip: String,
    local_port: u16,
    remote_port: u16,
}

impl PortForwardProxy {
    pub fn new() -> Self {
        Self {
            forwards: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start_forward(
        &self,
        namespace: &str,
        pod_name: &str,
        local_port: u16,
        remote_port: u16,
    ) -> Result<(), String> {
        // Get container IP
        let container_ip = get_pod_container_ip(namespace, pod_name)
            .await
            .ok_or_else(|| format!("Failed to get container IP for pod {}/{}", namespace, pod_name))?;

        let forward_id = format!("{}/{}/{}:{}", namespace, pod_name, local_port, remote_port);
        
        // Check if already forwarding
        {
            let forwards = self.forwards.read().await;
            if forwards.contains_key(&forward_id) {
                return Ok(()); // Already forwarding
            }
        }

        // Store forward info
        let info = ForwardInfo {
            pod_name: pod_name.to_string(),
            namespace: namespace.to_string(),
            container_ip: container_ip.clone(),
            local_port,
            remote_port,
        };

        {
            let mut forwards = self.forwards.write().await;
            forwards.insert(forward_id.clone(), info.clone());
        }

        // Start proxy in background
        let forwards = self.forwards.clone();
        tokio::spawn(async move {
            if let Err(e) = run_proxy(info).await {
                error!("Proxy failed: {}", e);
            }
            // Remove from active forwards
            let mut forwards = forwards.write().await;
            forwards.remove(&forward_id);
        });

        Ok(())
    }

    pub async fn stop_forward(
        &self,
        namespace: &str,
        pod_name: &str,
        local_port: u16,
        remote_port: u16,
    ) -> Result<(), String> {
        let forward_id = format!("{}/{}/{}:{}", namespace, pod_name, local_port, remote_port);
        let mut forwards = self.forwards.write().await;
        forwards.remove(&forward_id);
        Ok(())
    }

    pub async fn list_forwards(&self) -> Vec<String> {
        let forwards = self.forwards.read().await;
        forwards.keys().cloned().collect()
    }
}

async fn get_pod_container_ip(namespace: &str, name: &str) -> Option<String> {
    let docker = Docker::connect_with_local_defaults().ok()?;
    
    // Find container by pod labels
    let pod_label = format!("io.kubernetes.pod.name={}", name);
    let namespace_label = format!("io.kubernetes.pod.namespace={}", namespace);
    
    let mut filter_map = std::collections::HashMap::new();
    filter_map.entry("label".to_string()).or_insert_with(Vec::new).push(pod_label);
    filter_map.entry("label".to_string()).or_insert_with(Vec::new).push(namespace_label);
    
    let options = bollard::container::ListContainersOptions {
        all: true,
        filters: filter_map,
        ..Default::default()
    };
    
    let containers = docker.list_containers(Some(options)).await.ok()?;
    let container = containers.first()?;
    let container_id = container.id.as_ref()?;
    
    // Get container IP
    let inspect = docker.inspect_container(container_id, None).await.ok()?;
    inspect
        .network_settings?
        .networks?
        .get("bridge")?
        .ip_address
        .clone()
}

async fn run_proxy(info: ForwardInfo) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", info.local_port)).await?;
    info!(
        "Port forwarding: localhost:{} -> {}/{}:{}:{}",
        info.local_port, info.namespace, info.pod_name, info.container_ip, info.remote_port
    );

    loop {
        let (local_stream, addr) = listener.accept().await?;
        let container_endpoint = format!("{}:{}", info.container_ip, info.remote_port);
        
        info!("New connection from {} to {}", addr, container_endpoint);
        
        tokio::spawn(async move {
            if let Err(e) = proxy_connection(local_stream, &container_endpoint).await {
                error!("Proxy connection error: {}", e);
            }
        });
    }
}

async fn proxy_connection(
    local: TcpStream,
    container_endpoint: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let remote = match TcpStream::connect(container_endpoint).await {
        Ok(stream) => stream,
        Err(e) => {
            error!("Failed to connect to container: {}", e);
            return Err(Box::new(e));
        }
    };

    let (mut local_read, mut local_write) = local.into_split();
    let (mut remote_read, mut remote_write) = remote.into_split();

    // Create two tasks for bidirectional copying
    let client_to_container = tokio::spawn(async move {
        let mut buf = vec![0; 8192];
        loop {
            match local_read.read(&mut buf).await {
                Ok(0) => break, // Connection closed
                Ok(n) => {
                    if let Err(e) = remote_write.write_all(&buf[..n]).await {
                        error!("Failed to write to container: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to read from client: {}", e);
                    break;
                }
            }
        }
    });

    let container_to_client = tokio::spawn(async move {
        let mut buf = vec![0; 8192];
        loop {
            match remote_read.read(&mut buf).await {
                Ok(0) => break, // Connection closed
                Ok(n) => {
                    if let Err(e) = local_write.write_all(&buf[..n]).await {
                        error!("Failed to write to client: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to read from container: {}", e);
                    break;
                }
            }
        }
    });

    // Wait for either direction to complete
    tokio::select! {
        _ = client_to_container => {},
        _ = container_to_client => {},
    }

    Ok(())
}