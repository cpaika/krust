use anyhow::Result;
use serde_json::{json, Value};
use sqlx::Row;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};
use uuid::Uuid;

use crate::Storage;

pub struct ReplicaSetController {
    storage: Storage,
}

impl ReplicaSetController {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting replicaset controller");
        
        loop {
            if let Err(e) = self.reconcile_replicasets().await {
                error!("ReplicaSet controller error: {}", e);
            }
            
            sleep(Duration::from_secs(2)).await;
        }
    }

    async fn reconcile_replicasets(&self) -> Result<()> {
        // Get all replicasets
        let replicasets = sqlx::query(
            "SELECT uid, name, namespace, spec, status, replicas FROM replicasets WHERE deletion_timestamp IS NULL"
        )
        .fetch_all(&*self.storage.pool)
        .await?;
        
        for rs_row in replicasets {
            let rs_uid: String = rs_row.get("uid");
            let rs_name: String = rs_row.get("name");
            let rs_namespace: String = rs_row.get("namespace");
            let spec_str: String = rs_row.get("spec");
            let desired_replicas: i64 = rs_row.get("replicas");
            
            if let Ok(spec) = serde_json::from_str::<Value>(&spec_str) {
                let selector = &spec["selector"];
                
                // Count existing pods that match this ReplicaSet
                let existing_pods = self.count_matching_pods(&rs_namespace, selector, &rs_uid).await?;
                
                if existing_pods < desired_replicas {
                    // Need to create more pods
                    let pods_to_create = desired_replicas - existing_pods;
                    info!("ReplicaSet {}/{} needs {} more pods", rs_namespace, rs_name, pods_to_create);
                    
                    for i in 0..pods_to_create {
                        self.create_pod_for_replicaset(&rs_uid, &rs_name, &rs_namespace, &spec, i).await?;
                    }
                } else if existing_pods > desired_replicas {
                    // Need to delete excess pods
                    let pods_to_delete = existing_pods - desired_replicas;
                    info!("ReplicaSet {}/{} has {} excess pods", rs_namespace, rs_name, pods_to_delete);
                    
                    self.delete_excess_pods(&rs_namespace, selector, &rs_uid, pods_to_delete).await?;
                }
                
                // Update ReplicaSet status
                self.update_replicaset_status(&rs_uid, &rs_namespace, &rs_name, existing_pods).await?;
            }
        }
        
        Ok(())
    }

    async fn count_matching_pods(&self, namespace: &str, selector: &Value, rs_uid: &str) -> Result<i64> {
        // Build label query from selector
        let mut query_parts = Vec::new();
        
        if let Some(match_labels) = selector["matchLabels"].as_object() {
            for (key, value) in match_labels {
                if let Some(val) = value.as_str() {
                    query_parts.push(format!("\"{}\":\"{}\"", key, val));
                }
            }
        }
        
        if query_parts.is_empty() {
            return Ok(0);
        }
        
        let label_pattern = query_parts.join(",");
        
        // Count pods with matching labels that are owned by this ReplicaSet
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pods 
             WHERE namespace = ? 
             AND deletion_timestamp IS NULL 
             AND labels LIKE ?
             AND EXISTS (
                SELECT 1 FROM events 
                WHERE resource_type = 'pods' 
                AND resource_uid = pods.uid 
                AND object LIKE ?
             )"
        )
        .bind(namespace)
        .bind(format!("%{}%", label_pattern))
        .bind(format!("%ownerReferences%{}%", rs_uid))
        .fetch_one(&*self.storage.pool)
        .await
        .unwrap_or(0);
        
        Ok(count)
    }

    async fn create_pod_for_replicaset(
        &self, 
        rs_uid: &str, 
        rs_name: &str, 
        rs_namespace: &str, 
        spec: &Value,
        index: i64
    ) -> Result<()> {
        let template = &spec["template"];
        let pod_name = format!("{}-{}", rs_name, Uuid::new_v4().to_string().split('-').next().unwrap());
        
        let mut pod = json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": {
                "name": pod_name,
                "namespace": rs_namespace,
                "labels": template["metadata"]["labels"],
                "ownerReferences": [{
                    "apiVersion": "apps/v1",
                    "kind": "ReplicaSet",
                    "name": rs_name,
                    "uid": rs_uid,
                    "controller": true,
                    "blockOwnerDeletion": true
                }]
            },
            "spec": template["spec"]
        });
        
        // Add ReplicaSet labels to pod
        if let Some(labels) = template["metadata"]["labels"].as_object() {
            for (key, value) in labels {
                pod["metadata"]["labels"][key] = value.clone();
            }
        }
        
        // Create the pod
        if let Err(e) = self.storage.pods().create(rs_namespace, pod).await {
            error!("Failed to create pod for ReplicaSet {}/{}: {}", rs_namespace, rs_name, e);
        } else {
            info!("Created pod {} for ReplicaSet {}/{}", pod_name, rs_namespace, rs_name);
        }
        
        Ok(())
    }

    async fn delete_excess_pods(
        &self, 
        namespace: &str, 
        selector: &Value, 
        rs_uid: &str,
        count: i64
    ) -> Result<()> {
        // Build label query from selector
        let mut query_parts = Vec::new();
        
        if let Some(match_labels) = selector["matchLabels"].as_object() {
            for (key, value) in match_labels {
                if let Some(val) = value.as_str() {
                    query_parts.push(format!("\"{}\":\"{}\"", key, val));
                }
            }
        }
        
        if query_parts.is_empty() {
            return Ok(());
        }
        
        let label_pattern = query_parts.join(",");
        
        // Get pods to delete (oldest first)
        let pods = sqlx::query(
            "SELECT name FROM pods 
             WHERE namespace = ? 
             AND deletion_timestamp IS NULL 
             AND labels LIKE ?
             ORDER BY creation_timestamp ASC
             LIMIT ?"
        )
        .bind(namespace)
        .bind(format!("%{}%", label_pattern))
        .bind(count)
        .fetch_all(&*self.storage.pool)
        .await?;
        
        for pod_row in pods {
            let pod_name: String = pod_row.get("name");
            if let Err(e) = self.storage.pods().delete(namespace, &pod_name).await {
                error!("Failed to delete excess pod {}/{}: {}", namespace, pod_name, e);
            } else {
                info!("Deleted excess pod {}/{}", namespace, pod_name);
            }
        }
        
        Ok(())
    }

    async fn update_replicaset_status(&self, uid: &str, namespace: &str, name: &str, replicas: i64) -> Result<()> {
        let status = json!({
            "replicas": replicas,
            "fullyLabeledReplicas": replicas,
            "readyReplicas": replicas, // Simplified - assume all are ready
            "availableReplicas": replicas,
            "observedGeneration": 1,
            "conditions": [
                {
                    "type": "ReplicaFailure",
                    "status": "False",
                    "lastTransitionTime": chrono::Utc::now().to_rfc3339(),
                    "reason": "ReplicasAvailable",
                    "message": format!("{} replicas are available", replicas)
                }
            ]
        });
        
        sqlx::query(
            "UPDATE replicasets SET status = ? WHERE uid = ?"
        )
        .bind(status.to_string())
        .bind(uid)
        .execute(&*self.storage.pool)
        .await?;
        
        Ok(())
    }
}