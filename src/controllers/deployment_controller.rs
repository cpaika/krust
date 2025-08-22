use anyhow::Result;
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};
use uuid::Uuid;

use crate::Storage;

pub struct DeploymentController {
    storage: Storage,
}

impl DeploymentController {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting deployment controller");
        
        loop {
            if let Err(e) = self.reconcile_deployments().await {
                error!("Deployment controller error: {}", e);
            }
            
            sleep(Duration::from_secs(2)).await;
        }
    }

    async fn reconcile_deployments(&self) -> Result<()> {
        // Get all deployments
        let deployments = sqlx::query(
            "SELECT uid, name, namespace, spec, status, generation FROM deployments WHERE deletion_timestamp IS NULL"
        )
        .fetch_all(&*self.storage.pool)
        .await?;
        
        for deployment_row in deployments {
            let deployment_uid: String = deployment_row.get("uid");
            let deployment_name: String = deployment_row.get("name");
            let deployment_namespace: String = deployment_row.get("namespace");
            let spec_str: String = deployment_row.get("spec");
            let generation: i64 = deployment_row.get("generation");
            
            if let Ok(spec) = serde_json::from_str::<Value>(&spec_str) {
                // Check if ReplicaSet exists for this deployment
                let rs_name = self.generate_replicaset_name(&deployment_name, &spec);
                
                let existing_rs = sqlx::query(
                    "SELECT uid FROM replicasets WHERE name = ? AND namespace = ? AND deletion_timestamp IS NULL"
                )
                .bind(&rs_name)
                .bind(&deployment_namespace)
                .fetch_optional(&*self.storage.pool)
                .await?;
                
                if existing_rs.is_none() {
                    // Create ReplicaSet
                    info!("Creating ReplicaSet {} for Deployment {}/{}", rs_name, deployment_namespace, deployment_name);
                    
                    let replicas = spec["replicas"].as_i64().unwrap_or(1);
                    let selector = spec["selector"].clone();
                    let template = spec["template"].clone();
                    
                    // Create ReplicaSet with owner reference to Deployment
                    let replicaset = json!({
                        "metadata": {
                            "name": rs_name,
                            "namespace": deployment_namespace,
                            "labels": {
                                "deployment": deployment_name.clone()
                            },
                            "ownerReferences": [{
                                "apiVersion": "apps/v1",
                                "kind": "Deployment",
                                "name": deployment_name.clone(),
                                "uid": deployment_uid.clone(),
                                "controller": true,
                                "blockOwnerDeletion": true
                            }]
                        },
                        "spec": {
                            "replicas": replicas,
                            "selector": selector,
                            "template": template
                        }
                    });
                    
                    // Store the ReplicaSet
                    if let Err(e) = self.storage.replicasets()
                        .create(&deployment_namespace, replicaset)
                        .await 
                    {
                        error!("Failed to create ReplicaSet for Deployment {}/{}: {}", 
                            deployment_namespace, deployment_name, e);
                    }
                }
                
                // Update deployment status
                self.update_deployment_status(&deployment_uid, &deployment_namespace, &deployment_name).await?;
            }
        }
        
        Ok(())
    }

    async fn update_deployment_status(&self, uid: &str, namespace: &str, name: &str) -> Result<()> {
        // Count pods managed by this deployment's replicasets
        let rs_rows = sqlx::query(
            "SELECT name, replicas FROM replicasets 
             WHERE namespace = ? AND owner_references LIKE ? AND deletion_timestamp IS NULL"
        )
        .bind(namespace)
        .bind(format!("%\"uid\":\"{}%", uid))
        .fetch_all(&*self.storage.pool)
        .await?;
        
        let mut total_replicas = 0;
        let mut ready_replicas = 0;
        
        for rs_row in rs_rows {
            let rs_name: String = rs_row.get("name");
            let desired_replicas: i64 = rs_row.get("replicas");
            total_replicas += desired_replicas;
            
            // Count ready pods for this ReplicaSet
            // In a real implementation, we'd check pod status
            ready_replicas += desired_replicas; // Simplified for now
        }
        
        let status = json!({
            "observedGeneration": 1,
            "replicas": total_replicas,
            "updatedReplicas": total_replicas,
            "readyReplicas": ready_replicas,
            "availableReplicas": ready_replicas,
            "conditions": [
                {
                    "type": "Available",
                    "status": if ready_replicas > 0 { "True" } else { "False" },
                    "lastUpdateTime": chrono::Utc::now().to_rfc3339(),
                    "lastTransitionTime": chrono::Utc::now().to_rfc3339(),
                    "reason": "MinimumReplicasAvailable",
                    "message": format!("{} replicas available", ready_replicas)
                },
                {
                    "type": "Progressing",
                    "status": "True",
                    "lastUpdateTime": chrono::Utc::now().to_rfc3339(),
                    "lastTransitionTime": chrono::Utc::now().to_rfc3339(),
                    "reason": "NewReplicaSetAvailable",
                    "message": "ReplicaSet has successfully progressed"
                }
            ]
        });
        
        sqlx::query(
            "UPDATE deployments SET status = ? WHERE uid = ?"
        )
        .bind(status.to_string())
        .bind(uid)
        .execute(&*self.storage.pool)
        .await?;
        
        Ok(())
    }

    fn generate_replicaset_name(&self, deployment_name: &str, spec: &Value) -> String {
        // Generate a deterministic name based on deployment name and pod template hash
        let mut hasher = DefaultHasher::new();
        spec["template"].to_string().hash(&mut hasher);
        let hash = hasher.finish();
        format!("{}-{:x}", deployment_name, hash % 0xfffffff)
    }
}