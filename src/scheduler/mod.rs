use crate::Storage;
use anyhow::Result;
use serde_json::Value;
use sqlx::Row;
use tracing::{info, warn};

pub struct Scheduler {
    storage: Storage,
    node_name: String,
}

impl Scheduler {
    pub fn new(storage: Storage) -> Self {
        Self { 
            storage,
            node_name: "krust-node".to_string(),
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting scheduler");
        loop {
            if let Err(e) = self.schedule_pending_pods().await {
                warn!("Scheduler error: {}", e);
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    async fn schedule_pending_pods(&self) -> Result<()> {
        // Find all pods in Pending phase without a node
        let rows = sqlx::query(
            "SELECT uid, name, namespace FROM pods 
             WHERE phase = 'Pending' AND node_name IS NULL AND deletion_timestamp IS NULL"
        )
        .fetch_all(&*self.storage.pool)
        .await?;
        
        for row in rows {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let namespace: String = row.get("namespace");
            
            info!("Scheduling pod {}/{} to node {}", namespace, name, self.node_name);
            
            // Assign the pod to our single node
            sqlx::query(
                "UPDATE pods SET node_name = ?, phase = 'Scheduled' 
                 WHERE uid = ? AND node_name IS NULL"
            )
            .bind(&self.node_name)
            .bind(&uid)
            .execute(&*self.storage.pool)
            .await?;
            
            // Record scheduling event
            self.record_scheduling_event(&uid, &name, &namespace).await?;
        }
        
        Ok(())
    }

    async fn record_scheduling_event(&self, uid: &str, name: &str, namespace: &str) -> Result<()> {
        // Get the updated pod
        let pod_row = sqlx::query(
            "SELECT * FROM pods WHERE uid = ?"
        )
        .bind(uid)
        .fetch_one(&*self.storage.pool)
        .await?;
        
        let spec_str: String = pod_row.get("spec");
        let status_str: String = pod_row.get("status");
        let labels_str: String = pod_row.get("labels");
        let annotations_str: String = pod_row.get("annotations");
        
        let mut pod = serde_json::json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": {
                "uid": uid,
                "name": name,
                "namespace": namespace,
                "resourceVersion": pod_row.get::<i64, _>("resource_version").to_string(),
                "creationTimestamp": pod_row.get::<String, _>("creation_timestamp"),
            },
            "spec": serde_json::from_str::<Value>(&spec_str)?,
            "status": serde_json::from_str::<Value>(&status_str)?
        });
        
        // Add node name to status
        pod["status"]["phase"] = serde_json::json!("Scheduled");
        pod["spec"]["nodeName"] = serde_json::json!(self.node_name);
        
        // Record event
        sqlx::query(
            "INSERT INTO events (resource_type, resource_uid, resource_name, resource_namespace, event_type, resource_version, timestamp, object)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("pods")
        .bind(uid)
        .bind(name)
        .bind(namespace)
        .bind("MODIFIED")
        .bind(pod_row.get::<i64, _>("resource_version"))
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(pod.to_string())
        .execute(&*self.storage.pool)
        .await?;
        
        Ok(())
    }
}