use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct PersistentVolumeStore {
    pool: SqlitePool,
}

impl PersistentVolumeStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, mut pv: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = pv["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("PersistentVolume name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Extract spec fields
        let capacity = pv["spec"]["capacity"].clone();
        if capacity.is_null() {
            return Err(anyhow!("PersistentVolume capacity is required"));
        }
        
        let access_modes = pv["spec"]["accessModes"].clone();
        if access_modes.is_null() {
            return Err(anyhow!("PersistentVolume accessModes is required"));
        }
        
        let reclaim_policy = pv["spec"]
            .get("persistentVolumeReclaimPolicy")
            .and_then(|v| v.as_str())
            .unwrap_or("Retain")
            .to_string();
        
        let storage_class_name = pv["spec"]
            .get("storageClassName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let volume_mode = pv["spec"]
            .get("volumeMode")
            .and_then(|v| v.as_str())
            .unwrap_or("Filesystem")
            .to_string();
        
        // Extract volume source (only one should be set)
        let host_path = pv["spec"].get("hostPath").cloned();
        let nfs = pv["spec"].get("nfs").cloned();
        let local = pv["spec"].get("local").cloned();
        let csi = pv["spec"].get("csi").cloned();
        
        let labels = pv["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = pv["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        // Insert into database
        let query = r#"
            INSERT INTO persistent_volumes (
                uid, name, capacity, access_modes, reclaim_policy, storage_class_name, 
                volume_mode, host_path, nfs, local, csi, phase, labels, annotations, 
                resource_version, creation_timestamp
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 1, ?15)
        "#;
        
        sqlx::query(query)
            .bind(&uid)
            .bind(&name)
            .bind(capacity.to_string())
            .bind(access_modes.to_string())
            .bind(&reclaim_policy)
            .bind(storage_class_name.as_deref())
            .bind(&volume_mode)
            .bind(host_path.as_ref().map(|v| v.to_string()))
            .bind(nfs.as_ref().map(|v| v.to_string()))
            .bind(local.as_ref().map(|v| v.to_string()))
            .bind(csi.as_ref().map(|v| v.to_string()))
            .bind("Available")
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(&now)
            .execute(&self.pool)
            .await?;

        // Build response
        pv["apiVersion"] = json!("v1");
        pv["kind"] = json!("PersistentVolume");
        pv["metadata"]["uid"] = json!(uid);
        pv["metadata"]["resourceVersion"] = json!("1");
        pv["metadata"]["creationTimestamp"] = json!(now);
        pv["metadata"]["selfLink"] = json!(format!("/api/v1/persistentvolumes/{}", name));
        
        // Set default spec values if not present
        pv["spec"]["persistentVolumeReclaimPolicy"] = json!(reclaim_policy);
        pv["spec"]["volumeMode"] = json!(volume_mode);
        
        // Set status
        pv["status"] = json!({
            "phase": "Available"
        });
        
        if !labels.is_null() && labels != json!({}) {
            pv["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            pv["metadata"]["annotations"] = annotations;
        }

        Ok(pv)
    }

    pub async fn get(&self, name: &str) -> Result<Value> {
        let query = r#"
            SELECT uid, capacity, access_modes, reclaim_policy, storage_class_name, volume_mode,
                   host_path, nfs, local, csi, phase, message, reason,
                   claim_namespace, claim_name, claim_uid,
                   labels, annotations, resource_version, creation_timestamp 
            FROM persistent_volumes 
            WHERE name = ?1 AND deletion_timestamp IS NULL
        "#;
        
        let row = sqlx::query(query)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => {
                let uid: String = row.get("uid");
                let capacity_str: String = row.get("capacity");
                let access_modes_str: String = row.get("access_modes");
                let reclaim_policy: String = row.get("reclaim_policy");
                let storage_class_name: Option<String> = row.get("storage_class_name");
                let volume_mode: String = row.get("volume_mode");
                
                let host_path_str: Option<String> = row.get("host_path");
                let nfs_str: Option<String> = row.get("nfs");
                let local_str: Option<String> = row.get("local");
                let csi_str: Option<String> = row.get("csi");
                
                let phase: String = row.get("phase");
                let message: Option<String> = row.get("message");
                let reason: Option<String> = row.get("reason");
                
                let claim_namespace: Option<String> = row.get("claim_namespace");
                let claim_name: Option<String> = row.get("claim_name");
                let claim_uid: Option<String> = row.get("claim_uid");
                
                let labels_str: String = row.get("labels");
                let annotations_str: String = row.get("annotations");
                let resource_version: i64 = row.get("resource_version");
                let creation_timestamp: String = row.get("creation_timestamp");

                let capacity: Value = serde_json::from_str(&capacity_str)?;
                let access_modes: Value = serde_json::from_str(&access_modes_str)?;
                let labels: Value = serde_json::from_str(&labels_str)?;
                let annotations: Value = serde_json::from_str(&annotations_str)?;

                let mut pv = json!({
                    "apiVersion": "v1",
                    "kind": "PersistentVolume",
                    "metadata": {
                        "name": name,
                        "uid": uid,
                        "resourceVersion": resource_version.to_string(),
                        "creationTimestamp": creation_timestamp,
                        "selfLink": format!("/api/v1/persistentvolumes/{}", name),
                    },
                    "spec": {
                        "capacity": capacity,
                        "accessModes": access_modes,
                        "persistentVolumeReclaimPolicy": reclaim_policy,
                        "volumeMode": volume_mode
                    },
                    "status": {
                        "phase": phase
                    }
                });
                
                // Add optional spec fields
                if let Some(sc) = storage_class_name {
                    pv["spec"]["storageClassName"] = json!(sc);
                }
                
                // Add volume source
                if let Some(hp) = host_path_str {
                    pv["spec"]["hostPath"] = serde_json::from_str(&hp)?;
                }
                if let Some(n) = nfs_str {
                    pv["spec"]["nfs"] = serde_json::from_str(&n)?;
                }
                if let Some(l) = local_str {
                    pv["spec"]["local"] = serde_json::from_str(&l)?;
                }
                if let Some(c) = csi_str {
                    pv["spec"]["csi"] = serde_json::from_str(&c)?;
                }
                
                // Add claim reference if bound
                if let (Some(ns), Some(n), Some(u)) = (claim_namespace, claim_name, claim_uid) {
                    pv["spec"]["claimRef"] = json!({
                        "namespace": ns,
                        "name": n,
                        "uid": u
                    });
                }
                
                // Add status fields
                if let Some(msg) = message {
                    pv["status"]["message"] = json!(msg);
                }
                if let Some(rsn) = reason {
                    pv["status"]["reason"] = json!(rsn);
                }

                if !labels.is_null() && labels != json!({}) {
                    pv["metadata"]["labels"] = labels;
                }

                if !annotations.is_null() && annotations != json!({}) {
                    pv["metadata"]["annotations"] = annotations;
                }

                Ok(pv)
            }
            None => Err(anyhow!("PersistentVolume {} not found", name)),
        }
    }

    pub async fn list(&self) -> Result<Value> {
        let query = r#"
            SELECT uid, name, capacity, access_modes, reclaim_policy, storage_class_name, volume_mode,
                   host_path, nfs, local, csi, phase, message, reason,
                   claim_namespace, claim_name, claim_uid,
                   labels, annotations, resource_version, creation_timestamp 
            FROM persistent_volumes 
            WHERE deletion_timestamp IS NULL 
            ORDER BY name
        "#;

        let rows = sqlx::query(query)
            .fetch_all(&self.pool)
            .await?;

        let mut items = Vec::new();
        for row in rows {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let capacity_str: String = row.get("capacity");
            let access_modes_str: String = row.get("access_modes");
            let reclaim_policy: String = row.get("reclaim_policy");
            let storage_class_name: Option<String> = row.get("storage_class_name");
            let volume_mode: String = row.get("volume_mode");
            
            let host_path_str: Option<String> = row.get("host_path");
            let nfs_str: Option<String> = row.get("nfs");
            let local_str: Option<String> = row.get("local");
            let csi_str: Option<String> = row.get("csi");
            
            let phase: String = row.get("phase");
            let claim_namespace: Option<String> = row.get("claim_namespace");
            let claim_name: Option<String> = row.get("claim_name");
            let claim_uid: Option<String> = row.get("claim_uid");
            
            let labels_str: String = row.get("labels");
            let annotations_str: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let creation_timestamp: String = row.get("creation_timestamp");
            
            let capacity: Value = serde_json::from_str(&capacity_str)?;
            let access_modes: Value = serde_json::from_str(&access_modes_str)?;
            let labels: Value = serde_json::from_str(&labels_str)?;
            let annotations: Value = serde_json::from_str(&annotations_str)?;

            let mut pv = json!({
                "apiVersion": "v1",
                "kind": "PersistentVolume",
                "metadata": {
                    "name": name,
                    "uid": uid,
                    "resourceVersion": resource_version.to_string(),
                    "creationTimestamp": creation_timestamp,
                    "selfLink": format!("/api/v1/persistentvolumes/{}", name),
                },
                "spec": {
                    "capacity": capacity,
                    "accessModes": access_modes,
                    "persistentVolumeReclaimPolicy": reclaim_policy,
                    "volumeMode": volume_mode
                },
                "status": {
                    "phase": phase
                }
            });
            
            if let Some(sc) = storage_class_name {
                pv["spec"]["storageClassName"] = json!(sc);
            }
            
            // Add volume source
            if let Some(hp) = host_path_str {
                pv["spec"]["hostPath"] = serde_json::from_str(&hp)?;
            }
            if let Some(n) = nfs_str {
                pv["spec"]["nfs"] = serde_json::from_str(&n)?;
            }
            if let Some(l) = local_str {
                pv["spec"]["local"] = serde_json::from_str(&l)?;
            }
            if let Some(c) = csi_str {
                pv["spec"]["csi"] = serde_json::from_str(&c)?;
            }
            
            // Add claim reference if bound
            if let (Some(ns), Some(n), Some(u)) = (claim_namespace, claim_name, claim_uid) {
                pv["spec"]["claimRef"] = json!({
                    "namespace": ns,
                    "name": n,
                    "uid": u
                });
            }

            if !labels.is_null() && labels != json!({}) {
                pv["metadata"]["labels"] = labels;
            }

            if !annotations.is_null() && annotations != json!({}) {
                pv["metadata"]["annotations"] = annotations;
            }

            items.push(pv);
        }

        Ok(json!({
            "apiVersion": "v1",
            "kind": "PersistentVolumeList",
            "metadata": {
                "selfLink": "/api/v1/persistentvolumes",
                "resourceVersion": "1",
            },
            "items": items,
        }))
    }

    pub async fn update(&self, name: &str, pv: Value) -> Result<Value> {
        // Extract spec fields
        let capacity = pv["spec"]["capacity"].clone();
        if capacity.is_null() {
            return Err(anyhow!("PersistentVolume capacity is required"));
        }
        
        let access_modes = pv["spec"]["accessModes"].clone();
        if access_modes.is_null() {
            return Err(anyhow!("PersistentVolume accessModes is required"));
        }
        
        let reclaim_policy = pv["spec"]
            .get("persistentVolumeReclaimPolicy")
            .and_then(|v| v.as_str())
            .unwrap_or("Retain")
            .to_string();
        
        let storage_class_name = pv["spec"]
            .get("storageClassName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let volume_mode = pv["spec"]
            .get("volumeMode")
            .and_then(|v| v.as_str())
            .unwrap_or("Filesystem")
            .to_string();
        
        // Extract volume source
        let host_path = pv["spec"].get("hostPath").cloned();
        let nfs = pv["spec"].get("nfs").cloned();
        let local = pv["spec"].get("local").cloned();
        let csi = pv["spec"].get("csi").cloned();
        
        let labels = pv["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = pv["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        let update_query = r#"
            UPDATE persistent_volumes 
            SET capacity = ?1, access_modes = ?2, reclaim_policy = ?3, storage_class_name = ?4,
                volume_mode = ?5, host_path = ?6, nfs = ?7, local = ?8, csi = ?9,
                labels = ?10, annotations = ?11, resource_version = resource_version + 1
            WHERE name = ?12 AND deletion_timestamp IS NULL
        "#;

        let rows_affected = sqlx::query(update_query)
            .bind(capacity.to_string())
            .bind(access_modes.to_string())
            .bind(&reclaim_policy)
            .bind(storage_class_name.as_deref())
            .bind(&volume_mode)
            .bind(host_path.as_ref().map(|v| v.to_string()))
            .bind(nfs.as_ref().map(|v| v.to_string()))
            .bind(local.as_ref().map(|v| v.to_string()))
            .bind(csi.as_ref().map(|v| v.to_string()))
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(name)
            .execute(&self.pool)
            .await?
            .rows_affected();

        if rows_affected == 0 {
            return Err(anyhow!("PersistentVolume {} not found", name));
        }

        self.get(name).await
    }

    pub async fn delete(&self, name: &str) -> Result<Value> {
        // Get the PV before deletion
        let pv = self.get(name).await?;

        // Soft delete
        let deletion_timestamp = Utc::now().to_rfc3339();
        let delete_query = "UPDATE persistent_volumes SET deletion_timestamp = ?1 WHERE name = ?2";
        
        sqlx::query(delete_query)
            .bind(&deletion_timestamp)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(pv)
    }

    pub async fn bind_to_claim(&self, pv_name: &str, claim_namespace: &str, claim_name: &str, claim_uid: &str) -> Result<()> {
        let update_query = r#"
            UPDATE persistent_volumes 
            SET phase = 'Bound', claim_namespace = ?1, claim_name = ?2, claim_uid = ?3,
                resource_version = resource_version + 1
            WHERE name = ?4 AND phase = 'Available' AND deletion_timestamp IS NULL
        "#;

        let rows_affected = sqlx::query(update_query)
            .bind(claim_namespace)
            .bind(claim_name)
            .bind(claim_uid)
            .bind(pv_name)
            .execute(&self.pool)
            .await?
            .rows_affected();

        if rows_affected == 0 {
            return Err(anyhow!("Failed to bind PersistentVolume {} - not available", pv_name));
        }

        Ok(())
    }

    pub async fn release(&self, pv_name: &str) -> Result<()> {
        let update_query = r#"
            UPDATE persistent_volumes 
            SET phase = 'Released', claim_namespace = NULL, claim_name = NULL, claim_uid = NULL,
                resource_version = resource_version + 1
            WHERE name = ?1 AND phase = 'Bound' AND deletion_timestamp IS NULL
        "#;

        sqlx::query(update_query)
            .bind(pv_name)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}