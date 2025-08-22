use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct ReplicaSetStore {
    pool: SqlitePool,
}

impl ReplicaSetStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut replicaset: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = replicaset["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("ReplicaSet name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Set metadata fields
        replicaset["metadata"]["uid"] = json!(uid);
        replicaset["metadata"]["namespace"] = json!(namespace);
        replicaset["metadata"]["resourceVersion"] = json!("1");
        replicaset["metadata"]["creationTimestamp"] = json!(now);
        replicaset["metadata"]["generation"] = json!(1);
        replicaset["metadata"]["selfLink"] = json!(format!("/apis/apps/v1/namespaces/{}/replicasets/{}", namespace, name));
        
        // Set default replicas if not specified
        if replicaset["spec"]["replicas"].is_null() {
            replicaset["spec"]["replicas"] = json!(1);
        }
        
        // Set default status
        replicaset["status"] = json!({
            "replicas": 0,
            "fullyLabeledReplicas": 0,
            "readyReplicas": 0,
            "availableReplicas": 0,
            "observedGeneration": 1,
            "conditions": []
        });
        
        let labels = replicaset["metadata"]["labels"].to_string();
        let annotations = replicaset["metadata"]["annotations"].to_string();
        let spec = replicaset["spec"].to_string();
        let status = replicaset["status"].to_string();
        let owner_references = replicaset["metadata"]["ownerReferences"].to_string();
        
        sqlx::query(
            "INSERT INTO replicasets (uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, owner_references, replicas)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&uid)
        .bind(&name)
        .bind(namespace)
        .bind(1i64)
        .bind(&now)
        .bind(&labels)
        .bind(&annotations)
        .bind(&spec)
        .bind(&status)
        .bind(&owner_references)
        .bind(replicaset["spec"]["replicas"].as_i64().unwrap_or(1))
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("replicasets", &uid, &name, namespace, "ADDED", 1, &replicaset).await?;
        
        Ok(replicaset)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let row = sqlx::query(
            "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, owner_references
             FROM replicasets WHERE name = ? AND namespace = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .bind(namespace)
        .fetch_optional(&self.pool)
        .await?;
        
        match row {
            Some(row) => {
                let mut replicaset = json!({
                    "apiVersion": "apps/v1",
                    "kind": "ReplicaSet",
                    "metadata": {
                        "uid": row.get::<String, _>("uid"),
                        "name": row.get::<String, _>("name"),
                        "namespace": row.get::<String, _>("namespace"),
                        "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                        "creationTimestamp": row.get::<String, _>("creation_timestamp"),
                        "generation": 1,
                        "selfLink": format!("/apis/apps/v1/namespaces/{}/replicasets/{}", namespace, name)
                    },
                    "spec": serde_json::from_str::<Value>(&row.get::<String, _>("spec"))?,
                    "status": serde_json::from_str::<Value>(&row.get::<String, _>("status"))?
                });
                
                if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                    if !labels.is_null() {
                        replicaset["metadata"]["labels"] = labels;
                    }
                }
                
                if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                    if !annotations.is_null() {
                        replicaset["metadata"]["annotations"] = annotations;
                    }
                }
                
                if let Ok(owner_refs) = serde_json::from_str::<Value>(&row.get::<String, _>("owner_references")) {
                    if !owner_refs.is_null() {
                        replicaset["metadata"]["ownerReferences"] = owner_refs;
                    }
                }
                
                Ok(replicaset)
            }
            None => Err(anyhow!("ReplicaSet not found"))
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if let Some(ns) = namespace {
            sqlx::query(
                "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, owner_references
                 FROM replicasets WHERE namespace = ? AND deletion_timestamp IS NULL"
            )
            .bind(ns)
        } else {
            sqlx::query(
                "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, owner_references
                 FROM replicasets WHERE deletion_timestamp IS NULL"
            )
        };
        
        let rows = query.fetch_all(&self.pool).await?;
        
        let mut items = Vec::new();
        for row in rows {
            let mut replicaset = json!({
                "apiVersion": "apps/v1",
                "kind": "ReplicaSet",
                "metadata": {
                    "uid": row.get::<String, _>("uid"),
                    "name": row.get::<String, _>("name"),
                    "namespace": row.get::<String, _>("namespace"),
                    "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                    "creationTimestamp": row.get::<String, _>("creation_timestamp"),
                    "generation": 1,
                    "selfLink": format!("/apis/apps/v1/namespaces/{}/replicasets/{}", 
                        row.get::<String, _>("namespace"), 
                        row.get::<String, _>("name"))
                },
                "spec": serde_json::from_str::<Value>(&row.get::<String, _>("spec"))?,
                "status": serde_json::from_str::<Value>(&row.get::<String, _>("status"))?
            });
            
            if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                if !labels.is_null() {
                    replicaset["metadata"]["labels"] = labels;
                }
            }
            
            if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                if !annotations.is_null() {
                    replicaset["metadata"]["annotations"] = annotations;
                }
            }
            
            if let Ok(owner_refs) = serde_json::from_str::<Value>(&row.get::<String, _>("owner_references")) {
                if !owner_refs.is_null() {
                    replicaset["metadata"]["ownerReferences"] = owner_refs;
                }
            }
            
            items.push(replicaset);
        }
        
        Ok(json!({
            "apiVersion": "apps/v1",
            "kind": "ReplicaSetList",
            "metadata": {
                "resourceVersion": "1"
            },
            "items": items
        }))
    }

    pub async fn update_status(&self, namespace: &str, name: &str, status: Value) -> Result<()> {
        let replicaset = self.get(namespace, name).await?;
        let uid = replicaset["metadata"]["uid"].as_str().unwrap();
        
        sqlx::query(
            "UPDATE replicasets SET status = ? WHERE uid = ?"
        )
        .bind(status.to_string())
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<()> {
        let replicaset = self.get(namespace, name).await?;
        let uid = replicaset["metadata"]["uid"].as_str().unwrap();
        
        let now = Utc::now().to_rfc3339();
        
        sqlx::query(
            "UPDATE replicasets SET deletion_timestamp = ? WHERE uid = ?"
        )
        .bind(&now)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        let resource_version = replicaset["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse::<i64>()?;
        self.record_event("replicasets", uid, name, namespace, "DELETED", resource_version, &replicaset).await?;
        
        Ok(())
    }

    async fn record_event(&self, resource_type: &str, uid: &str, name: &str, namespace: &str, event_type: &str, version: i64, object: &Value) -> Result<()> {
        sqlx::query(
            "INSERT INTO events (resource_type, resource_uid, resource_name, resource_namespace, event_type, resource_version, timestamp, object)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(resource_type)
        .bind(uid)
        .bind(name)
        .bind(namespace)
        .bind(event_type)
        .bind(version)
        .bind(Utc::now().to_rfc3339())
        .bind(object.to_string())
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}