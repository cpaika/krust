use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct SecretStore {
    pool: SqlitePool,
}

impl SecretStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut secret: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = secret["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("Secret name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Handle stringData - convert to base64 encoded data
        let mut data = secret.get("data").unwrap_or(&json!({})).clone();
        if let Some(string_data) = secret.get("stringData") {
            if let Some(string_data_obj) = string_data.as_object() {
                if data.is_null() {
                    data = json!({});
                }
                if let Some(data_obj) = data.as_object_mut() {
                    for (key, value) in string_data_obj {
                        if let Some(string_value) = value.as_str() {
                            let encoded = STANDARD.encode(string_value.as_bytes());
                            data_obj.insert(key.clone(), json!(encoded));
                        }
                    }
                }
            }
        }
        
        // Extract fields
        let secret_type = secret.get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("Opaque")
            .to_string();
        let immutable = secret.get("immutable").and_then(|v| v.as_bool()).unwrap_or(false);
        
        let labels = secret["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = secret["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        // Insert into database
        let query = r#"
            INSERT INTO secrets (uid, namespace, name, type, data, immutable, labels, annotations, resource_version, creation_timestamp)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9)
        "#;
        
        sqlx::query(query)
            .bind(&uid)
            .bind(namespace)
            .bind(&name)
            .bind(&secret_type)
            .bind(data.to_string())
            .bind(immutable)
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(&now)
            .execute(&self.pool)
            .await?;

        // Build response (never include stringData in response)
        secret.as_object_mut().map(|obj| obj.remove("stringData"));
        
        secret["apiVersion"] = json!("v1");
        secret["kind"] = json!("Secret");
        secret["metadata"]["uid"] = json!(uid);
        secret["metadata"]["namespace"] = json!(namespace);
        secret["metadata"]["resourceVersion"] = json!("1");
        secret["metadata"]["creationTimestamp"] = json!(now);
        secret["metadata"]["selfLink"] = json!(format!("/api/v1/namespaces/{}/secrets/{}", namespace, name));
        secret["type"] = json!(secret_type);
        
        if !data.is_null() && data != json!({}) {
            secret["data"] = data;
        }
        
        if immutable {
            secret["immutable"] = json!(true);
        }

        if !labels.is_null() && labels != json!({}) {
            secret["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            secret["metadata"]["annotations"] = annotations;
        }

        Ok(secret)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let query = r#"
            SELECT uid, type, data, immutable, labels, annotations, resource_version, creation_timestamp 
            FROM secrets 
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
                let secret_type: String = row.get("type");
                let data_str: String = row.get("data");
                let immutable: bool = row.get("immutable");
                let labels_str: String = row.get("labels");
                let annotations_str: String = row.get("annotations");
                let resource_version: i64 = row.get("resource_version");
                let creation_timestamp: String = row.get("creation_timestamp");

                let data: Value = serde_json::from_str(&data_str)?;
                let labels: Value = serde_json::from_str(&labels_str)?;
                let annotations: Value = serde_json::from_str(&annotations_str)?;

                let mut secret = json!({
                    "apiVersion": "v1",
                    "kind": "Secret",
                    "metadata": {
                        "name": name,
                        "namespace": namespace,
                        "uid": uid,
                        "resourceVersion": resource_version.to_string(),
                        "creationTimestamp": creation_timestamp,
                        "selfLink": format!("/api/v1/namespaces/{}/secrets/{}", namespace, name),
                    },
                    "type": secret_type
                });
                
                if !data.is_null() && data != json!({}) {
                    secret["data"] = data;
                }
                
                if immutable {
                    secret["immutable"] = json!(true);
                }

                if !labels.is_null() && labels != json!({}) {
                    secret["metadata"]["labels"] = labels;
                }

                if !annotations.is_null() && annotations != json!({}) {
                    secret["metadata"]["annotations"] = annotations;
                }

                Ok(secret)
            }
            None => Err(anyhow!("Secret {}/{} not found", namespace, name)),
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if namespace.is_some() {
            r#"
                SELECT uid, namespace, name, type, data, immutable, labels, annotations, resource_version, creation_timestamp 
                FROM secrets 
                WHERE namespace = ?1 AND deletion_timestamp IS NULL 
                ORDER BY name
            "#
        } else {
            r#"
                SELECT uid, namespace, name, type, data, immutable, labels, annotations, resource_version, creation_timestamp 
                FROM secrets 
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
            let secret_type: String = row.get("type");
            let data_str: String = row.get("data");
            let immutable: bool = row.get("immutable");
            let labels_str: String = row.get("labels");
            let annotations_str: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let creation_timestamp: String = row.get("creation_timestamp");
            
            let data: Value = serde_json::from_str(&data_str)?;
            let labels: Value = serde_json::from_str(&labels_str)?;
            let annotations: Value = serde_json::from_str(&annotations_str)?;

            let mut secret = json!({
                "apiVersion": "v1",
                "kind": "Secret",
                "metadata": {
                    "name": name,
                    "namespace": ns,
                    "uid": uid,
                    "resourceVersion": resource_version.to_string(),
                    "creationTimestamp": creation_timestamp,
                    "selfLink": format!("/api/v1/namespaces/{}/secrets/{}", ns, name),
                },
                "type": secret_type
            });
            
            if !data.is_null() && data != json!({}) {
                secret["data"] = data;
            }
            
            if immutable {
                secret["immutable"] = json!(true);
            }

            if !labels.is_null() && labels != json!({}) {
                secret["metadata"]["labels"] = labels;
            }

            if !annotations.is_null() && annotations != json!({}) {
                secret["metadata"]["annotations"] = annotations;
            }

            items.push(secret);
        }

        Ok(json!({
            "apiVersion": "v1",
            "kind": "SecretList",
            "metadata": {
                "selfLink": if let Some(ns) = namespace {
                    format!("/api/v1/namespaces/{}/secrets", ns)
                } else {
                    "/api/v1/secrets".to_string()
                },
                "resourceVersion": "1",
            },
            "items": items,
        }))
    }

    pub async fn update(&self, namespace: &str, name: &str, mut secret: Value) -> Result<Value> {
        // Check if Secret exists and is not immutable
        let check_query = "SELECT uid, immutable FROM secrets WHERE namespace = ?1 AND name = ?2 AND deletion_timestamp IS NULL";
        let row = sqlx::query(check_query)
            .bind(namespace)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| anyhow!("Secret {}/{} not found", namespace, name))?;

        let _uid: String = row.get("uid");
        let is_immutable: bool = row.get("immutable");

        if is_immutable {
            return Err(anyhow!("Cannot update immutable Secret"));
        }

        // Handle stringData - convert to base64 encoded data
        let mut data = secret.get("data").unwrap_or(&json!({})).clone();
        if let Some(string_data) = secret.get("stringData") {
            if let Some(string_data_obj) = string_data.as_object() {
                if data.is_null() {
                    data = json!({});
                }
                if let Some(data_obj) = data.as_object_mut() {
                    for (key, value) in string_data_obj {
                        if let Some(string_value) = value.as_str() {
                            let encoded = STANDARD.encode(string_value.as_bytes());
                            data_obj.insert(key.clone(), json!(encoded));
                        }
                    }
                }
            }
        }

        let secret_type = secret.get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("Opaque")
            .to_string();
        let immutable = secret.get("immutable").and_then(|v| v.as_bool()).unwrap_or(false);
        
        let labels = secret["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = secret["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        let update_query = r#"
            UPDATE secrets 
            SET type = ?1, data = ?2, immutable = ?3, labels = ?4, annotations = ?5, resource_version = resource_version + 1
            WHERE namespace = ?6 AND name = ?7
        "#;

        sqlx::query(update_query)
            .bind(&secret_type)
            .bind(data.to_string())
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
        // Get existing Secret
        let mut existing = self.get(namespace, name).await?;
        
        // Check if immutable
        if existing.get("immutable").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Err(anyhow!("Cannot patch immutable Secret"));
        }

        // Handle stringData in patch
        let mut patch_data = patch.get("data").cloned();
        if let Some(string_data) = patch.get("stringData") {
            if let Some(string_data_obj) = string_data.as_object() {
                if patch_data.is_none() {
                    patch_data = Some(json!({}));
                }
                if let Some(data_obj) = patch_data.as_mut().and_then(|d| d.as_object_mut()) {
                    for (key, value) in string_data_obj {
                        if let Some(string_value) = value.as_str() {
                            let encoded = STANDARD.encode(string_value.as_bytes());
                            data_obj.insert(key.clone(), json!(encoded));
                        }
                    }
                }
            }
        }

        // Apply merge patch
        if let Some(data) = patch_data {
            if let Some(existing_data) = existing.get_mut("data") {
                if let (Some(existing_obj), Some(patch_obj)) = (existing_data.as_object_mut(), data.as_object()) {
                    for (key, value) in patch_obj {
                        existing_obj.insert(key.clone(), value.clone());
                    }
                }
            } else {
                existing["data"] = data;
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

        if let Some(secret_type) = patch.get("type") {
            existing["type"] = secret_type.clone();
        }

        // Update the Secret
        self.update(namespace, name, existing).await
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        // Get the Secret before deletion
        let secret = self.get(namespace, name).await?;

        // Soft delete
        let deletion_timestamp = Utc::now().to_rfc3339();
        let delete_query = "UPDATE secrets SET deletion_timestamp = ?1 WHERE namespace = ?2 AND name = ?3";
        
        sqlx::query(delete_query)
            .bind(&deletion_timestamp)
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(secret)
    }
}