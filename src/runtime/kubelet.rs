use anyhow::Result;
use bollard::{
    container::{Config, CreateContainerOptions, StartContainerOptions},
    Docker,
};
use serde_json::Value;
use sqlx::Row;
use std::collections::HashMap;
use tracing::{error, info};

use crate::Storage;

pub struct Kubelet {
    storage: Storage,
    docker: Docker,
    node_name: String,
}

impl Kubelet {
    pub async fn new(storage: Storage) -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        
        // Test Docker connection
        docker.ping().await?;
        info!("Connected to Docker daemon");
        
        Ok(Self {
            storage,
            docker,
            node_name: "krust-node".to_string(),
        })
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting kubelet");
        
        loop {
            // Process scheduled pods
            if let Err(e) = self.sync_pods().await {
                error!("Kubelet sync error: {}", e);
            }
            
            // Update pod statuses
            if let Err(e) = self.update_pod_statuses().await {
                error!("Status update error: {}", e);
            }
            
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    async fn sync_pods(&self) -> Result<()> {
        // Find pods scheduled to this node that aren't running yet
        let rows = sqlx::query(
            "SELECT uid, name, namespace, spec FROM pods 
             WHERE node_name = ? AND phase IN ('Scheduled', 'Pending') 
             AND deletion_timestamp IS NULL"
        )
        .bind(&self.node_name)
        .fetch_all(&*self.storage.pool)
        .await?;
        
        for row in rows {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let namespace: String = row.get("namespace");
            let spec_str: String = row.get("spec");
            let spec: Value = serde_json::from_str(&spec_str)?;
            
            info!("Starting pod {}/{}", namespace, name);
            
            if let Err(e) = self.start_pod(&uid, &name, &namespace, &spec).await {
                error!("Failed to start pod {}/{}: {}", namespace, name, e);
                // Update pod status to Failed
                self.update_pod_phase(&uid, "Failed").await?;
            } else {
                // Update pod status to Running
                self.update_pod_phase(&uid, "Running").await?;
            }
        }
        
        Ok(())
    }

    async fn start_pod(&self, uid: &str, name: &str, namespace: &str, spec: &Value) -> Result<()> {
        // Process each container in the pod
        if let Some(containers) = spec["containers"].as_array() {
            for container in containers {
                let container_name = container["name"]
                    .as_str()
                    .unwrap_or("container");
                let image = container["image"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Container image is required"))?;
                
                let full_container_name = format!("k8s_{}_{}_{}_{}", 
                    container_name, name, namespace, uid);
                
                // Check if container already exists
                if self.container_exists(&full_container_name).await {
                    info!("Container {} already exists", full_container_name);
                    continue;
                }
                
                // Create container config
                let mut config = Config {
                    image: Some(image.to_string()),
                    hostname: Some(name.to_string()),
                    labels: Some(HashMap::from([
                        ("io.kubernetes.pod.name".to_string(), name.to_string()),
                        ("io.kubernetes.pod.namespace".to_string(), namespace.to_string()),
                        ("io.kubernetes.pod.uid".to_string(), uid.to_string()),
                        ("io.kubernetes.container.name".to_string(), container_name.to_string()),
                    ])),
                    ..Default::default()
                };
                
                // Add environment variables
                if let Some(env_vars) = container["env"].as_array() {
                    let mut env = Vec::new();
                    for var in env_vars {
                        if let (Some(name), Some(value)) = 
                            (var["name"].as_str(), var["value"].as_str()) {
                            env.push(format!("{}={}", name, value));
                        }
                    }
                    config.env = Some(env);
                }
                
                // Add command if specified
                if let Some(command) = container["command"].as_array() {
                    config.cmd = Some(
                        command
                            .iter()
                            .filter_map(|c| c.as_str())
                            .map(String::from)
                            .collect()
                    );
                }
                
                // Add args if specified
                if let Some(args) = container["args"].as_array() {
                    let args_vec: Vec<String> = args
                        .iter()
                        .filter_map(|a| a.as_str())
                        .map(String::from)
                        .collect();
                    
                    if let Some(ref mut cmd) = config.cmd {
                        cmd.extend(args_vec);
                    } else {
                        config.cmd = Some(args_vec);
                    }
                }
                
                // Create the container
                let options = CreateContainerOptions {
                    name: full_container_name.clone(),
                    ..Default::default()
                };
                
                info!("Creating container {} with image {}", full_container_name, image);
                self.docker.create_container(Some(options), config).await?;
                
                // Start the container
                info!("Starting container {}", full_container_name);
                self.docker.start_container(&full_container_name, None::<StartContainerOptions<String>>).await?;
            }
        }
        
        Ok(())
    }

    async fn container_exists(&self, name: &str) -> bool {
        match self.docker.inspect_container(name, None).await {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    async fn update_pod_phase(&self, uid: &str, phase: &str) -> Result<()> {
        sqlx::query(
            "UPDATE pods SET phase = ? WHERE uid = ?"
        )
        .bind(phase)
        .bind(uid)
        .execute(&*self.storage.pool)
        .await?;
        
        Ok(())
    }

    async fn update_pod_statuses(&self) -> Result<()> {
        // Get all running pods on this node
        let rows = sqlx::query(
            "SELECT uid, name, namespace FROM pods 
             WHERE node_name = ? AND phase = 'Running' 
             AND deletion_timestamp IS NULL"
        )
        .bind(&self.node_name)
        .fetch_all(&*self.storage.pool)
        .await?;
        
        for row in rows {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let namespace: String = row.get("namespace");
            
            // Check if all containers are still running
            let pattern = format!("k8s_*_{}_{}_{}", name, namespace, uid);
            let filters = HashMap::from([
                ("label".to_string(), vec![format!("io.kubernetes.pod.uid={}", uid)]),
            ]);
            
            let containers = self.docker.list_containers(Some(bollard::container::ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            })).await?;
            
            let mut all_running = true;
            for container in &containers {
                if let Some(state) = &container.state {
                    if state != "running" {
                        all_running = false;
                        break;
                    }
                }
            }
            
            if !all_running && !containers.is_empty() {
                // At least one container has stopped
                self.update_pod_phase(&uid, "Failed").await?;
            } else if containers.is_empty() {
                // No containers found for this pod
                self.update_pod_phase(&uid, "Failed").await?;
            }
        }
        
        // Handle pod deletions
        self.cleanup_deleted_pods().await?;
        
        Ok(())
    }

    async fn cleanup_deleted_pods(&self) -> Result<()> {
        // Find pods that have been marked for deletion
        let rows = sqlx::query(
            "SELECT uid, name, namespace FROM pods 
             WHERE node_name = ? AND deletion_timestamp IS NOT NULL"
        )
        .bind(&self.node_name)
        .fetch_all(&*self.storage.pool)
        .await?;
        
        for row in rows {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let namespace: String = row.get("namespace");
            
            info!("Cleaning up deleted pod {}/{}", namespace, name);
            
            // Stop and remove containers
            let filters = HashMap::from([
                ("label".to_string(), vec![format!("io.kubernetes.pod.uid={}", uid)]),
            ]);
            
            let containers = self.docker.list_containers(Some(bollard::container::ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            })).await?;
            
            for container in containers {
                if let Some(id) = container.id {
                    info!("Stopping container {}", id);
                    let _ = self.docker.stop_container(&id, None).await;
                    
                    info!("Removing container {}", id);
                    let _ = self.docker.remove_container(&id, None).await;
                }
            }
            
            // Remove from database
            sqlx::query("DELETE FROM pods WHERE uid = ?")
                .bind(&uid)
                .execute(&*self.storage.pool)
                .await?;
        }
        
        Ok(())
    }
}