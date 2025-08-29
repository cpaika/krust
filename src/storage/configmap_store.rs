use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct ConfigMapStore {
    pool: SqlitePool,
}

impl ConfigMapStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut configmap: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = configmap["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("ConfigMap name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Extract fields
        let data = configmap.get("data").unwrap_or(&json!({})).clone();
        let binary_data = configmap.get("binaryData").cloned();
        let immutable = configmap.get("immutable").and_then(|v| v.as_bool()).unwrap_or(false);
        
        let labels = configmap["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = configmap["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        // Insert into database
        let query = r#"
            INSERT INTO configmaps (uid, namespace, name, data, binary_data, immutable, labels, annotations, resource_version, creation_timestamp)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9)
        "#;
        
        sqlx::query(query)
            .bind(&uid)
            .bind(namespace)
            .bind(&name)
            .bind(data.to_string())
            .bind(binary_data.as_ref().map(|v| v.to_string()))
            .bind(immutable)
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(&now)
            .execute(&self.pool)
            .await?;

        // Build response
        configmap["apiVersion"] = json!("v1");
        configmap["kind"] = json!("ConfigMap");
        configmap["metadata"]["uid"] = json!(uid);
        configmap["metadata"]["namespace"] = json!(namespace);
        configmap["metadata"]["resourceVersion"] = json!("1");
        configmap["metadata"]["creationTimestamp"] = json!(now);
        configmap["metadata"]["selfLink"] = json!(format!("/api/v1/namespaces/{}/configmaps/{}", namespace, name));
        
        if !data.is_null() && data != json!({}) {
            configmap["data"] = data;
        }
        
        if let Some(bd) = binary_data {
            if !bd.is_null() {
                configmap["binaryData"] = bd.clone();
            }
        }
        
        if immutable {
            configmap["immutable"] = json!(true);
        }

        if !labels.is_null() && labels != json!({}) {
            configmap["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            configmap["metadata"]["annotations"] = annotations;
        }

        Ok(configmap)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let query = r#"
            SELECT uid, data, binary_data, immutable, labels, annotations, resource_version, creation_timestamp 
            FROM configmaps 
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
                let data_str: String = row.get("data");
                let binary_data_str: Option<String> = row.get("binary_data");
                let immutable: bool = row.get("immutable");
                let labels_str: String = row.get("labels");
                let annotations_str: String = row.get("annotations");
                let resource_version: i64 = row.get("resource_version");
                let creation_timestamp: String = row.get("creation_timestamp");

                let data: Value = serde_json::from_str(&data_str)?;
                let binary_data: Option<Value> = binary_data_str.map(|s| serde_json::from_str(&s)).transpose()?;
                let labels: Value = serde_json::from_str(&labels_str)?;
                let annotations: Value = serde_json::from_str(&annotations_str)?;

                let mut configmap = json!({
                    "apiVersion": "v1",
                    "kind": "ConfigMap",
                    "metadata": {
                        "name": name,
                        "namespace": namespace,
                        "uid": uid,
                        "resourceVersion": resource_version.to_string(),
                        "creationTimestamp": creation_timestamp,
                        "selfLink": format!("/api/v1/namespaces/{}/configmaps/{}", namespace, name),
                    }
                });
                
                if !data.is_null() && data != json!({}) {
                    configmap["data"] = data;
                }

                if let Some(bd) = binary_data {
                    if !bd.is_null() {
                        configmap["binaryData"] = bd;
                    }
                }
                
                if immutable {
                    configmap["immutable"] = json!(true);
                }

                if !labels.is_null() && labels != json!({}) {
                    configmap["metadata"]["labels"] = labels;
                }

                if !annotations.is_null() && annotations != json!({}) {
                    configmap["metadata"]["annotations"] = annotations;
                }

                Ok(configmap)
            }
            None => Err(anyhow!("ConfigMap {}/{} not found", namespace, name)),
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if namespace.is_some() {
            r#"
                SELECT uid, namespace, name, data, binary_data, immutable, labels, annotations, resource_version, creation_timestamp 
                FROM configmaps 
                WHERE namespace = ?1 AND deletion_timestamp IS NULL 
                ORDER BY name
            "#
        } else {
            r#"
                SELECT uid, namespace, name, data, binary_data, immutable, labels, annotations, resource_version, creation_timestamp 
                FROM configmaps 
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
            let data_str: String = row.get("data");
            let binary_data_str: Option<String> = row.get("binary_data");
            let immutable: bool = row.get("immutable");
            let labels_str: String = row.get("labels");
            let annotations_str: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let creation_timestamp: String = row.get("creation_timestamp");
            
            let data: Value = serde_json::from_str(&data_str)?;
            let binary_data: Option<Value> = binary_data_str.map(|s| serde_json::from_str(&s)).transpose()?;
            let labels: Value = serde_json::from_str(&labels_str)?;
            let annotations: Value = serde_json::from_str(&annotations_str)?;

            let mut configmap = json!({
                "apiVersion": "v1",
                "kind": "ConfigMap",
                "metadata": {
                    "name": name,
                    "namespace": ns,
                    "uid": uid,
                    "resourceVersion": resource_version.to_string(),
                    "creationTimestamp": creation_timestamp,
                    "selfLink": format!("/api/v1/namespaces/{}/configmaps/{}", ns, name),
                }
            });
            
            if !data.is_null() && data != json!({}) {
                configmap["data"] = data;
            }

            if let Some(bd) = binary_data {
                if !bd.is_null() {
                    configmap["binaryData"] = bd;
                }
            }
            
            if immutable {
                configmap["immutable"] = json!(true);
            }

            if !labels.is_null() && labels != json!({}) {
                configmap["metadata"]["labels"] = labels;
            }

            if !annotations.is_null() && annotations != json!({}) {
                configmap["metadata"]["annotations"] = annotations;
            }

            items.push(configmap);
        }

        Ok(json!({
            "apiVersion": "v1",
            "kind": "ConfigMapList",
            "metadata": {
                "selfLink": if let Some(ns) = namespace {
                    format!("/api/v1/namespaces/{}/configmaps", ns)
                } else {
                    "/api/v1/configmaps".to_string()
                },
                "resourceVersion": "1",
            },
            "items": items,
        }))
    }

    pub async fn update(&self, namespace: &str, name: &str, configmap: Value) -> Result<Value> {
        // Check if ConfigMap exists and is not immutable
        let check_query = "SELECT uid, immutable FROM configmaps WHERE namespace = ?1 AND name = ?2 AND deletion_timestamp IS NULL";
        let row = sqlx::query(check_query)
            .bind(namespace)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| anyhow!("ConfigMap {}/{} not found", namespace, name))?;

        let _uid: String = row.get("uid");
        let is_immutable: bool = row.get("immutable");

        if is_immutable {
            return Err(anyhow!("Cannot update immutable ConfigMap"));
        }

        let data = configmap.get("data").unwrap_or(&json!({})).clone();
        let binary_data = configmap.get("binaryData").cloned();
        let immutable = configmap.get("immutable").and_then(|v| v.as_bool()).unwrap_or(false);
        
        let labels = configmap["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = configmap["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        let update_query = r#"
            UPDATE configmaps 
            SET data = ?1, binary_data = ?2, immutable = ?3, labels = ?4, annotations = ?5, resource_version = resource_version + 1
            WHERE namespace = ?6 AND name = ?7
        "#;

        sqlx::query(update_query)
            .bind(data.to_string())
            .bind(binary_data.map(|v| v.to_string()))
            .bind(immutable)
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        self.get(namespace, name).await
    }

    pub async fn patch(&self, namespace: &str, name: &str, patch: Value) -> Result<Value> {
        // Get existing ConfigMap
        let mut existing = self.get(namespace, name).await?;
        
        // Check if immutable
        if existing.get("immutable").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Err(anyhow!("Cannot patch immutable ConfigMap"));
        }

        // Apply merge patch
        if let Some(data) = patch.get("data") {
            if let Some(existing_data) = existing.get_mut("data") {
                if let (Some(existing_obj), Some(patch_obj)) = (existing_data.as_object_mut(), data.as_object()) {
                    for (key, value) in patch_obj {
                        existing_obj.insert(key.clone(), value.clone());
                    }
                }
            } else {
                existing["data"] = data.clone();
            }
        }

        if let Some(binary_data) = patch.get("binaryData") {
            if let Some(existing_data) = existing.get_mut("binaryData") {
                if let (Some(existing_obj), Some(patch_obj)) = (existing_data.as_object_mut(), binary_data.as_object()) {
                    for (key, value) in patch_obj {
                        existing_obj.insert(key.clone(), value.clone());
                    }
                }
            } else {
                existing["binaryData"] = binary_data.clone();
            }
        }

        if let Some(metadata) = patch.get("metadata") {
            if let Some(labels) = metadata.get("labels") {
                existing["metadata"]["labels"] = labels.clone();
            }
            if let Some(annotations) = metadata.get("annotations") {
                existing["metadata"]["annotations"] = annotations.clone();
            }
        }

        if let Some(immutable) = patch.get("immutable") {
            existing["immutable"] = immutable.clone();
        }

        // Update the ConfigMap
        self.update(namespace, name, existing).await
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        // Get the ConfigMap before deletion
        let configmap = self.get(namespace, name).await?;

        // Soft delete
        let deletion_timestamp = Utc::now().to_rfc3339();
        let delete_query = "UPDATE configmaps SET deletion_timestamp = ?1 WHERE namespace = ?2 AND name = ?3";
        
        sqlx::query(delete_query)
            .bind(&deletion_timestamp)
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(configmap)
    }
}