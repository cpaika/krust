use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct PersistentVolumeClaimStore {
    pool: SqlitePool,
}

impl PersistentVolumeClaimStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut pvc: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = pvc["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("PersistentVolumeClaim name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Extract spec fields
        let access_modes = pvc["spec"]["accessModes"].clone();
        if access_modes.is_null() {
            return Err(anyhow!("PersistentVolumeClaim accessModes is required"));
        }
        
        let resources = pvc["spec"]["resources"].clone();
        if resources.is_null() || resources["requests"]["storage"].is_null() {
            return Err(anyhow!("PersistentVolumeClaim storage request is required"));
        }
        
        let storage_class_name = pvc["spec"]
            .get("storageClassName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let volume_mode = pvc["spec"]
            .get("volumeMode")
            .and_then(|v| v.as_str())
            .unwrap_or("Filesystem")
            .to_string();
        
        let volume_name = pvc["spec"]
            .get("volumeName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let selector = pvc["spec"].get("selector").cloned();
        
        let labels = pvc["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = pvc["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        // Insert into database
        let query = r#"
            INSERT INTO persistent_volume_claims (
                uid, namespace, name, access_modes, resources, storage_class_name, 
                volume_mode, volume_name, selector, phase, labels, annotations, 
                resource_version, creation_timestamp
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1, ?13)
        "#;
        
        sqlx::query(query)
            .bind(&uid)
            .bind(namespace)
            .bind(&name)
            .bind(access_modes.to_string())
            .bind(resources.to_string())
            .bind(storage_class_name.as_deref())
            .bind(&volume_mode)
            .bind(volume_name.as_deref())
            .bind(selector.as_ref().map(|v| v.to_string()))
            .bind("Pending")
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(&now)
            .execute(&self.pool)
            .await?;

        // Build response
        pvc["apiVersion"] = json!("v1");
        pvc["kind"] = json!("PersistentVolumeClaim");
        pvc["metadata"]["uid"] = json!(uid);
        pvc["metadata"]["namespace"] = json!(namespace);
        pvc["metadata"]["resourceVersion"] = json!("1");
        pvc["metadata"]["creationTimestamp"] = json!(now);
        pvc["metadata"]["selfLink"] = json!(format!("/api/v1/namespaces/{}/persistentvolumeclaims/{}", namespace, name));
        
        // Set default spec values if not present
        pvc["spec"]["volumeMode"] = json!(volume_mode);
        
        // Set status
        pvc["status"] = json!({
            "phase": "Pending"
        });
        
        if !labels.is_null() && labels != json!({}) {
            pvc["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            pvc["metadata"]["annotations"] = annotations;
        }

        Ok(pvc)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let query = r#"
            SELECT uid, access_modes, resources, storage_class_name, volume_mode,
                   volume_name, selector, phase, access_modes_status, capacity,
                   labels, annotations, resource_version, creation_timestamp 
            FROM persistent_volume_claims 
            WHERE namespace = ?1 AND name = ?2 AND deletion_timestamp IS NULL
        "#;
        
        let row = sqlx::query(query)
            .bind(namespace)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => {
                let uid: String = row.get("uid");
                let access_modes_str: String = row.get("access_modes");
                let resources_str: String = row.get("resources");
                let storage_class_name: Option<String> = row.get("storage_class_name");
                let volume_mode: String = row.get("volume_mode");
                let volume_name: Option<String> = row.get("volume_name");
                let selector_str: Option<String> = row.get("selector");
                
                let phase: String = row.get("phase");
                let access_modes_status_str: Option<String> = row.get("access_modes_status");
                let capacity_str: Option<String> = row.get("capacity");
                
                let labels_str: String = row.get("labels");
                let annotations_str: String = row.get("annotations");
                let resource_version: i64 = row.get("resource_version");
                let creation_timestamp: String = row.get("creation_timestamp");

                let access_modes: Value = serde_json::from_str(&access_modes_str)?;
                let resources: Value = serde_json::from_str(&resources_str)?;
                let selector: Option<Value> = selector_str.map(|s| serde_json::from_str(&s)).transpose()?;
                let labels: Value = serde_json::from_str(&labels_str)?;
                let annotations: Value = serde_json::from_str(&annotations_str)?;
                let access_modes_status: Option<Value> = access_modes_status_str.map(|s| serde_json::from_str(&s)).transpose()?;
                let capacity: Option<Value> = capacity_str.map(|s| serde_json::from_str(&s)).transpose()?;

                let mut pvc = json!({
                    "apiVersion": "v1",
                    "kind": "PersistentVolumeClaim",
                    "metadata": {
                        "name": name,
                        "namespace": namespace,
                        "uid": uid,
                        "resourceVersion": resource_version.to_string(),
                        "creationTimestamp": creation_timestamp,
                        "selfLink": format!("/api/v1/namespaces/{}/persistentvolumeclaims/{}", namespace, name),
                    },
                    "spec": {
                        "accessModes": access_modes,
                        "resources": resources,
                        "volumeMode": volume_mode
                    },
                    "status": {
                        "phase": phase
                    }
                });
                
                // Add optional spec fields
                if let Some(sc) = storage_class_name {
                    pvc["spec"]["storageClassName"] = json!(sc);
                }
                if let Some(vn) = volume_name {
                    pvc["spec"]["volumeName"] = json!(vn);
                }
                if let Some(sel) = selector {
                    pvc["spec"]["selector"] = sel;
                }
                
                // Add status fields
                if let Some(am) = access_modes_status {
                    pvc["status"]["accessModes"] = am;
                }
                if let Some(cap) = capacity {
                    pvc["status"]["capacity"] = cap;
                }

                if !labels.is_null() && labels != json!({}) {
                    pvc["metadata"]["labels"] = labels;
                }

                if !annotations.is_null() && annotations != json!({}) {
                    pvc["metadata"]["annotations"] = annotations;
                }

                Ok(pvc)
            }
            None => Err(anyhow!("PersistentVolumeClaim {}/{} not found", namespace, name)),
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if namespace.is_some() {
            r#"
                SELECT uid, namespace, name, access_modes, resources, storage_class_name, volume_mode,
                       volume_name, selector, phase, access_modes_status, capacity,
                       labels, annotations, resource_version, creation_timestamp 
                FROM persistent_volume_claims 
                WHERE namespace = ?1 AND deletion_timestamp IS NULL 
                ORDER BY name
            "#
        } else {
            r#"
                SELECT uid, namespace, name, access_modes, resources, storage_class_name, volume_mode,
                       volume_name, selector, phase, access_modes_status, capacity,
                       labels, annotations, resource_version, creation_timestamp 
                FROM persistent_volume_claims 
                WHERE deletion_timestamp IS NULL 
                ORDER BY namespace, name
            "#
        };

        let rows = if let Some(ns) = namespace {
            sqlx::query(query)
                .bind(ns)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query(query)
                .fetch_all(&self.pool)
                .await?
        };

        let mut items = Vec::new();
        for row in rows {
            let uid: String = row.get("uid");
            let ns: String = row.get("namespace");
            let name: String = row.get("name");
            let access_modes_str: String = row.get("access_modes");
            let resources_str: String = row.get("resources");
            let storage_class_name: Option<String> = row.get("storage_class_name");
            let volume_mode: String = row.get("volume_mode");
            let volume_name: Option<String> = row.get("volume_name");
            let selector_str: Option<String> = row.get("selector");
            
            let phase: String = row.get("phase");
            let access_modes_status_str: Option<String> = row.get("access_modes_status");
            let capacity_str: Option<String> = row.get("capacity");
            
            let labels_str: String = row.get("labels");
            let annotations_str: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let creation_timestamp: String = row.get("creation_timestamp");
            
            let access_modes: Value = serde_json::from_str(&access_modes_str)?;
            let resources: Value = serde_json::from_str(&resources_str)?;
            let selector: Option<Value> = selector_str.map(|s| serde_json::from_str(&s)).transpose()?;
            let labels: Value = serde_json::from_str(&labels_str)?;
            let annotations: Value = serde_json::from_str(&annotations_str)?;
            let access_modes_status: Option<Value> = access_modes_status_str.map(|s| serde_json::from_str(&s)).transpose()?;
            let capacity: Option<Value> = capacity_str.map(|s| serde_json::from_str(&s)).transpose()?;

            let mut pvc = json!({
                "apiVersion": "v1",
                "kind": "PersistentVolumeClaim",
                "metadata": {
                    "name": name,
                    "namespace": ns,
                    "uid": uid,
                    "resourceVersion": resource_version.to_string(),
                    "creationTimestamp": creation_timestamp,
                    "selfLink": format!("/api/v1/namespaces/{}/persistentvolumeclaims/{}", ns, name),
                },
                "spec": {
                    "accessModes": access_modes,
                    "resources": resources,
                    "volumeMode": volume_mode
                },
                "status": {
                    "phase": phase
                }
            });
            
            if let Some(sc) = storage_class_name {
                pvc["spec"]["storageClassName"] = json!(sc);
            }
            if let Some(vn) = volume_name {
                pvc["spec"]["volumeName"] = json!(vn);
            }
            if let Some(sel) = selector {
                pvc["spec"]["selector"] = sel;
            }
            
            if let Some(am) = access_modes_status {
                pvc["status"]["accessModes"] = am;
            }
            if let Some(cap) = capacity {
                pvc["status"]["capacity"] = cap;
            }

            if !labels.is_null() && labels != json!({}) {
                pvc["metadata"]["labels"] = labels;
            }

            if !annotations.is_null() && annotations != json!({}) {
                pvc["metadata"]["annotations"] = annotations;
            }

            items.push(pvc);
        }

        Ok(json!({
            "apiVersion": "v1",
            "kind": "PersistentVolumeClaimList",
            "metadata": {
                "selfLink": if let Some(ns) = namespace {
                    format!("/api/v1/namespaces/{}/persistentvolumeclaims", ns)
                } else {
                    "/api/v1/persistentvolumeclaims".to_string()
                },
                "resourceVersion": "1",
            },
            "items": items,
        }))
    }

    pub async fn update(&self, namespace: &str, name: &str, pvc: Value) -> Result<Value> {
        // Extract spec fields
        let access_modes = pvc["spec"]["accessModes"].clone();
        if access_modes.is_null() {
            return Err(anyhow!("PersistentVolumeClaim accessModes is required"));
        }
        
        let resources = pvc["spec"]["resources"].clone();
        if resources.is_null() || resources["requests"]["storage"].is_null() {
            return Err(anyhow!("PersistentVolumeClaim storage request is required"));
        }
        
        let storage_class_name = pvc["spec"]
            .get("storageClassName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let volume_mode = pvc["spec"]
            .get("volumeMode")
            .and_then(|v| v.as_str())
            .unwrap_or("Filesystem")
            .to_string();
        
        let volume_name = pvc["spec"]
            .get("volumeName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let selector = pvc["spec"].get("selector").cloned();
        
        let labels = pvc["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = pvc["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        let update_query = r#"
            UPDATE persistent_volume_claims 
            SET access_modes = ?1, resources = ?2, storage_class_name = ?3, volume_mode = ?4,
                volume_name = ?5, selector = ?6, labels = ?7, annotations = ?8, 
                resource_version = resource_version + 1
            WHERE namespace = ?9 AND name = ?10 AND deletion_timestamp IS NULL
        "#;

        let rows_affected = sqlx::query(update_query)
            .bind(access_modes.to_string())
            .bind(resources.to_string())
            .bind(storage_class_name.as_deref())
            .bind(&volume_mode)
            .bind(volume_name.as_deref())
            .bind(selector.as_ref().map(|v| v.to_string()))
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?
            .rows_affected();

        if rows_affected == 0 {
            return Err(anyhow!("PersistentVolumeClaim {}/{} not found", namespace, name));
        }

        self.get(namespace, name).await
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        // Get the PVC before deletion
        let pvc = self.get(namespace, name).await?;

        // Soft delete
        let deletion_timestamp = Utc::now().to_rfc3339();
        let delete_query = "UPDATE persistent_volume_claims SET deletion_timestamp = ?1 WHERE namespace = ?2 AND name = ?3";
        
        sqlx::query(delete_query)
            .bind(&deletion_timestamp)
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(pvc)
    }

    pub async fn bind_to_volume(&self, namespace: &str, name: &str, volume_name: &str, capacity: &Value) -> Result<()> {
        let update_query = r#"
            UPDATE persistent_volume_claims 
            SET phase = 'Bound', volume_name = ?1, capacity = ?2,
                resource_version = resource_version + 1
            WHERE namespace = ?3 AND name = ?4 AND phase = 'Pending' AND deletion_timestamp IS NULL
        "#;

        let rows_affected = sqlx::query(update_query)
            .bind(volume_name)
            .bind(capacity.to_string())
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?
            .rows_affected();

        if rows_affected == 0 {
            return Err(anyhow!("Failed to bind PersistentVolumeClaim {}/{} - not pending", namespace, name));
        }

        Ok(())
    }

    pub async fn unbind(&self, namespace: &str, name: &str) -> Result<()> {
        let update_query = r#"
            UPDATE persistent_volume_claims 
            SET phase = 'Lost', resource_version = resource_version + 1
            WHERE namespace = ?1 AND name = ?2 AND phase = 'Bound' AND deletion_timestamp IS NULL
        "#;

        sqlx::query(update_query)
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}