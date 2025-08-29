// Lightweight container runtime for Krust
// Uses Linux namespaces and networking directly for port-forward support
use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener as TokioTcpListener;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Simple container representation for port forwarding
pub struct Container {
    pub id: String,
    pub name: String,
    pub namespace: String,
    pub ports: Vec<ContainerPort>,
    pub services: Arc<RwLock<HashMap<u16, TokioTcpListener>>>,
    pub port_mappings: Arc<RwLock<HashMap<u16, u16>>>,  // container port -> actual host port
}

#[derive(Clone, Debug)]
pub struct ContainerPort {
    pub container_port: u16,
    pub protocol: String,
}

impl Container {
    pub fn new(id: String, name: String, namespace: String) -> Self {
        Self {
            id,
            name,
            namespace,
            ports: Vec::new(),
            services: Arc::new(RwLock::new(HashMap::new())),
            port_mappings: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Start a mock service on a port (for testing)
    pub async fn start_service(&self, port: u16, handler: ServiceHandler) -> Result<(), String> {
        info!("Starting service on port {} for container {}", port, self.id);
        
        // Bind to any available port (0 = let OS assign)
        // This simulates container having its own network namespace
        let listener = TokioTcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Failed to bind port: {}", e))?;
        
        // Get the actual port that was assigned
        let actual_port = listener.local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?
            .port();
        
        info!("Container {} service bound to actual port {} (container port {})", 
              self.id, actual_port, port);
        
        let handler = Arc::new(handler);
        let container_id = self.id.clone();
        
        // Create an Arc for the listener to share between spawn and storage
        let listener = Arc::new(listener);
        let listener_clone = listener.clone();
        
        // Store the port mapping
        {
            let mut mappings = self.port_mappings.write().await;
            mappings.insert(port, actual_port);
        }
        
        // Spawn service handler
        tokio::spawn(async move {
            info!("Service listening on actual port {} (container port {}) for container {}", 
                  actual_port, port, container_id);
            
            loop {
                match listener_clone.accept().await {
                    Ok((stream, addr)) => {
                        debug!("Connection from {} to container {} port {}", addr, container_id, port);
                        let handler = handler.clone();
                        
                        tokio::spawn(async move {
                            if let Err(e) = handler.handle(stream).await {
                                error!("Handler error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Accept error on port {}: {}", port, e);
                    }
                }
            }
        });
        
        // Since we can't store Arc<TcpListener> directly, we'll skip storing it
        // The listener will continue running in the background task
        
        Ok(())
    }
    
    /// Connect to a service port in this container
    pub async fn connect_to_port(&self, port: u16) -> Result<tokio::net::TcpStream, String> {
        info!("Connecting to port {} in container {}", port, self.id);
        
        // Look up the actual host port for this container port
        let mappings = self.port_mappings.read().await;
        let actual_port = mappings.get(&port).copied().unwrap_or(port);
        
        info!("Connecting to actual port {} (container port {}) for container {}", 
              actual_port, port, self.id);
        
        // Connect to the actual host port
        tokio::net::TcpStream::connect(format!("127.0.0.1:{}", actual_port))
            .await
            .map_err(|e| format!("Failed to connect to container port {} (actual port {}): {}", 
                                port, actual_port, e))
    }
}

/// Service handler for mock container services
pub struct ServiceHandler {
    handler_fn: Box<dyn Fn(tokio::net::TcpStream) -> BoxFuture<'static, Result<(), String>> + Send + Sync>,
}

impl ServiceHandler {
    pub fn new<F, Fut>(handler: F) -> Self
    where
        F: Fn(tokio::net::TcpStream) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), String>> + Send + 'static,
    {
        Self {
            handler_fn: Box::new(move |stream| Box::pin(handler(stream))),
        }
    }
    
    pub async fn handle(&self, stream: tokio::net::TcpStream) -> Result<(), String> {
        (self.handler_fn)(stream).await
    }
}

use std::pin::Pin;
use std::future::Future;
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Container runtime manager
pub struct ContainerRuntime {
    containers: Arc<RwLock<HashMap<String, Arc<Container>>>>,
}

impl ContainerRuntime {
    pub fn new() -> Self {
        Self {
            containers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Register a container
    pub async fn register_container(&self, container: Container) -> Arc<Container> {
        let container = Arc::new(container);
        self.containers.write().await.insert(
            format!("{}/{}", container.namespace, container.name),
            container.clone(),
        );
        container
    }
    
    /// Get a container by namespace and name
    pub async fn get_container(&self, namespace: &str, name: &str) -> Option<Arc<Container>> {
        let key = format!("{}/{}", namespace, name);
        self.containers.read().await.get(&key).cloned()
    }
    
    /// Start a default test container with an echo service
    pub async fn start_test_container(&self, namespace: &str, name: &str, port: u16) -> Result<Arc<Container>, String> {
        let container = Container::new(
            format!("{}-{}", namespace, name),
            name.to_string(),
            namespace.to_string(),
        );
        
        // Add an echo service for testing
        let echo_handler = ServiceHandler::new(|mut stream| async move {
            let mut buffer = vec![0; 1024];
            
            loop {
                match stream.read(&mut buffer).await {
                    Ok(0) => break, // Connection closed
                    Ok(n) => {
                        // Echo back with a prefix
                        let response = format!("Echo from container: {}", String::from_utf8_lossy(&buffer[..n]));
                        stream.write_all(response.as_bytes()).await.map_err(|e| e.to_string())?;
                        stream.flush().await.map_err(|e| e.to_string())?;
                    }
                    Err(e) => {
                        error!("Read error: {}", e);
                        break;
                    }
                }
            }
            
            Ok(())
        });
        
        container.start_service(port, echo_handler).await?;
        
        let container = self.register_container(container).await;
        info!("Started test container {}/{} with echo service on port {}", namespace, name, port);
        
        Ok(container)
    }
    
    /// Start an HTTP test container
    pub async fn start_http_container(&self, namespace: &str, name: &str, port: u16) -> Result<Arc<Container>, String> {
        let container = Container::new(
            format!("{}-{}-http", namespace, name),
            name.to_string(),
            namespace.to_string(),
        );
        
        // Add a simple HTTP service
        let http_handler = ServiceHandler::new(move |mut stream| async move {
            let mut buffer = vec![0; 4096];
            
            // Read request
            match stream.read(&mut buffer).await {
                Ok(n) if n > 0 => {
                    let request = String::from_utf8_lossy(&buffer[..n]);
                    debug!("HTTP request: {}", request);
                    
                    // Simple HTTP response
                    let response = format!(
                        "HTTP/1.1 200 OK\r\n\
                        Content-Type: text/plain\r\n\
                        Content-Length: 28\r\n\
                        \r\n\
                        Hello from port-forward test!"
                    );
                    
                    stream.write_all(response.as_bytes()).await.map_err(|e| e.to_string())?;
                    stream.flush().await.map_err(|e| e.to_string())?;
                }
                _ => {}
            }
            
            Ok(())
        });
        
        container.start_service(port, http_handler).await?;
        
        let container = self.register_container(container).await;
        info!("Started HTTP container {}/{} on port {}", namespace, name, port);
        
        Ok(container)
    }
}