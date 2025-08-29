// Full container runtime implementation with namespace isolation
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::process::Command;
use tracing::{debug, error, info};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    pub image: String,
    pub command: Vec<String>,
    pub env: HashMap<String, String>,
    pub ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMount>,
    pub resources: ResourceLimits,
    pub hostname: String,
    pub network_mode: NetworkMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub container_port: u16,
    pub host_port: u16,
    pub protocol: String, // tcp/udp
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    pub host_path: String,
    pub container_path: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub memory_mb: Option<u64>,
    pub cpu_shares: Option<u64>,
    pub cpu_quota: Option<u64>,
    pub pids_limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMode {
    Bridge,
    Host,
    None,
    Container(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContainerState {
    Created,
    Running,
    Paused,
    Stopped,
    Removing,
}

#[derive(Debug, Clone)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub config: ContainerConfig,
    pub state: ContainerState,
    pub pid: Option<u32>,
    pub rootfs: PathBuf,
    pub ip_address: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct ContainerRuntime {
    containers: Arc<RwLock<HashMap<String, Arc<Container>>>>,
    runtime_dir: PathBuf,
    network_manager: Arc<NetworkManager>,
    storage_manager: Arc<StorageManager>,
}

impl ContainerRuntime {
    pub fn new() -> Result<Self, String> {
        let runtime_dir = PathBuf::from("/var/lib/krust");
        fs::create_dir_all(&runtime_dir).map_err(|e| format!("Failed to create runtime dir: {}", e))?;
        
        Ok(Self {
            containers: Arc::new(RwLock::new(HashMap::new())),
            runtime_dir,
            network_manager: Arc::new(NetworkManager::new()?),
            storage_manager: Arc::new(StorageManager::new()?),
        })
    }
    
    pub async fn create_container(&self, name: String, config: ContainerConfig) -> Result<Arc<Container>, String> {
        info!("Creating container {} with image {}", name, config.image);
        
        let id = Uuid::new_v4().to_string();
        let container_dir = self.runtime_dir.join(&id);
        fs::create_dir_all(&container_dir).map_err(|e| e.to_string())?;
        
        // Prepare rootfs
        let rootfs = container_dir.join("rootfs");
        self.storage_manager.prepare_rootfs(&config.image, &rootfs).await?;
        
        // Create container struct
        let container = Arc::new(Container {
            id: id.clone(),
            name: name.clone(),
            config,
            state: ContainerState::Created,
            pid: None,
            rootfs,
            ip_address: None,
            created_at: chrono::Utc::now(),
            started_at: None,
        });
        
        // Store container
        self.containers.write().await.insert(id.clone(), container.clone());
        
        info!("Container {} created successfully", name);
        Ok(container)
    }
    
    pub async fn start_container(&self, id: &str) -> Result<(), String> {
        let containers = self.containers.read().await;
        let container = containers.get(id).ok_or_else(|| format!("Container {} not found", id))?;
        let container = container.clone();
        drop(containers);
        
        info!("Starting container {}", container.name);
        
        // Create network namespace
        let net_config = self.network_manager.setup_container_network(&container.id).await?;
        
        // Start container process with namespaces
        let pid = self.spawn_container_process(&container, &net_config).await?;
        
        // Update container state
        let mut containers = self.containers.write().await;
        if let Some(c) = containers.get_mut(id) {
            let mut_container = Arc::make_mut(c);
            mut_container.state = ContainerState::Running;
            mut_container.pid = Some(pid);
            mut_container.ip_address = Some(net_config.ip_address);
            mut_container.started_at = Some(chrono::Utc::now());
        }
        
        info!("Container {} started with PID {}", container.name, pid);
        Ok(())
    }
    
    async fn spawn_container_process(&self, container: &Container, net_config: &NetworkConfig) -> Result<u32, String> {
        // Create init script for container
        let init_script = self.create_init_script(container)?;
        let init_path = container.rootfs.join("init.sh");
        fs::write(&init_path, init_script).map_err(|e| e.to_string())?;
        
        // Use unshare to create namespaces
        let mut cmd = Command::new("unshare");
        cmd.arg("--mount")
            .arg("--pid")
            .arg("--fork");
        
        // Add network namespace if not host mode
        if !matches!(container.config.network_mode, NetworkMode::Host) {
            cmd.arg("--net=/var/run/netns/").arg(&net_config.namespace);
        }
        
        // Add UTS namespace for hostname
        cmd.arg("--uts");
        
        // Chroot and execute
        cmd.arg("chroot")
            .arg(&container.rootfs)
            .arg("/bin/sh")
            .arg("/init.sh");
        
        // Set environment variables
        for (key, value) in &container.config.env {
            cmd.env(key, value);
        }
        
        // Spawn the process
        let child = cmd.spawn().map_err(|e| format!("Failed to spawn container: {}", e))?;
        
        Ok(child.id().unwrap_or(0))
    }
    
    fn create_init_script(&self, container: &Container) -> Result<String, String> {
        let mut script = String::from("#!/bin/sh\n");
        
        // Set hostname
        script.push_str(&format!("hostname {}\n", container.config.hostname));
        
        // Mount proc and sys
        script.push_str("mount -t proc proc /proc 2>/dev/null\n");
        script.push_str("mount -t sysfs sys /sys 2>/dev/null\n");
        
        // Execute the container command
        let cmd = container.config.command.join(" ");
        script.push_str(&format!("exec {}\n", cmd));
        
        Ok(script)
    }
    
    pub async fn stop_container(&self, id: &str) -> Result<(), String> {
        let containers = self.containers.read().await;
        let container = containers.get(id).ok_or_else(|| format!("Container {} not found", id))?;
        let container = container.clone();
        drop(containers);
        
        info!("Stopping container {}", container.name);
        
        if let Some(pid) = container.pid {
            // Send SIGTERM to the process
            Command::new("kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .output()
                .await
                .map_err(|e| format!("Failed to stop container: {}", e))?;
            
            // Wait a bit for graceful shutdown
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            
            // Force kill if still running
            let _ = Command::new("kill")
                .arg("-KILL")
                .arg(pid.to_string())
                .output()
                .await;
        }
        
        // Clean up network
        self.network_manager.cleanup_container_network(&container.id).await?;
        
        // Update container state
        let mut containers = self.containers.write().await;
        if let Some(c) = containers.get_mut(id) {
            Arc::make_mut(c).state = ContainerState::Stopped;
        }
        
        info!("Container {} stopped", container.name);
        Ok(())
    }
    
    pub async fn remove_container(&self, id: &str) -> Result<(), String> {
        // Stop container if running
        let containers = self.containers.read().await;
        if let Some(container) = containers.get(id) {
            if matches!(container.state, ContainerState::Running) {
                drop(containers);
                self.stop_container(id).await?;
            }
        } else {
            return Err(format!("Container {} not found", id));
        }
        
        // Remove from storage
        self.containers.write().await.remove(id);
        
        // Clean up filesystem
        let container_dir = self.runtime_dir.join(id);
        if container_dir.exists() {
            fs::remove_dir_all(container_dir).map_err(|e| e.to_string())?;
        }
        
        info!("Container {} removed", id);
        Ok(())
    }
    
    pub async fn list_containers(&self) -> Vec<Arc<Container>> {
        self.containers.read().await.values().cloned().collect()
    }
    
    pub async fn get_container(&self, id: &str) -> Option<Arc<Container>> {
        self.containers.read().await.get(id).cloned()
    }
}

// Network management
pub struct NetworkManager {
    networks: Arc<RwLock<HashMap<String, Network>>>,
    bridge_name: String,
    subnet: String,
    next_ip: Arc<RwLock<u32>>,
}

#[derive(Debug, Clone)]
pub struct Network {
    pub name: String,
    pub bridge: String,
    pub subnet: String,
    pub gateway: String,
}

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub namespace: String,
    pub veth_host: String,
    pub veth_container: String,
    pub ip_address: String,
    pub gateway: String,
}

impl NetworkManager {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            networks: Arc::new(RwLock::new(HashMap::new())),
            bridge_name: "krust0".to_string(),
            subnet: "172.30.0.0/16".to_string(),
            next_ip: Arc::new(RwLock::new(2)), // Start from .2, .1 is gateway
        })
    }
    
    pub async fn setup_container_network(&self, container_id: &str) -> Result<NetworkConfig, String> {
        // Create network namespace
        let namespace = format!("krust-{}", &container_id[..12]);
        Command::new("ip")
            .args(&["netns", "add", &namespace])
            .output()
            .await
            .map_err(|e| format!("Failed to create network namespace: {}", e))?;
        
        // Create veth pair
        let veth_host = format!("veth-h-{}", &container_id[..8]);
        let veth_container = format!("veth-c-{}", &container_id[..8]);
        
        Command::new("ip")
            .args(&["link", "add", &veth_host, "type", "veth", "peer", "name", &veth_container])
            .output()
            .await
            .map_err(|e| format!("Failed to create veth pair: {}", e))?;
        
        // Move container end to namespace
        Command::new("ip")
            .args(&["link", "set", &veth_container, "netns", &namespace])
            .output()
            .await
            .map_err(|e| format!("Failed to move veth to namespace: {}", e))?;
        
        // Configure IP address
        let ip_num = *self.next_ip.write().await;
        *self.next_ip.write().await += 1;
        let ip_address = format!("172.30.0.{}/16", ip_num);
        
        // Set up container interface
        Command::new("ip")
            .args(&["netns", "exec", &namespace, "ip", "addr", "add", &ip_address, "dev", &veth_container])
            .output()
            .await
            .map_err(|e| format!("Failed to set IP address: {}", e))?;
        
        Command::new("ip")
            .args(&["netns", "exec", &namespace, "ip", "link", "set", &veth_container, "up"])
            .output()
            .await
            .map_err(|e| format!("Failed to bring up container interface: {}", e))?;
        
        // Set up host interface
        Command::new("ip")
            .args(&["link", "set", &veth_host, "up"])
            .output()
            .await
            .map_err(|e| format!("Failed to bring up host interface: {}", e))?;
        
        Ok(NetworkConfig {
            namespace,
            veth_host,
            veth_container,
            ip_address: format!("172.30.0.{}", ip_num),
            gateway: "172.30.0.1".to_string(),
        })
    }
    
    pub async fn cleanup_container_network(&self, container_id: &str) -> Result<(), String> {
        let namespace = format!("krust-{}", &container_id[..12]);
        
        // Delete network namespace (this also cleans up veth pairs)
        let _ = Command::new("ip")
            .args(&["netns", "del", &namespace])
            .output()
            .await;
        
        Ok(())
    }
}

// Storage management
pub struct StorageManager {
    images_dir: PathBuf,
    layers_dir: PathBuf,
}

impl StorageManager {
    pub fn new() -> Result<Self, String> {
        let images_dir = PathBuf::from("/var/lib/krust/images");
        let layers_dir = PathBuf::from("/var/lib/krust/layers");
        
        fs::create_dir_all(&images_dir).map_err(|e| e.to_string())?;
        fs::create_dir_all(&layers_dir).map_err(|e| e.to_string())?;
        
        Ok(Self {
            images_dir,
            layers_dir,
        })
    }
    
    pub async fn prepare_rootfs(&self, image: &str, rootfs: &Path) -> Result<(), String> {
        info!("Preparing rootfs for image {}", image);
        
        // Create rootfs directory
        fs::create_dir_all(rootfs).map_err(|e| e.to_string())?;
        
        // For now, create a minimal filesystem structure
        // In production, this would extract OCI image layers
        let dirs = ["bin", "dev", "etc", "home", "lib", "proc", "root", "sys", "tmp", "usr", "var"];
        for dir in &dirs {
            fs::create_dir_all(rootfs.join(dir)).map_err(|e| e.to_string())?;
        }
        
        // Copy busybox for basic utilities (if available)
        if Path::new("/bin/busybox").exists() {
            let busybox_dest = rootfs.join("bin/busybox");
            fs::copy("/bin/busybox", &busybox_dest).map_err(|e| e.to_string())?;
            
            // Create symlinks for common commands
            let commands = ["sh", "ls", "cat", "echo", "sleep", "ps", "mount", "hostname"];
            for cmd in &commands {
                let link_path = rootfs.join("bin").join(cmd);
                let _ = std::os::unix::fs::symlink("/bin/busybox", link_path);
            }
        }
        
        // Create basic /etc files
        fs::write(rootfs.join("etc/resolv.conf"), "nameserver 8.8.8.8\n").map_err(|e| e.to_string())?;
        fs::write(rootfs.join("etc/hosts"), "127.0.0.1 localhost\n").map_err(|e| e.to_string())?;
        
        info!("Rootfs prepared successfully");
        Ok(())
    }
}