use anyhow::Result;
use bollard::{
    container::{Config, CreateContainerOptions, StartContainerOptions},
    Docker,
};
use serde_json::{json, Value};
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
                
                // Pull image if not present
                info!("Pulling image {} if needed...", image);
                if let Err(e) = self.pull_image(image).await {
                    error!("Failed to pull image {}: {}", image, e);
                    return Err(anyhow::anyhow!("Failed to pull image: {}", e));
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

    async fn pull_image(&self, image: &str) -> Result<()> {
        use bollard::image::CreateImageOptions;
        use futures::StreamExt;
        
        // Parse image name and tag
        let parts: Vec<&str> = image.split(':').collect();
        let (image_name, tag) = if parts.len() > 1 {
            (parts[0], parts[1])
        } else {
            (image, "latest")
        };
        
        let options = CreateImageOptions {
            from_image: image_name,
            tag,
            ..Default::default()
        };
        
        info!("Pulling image {}:{}", image_name, tag);
        
        let mut stream = self.docker.create_image(Some(options), None, None);
        
        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status {
                        if let Some(progress) = info.progress {
                            info!("Pull progress: {} - {}", status, progress);
                        } else {
                            info!("Pull status: {}", status);
                        }
                    }
                }
                Err(e) => {
                    error!("Error pulling image: {}", e);
                    return Err(anyhow::anyhow!("Failed to pull image: {}", e));
                }
            }
        }
        
        info!("Successfully pulled image {}:{}", image_name, tag);
        Ok(())
    }

    async fn update_pod_phase(&self, uid: &str, phase: &str) -> Result<()> {
        // Get current pod to update status properly
        let pod_row = sqlx::query(
            "SELECT spec, status FROM pods WHERE uid = ?"
        )
        .bind(uid)
        .fetch_optional(&*self.storage.pool)
        .await?;
        
        if let Some(row) = pod_row {
            let spec_str: String = row.get("spec");
            let spec: Value = serde_json::from_str(&spec_str)?;
            let mut status: Value = serde_json::from_str(&row.get::<String, _>("status"))?;
            
            // Update phase
            status["phase"] = json!(phase);
            
            // Update conditions based on phase
            let now = chrono::Utc::now().to_rfc3339();
            if phase == "Running" {
                // Update Ready condition
                if let Some(conditions) = status["conditions"].as_array_mut() {
                    for condition in conditions {
                        if condition["type"] == "Ready" || condition["type"] == "ContainersReady" {
                            condition["status"] = json!("True");
                            condition["lastTransitionTime"] = json!(now);
                            condition["reason"] = json!("ContainersReady");
                            condition["message"] = json!("All containers are ready");
                        } else if condition["type"] == "PodScheduled" {
                            condition["status"] = json!("True");
                            condition["lastTransitionTime"] = json!(now);
                            condition["reason"] = json!("Scheduled");
                            condition["message"] = json!("Pod has been scheduled to node");
                        }
                    }
                }
                
                // Add container statuses
                if let Some(containers) = spec["containers"].as_array() {
                    let mut container_statuses = Vec::new();
                    for container in containers {
                        let name = container["name"].as_str().unwrap_or("container");
                        container_statuses.push(json!({
                            "name": name,
                            "state": {
                                "running": {
                                    "startedAt": now
                                }
                            },
                            "ready": true,
                            "restartCount": 0,
                            "image": container["image"],
                            "imageID": container["image"],
                            "containerID": format!("docker://{}", uid),
                            "started": true
                        }));
                    }
                    status["containerStatuses"] = json!(container_statuses);
                }
                
                status["startTime"] = json!(now);
                
                // Assign pod IP (simplified - just use a unique IP)
                let pod_ip = format!("10.244.0.{}", (uid.bytes().fold(0u8, |a, b| a.wrapping_add(b)) % 254) + 1);
                status["podIP"] = json!(pod_ip.clone());
                status["podIPs"] = json!([{"ip": pod_ip}]);
                status["hostIP"] = json!("127.0.0.1");
            } else if phase == "Failed" {
                // Update conditions for failed state
                if let Some(conditions) = status["conditions"].as_array_mut() {
                    for condition in conditions {
                        if condition["type"] == "Ready" || condition["type"] == "ContainersReady" {
                            condition["status"] = json!("False");
                            condition["lastTransitionTime"] = json!(now);
                            condition["reason"] = json!("ContainersFailed");
                            condition["message"] = json!("One or more containers failed");
                        }
                    }
                }
            }
            
            sqlx::query(
                "UPDATE pods SET phase = ?, status = ? WHERE uid = ?"
            )
            .bind(phase)
            .bind(status.to_string())
            .bind(uid)
            .execute(&*self.storage.pool)
            .await?;
        } else {
            // Fallback to simple phase update
            sqlx::query(
                "UPDATE pods SET phase = ? WHERE uid = ?"
            )
            .bind(phase)
            .bind(uid)
            .execute(&*self.storage.pool)
            .await?;
        }
        
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