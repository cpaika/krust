use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::models::pod::Pod;

pub struct PodStore {
    pool: SqlitePool,
}

impl PodStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut pod: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = pod["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("Pod name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Set metadata fields
        pod["metadata"]["uid"] = json!(uid);
        pod["metadata"]["namespace"] = json!(namespace);
        pod["metadata"]["resourceVersion"] = json!("1");
        pod["metadata"]["creationTimestamp"] = json!(now);
        pod["metadata"]["selfLink"] = json!(format!("/api/v1/namespaces/{}/pods/{}", namespace, name));
        
        // Set default status
        if pod["status"].is_null() {
            pod["status"] = json!({
                "phase": "Pending",
                "conditions": []
            });
        }
        
        let labels = pod["metadata"]["labels"].to_string();
        let annotations = pod["metadata"]["annotations"].to_string();
        let spec = pod["spec"].to_string();
        let status = pod["status"].to_string();
        
        sqlx::query(
            "INSERT INTO pods (uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, phase)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
        .bind("Pending")
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("pods", &uid, &name, namespace, "ADDED", 1, &pod).await?;
        
        Ok(pod)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let row = sqlx::query(
            "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, node_name, phase
             FROM pods WHERE name = ? AND namespace = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .bind(namespace)
        .fetch_optional(&self.pool)
        .await?;
        
        match row {
            Some(row) => {
                let mut pod = json!({
                    "apiVersion": "v1",
                    "kind": "Pod",
                    "metadata": {
                        "uid": row.get::<String, _>("uid"),
                        "name": row.get::<String, _>("name"),
                        "namespace": row.get::<String, _>("namespace"),
                        "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                        "creationTimestamp": row.get::<String, _>("creation_timestamp"),
                        "selfLink": format!("/api/v1/namespaces/{}/pods/{}", namespace, name)
                    },
                    "spec": serde_json::from_str::<Value>(&row.get::<String, _>("spec"))?,
                    "status": serde_json::from_str::<Value>(&row.get::<String, _>("status"))?
                });
                
                if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                    if !labels.is_null() {
                        pod["metadata"]["labels"] = labels;
                    }
                }
                
                if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                    if !annotations.is_null() {
                        pod["metadata"]["annotations"] = annotations;
                    }
                }
                
                Ok(pod)
            }
            None => Err(anyhow!("Pod not found"))
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if let Some(ns) = namespace {
            sqlx::query(
                "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, node_name, phase
                 FROM pods WHERE namespace = ? AND deletion_timestamp IS NULL"
            )
            .bind(ns)
        } else {
            sqlx::query(
                "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, node_name, phase
                 FROM pods WHERE deletion_timestamp IS NULL"
            )
        };
        
        let rows = query.fetch_all(&self.pool).await?;
        
        let mut items = Vec::new();
        for row in rows {
            let mut pod = json!({
                "apiVersion": "v1",
                "kind": "Pod",
                "metadata": {
                    "uid": row.get::<String, _>("uid"),
                    "name": row.get::<String, _>("name"),
                    "namespace": row.get::<String, _>("namespace"),
                    "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                    "creationTimestamp": row.get::<String, _>("creation_timestamp"),
                    "selfLink": format!("/api/v1/namespaces/{}/pods/{}", 
                        row.get::<String, _>("namespace"), 
                        row.get::<String, _>("name"))
                },
                "spec": serde_json::from_str::<Value>(&row.get::<String, _>("spec"))?,
                "status": serde_json::from_str::<Value>(&row.get::<String, _>("status"))?
            });
            
            if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                if !labels.is_null() {
                    pod["metadata"]["labels"] = labels;
                }
            }
            
            if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                if !annotations.is_null() {
                    pod["metadata"]["annotations"] = annotations;
                }
            }
            
            items.push(pod);
        }
        
        Ok(json!({
            "apiVersion": "v1",
            "kind": "PodList",
            "metadata": {
                "resourceVersion": "1"
            },
            "items": items
        }))
    }

    pub async fn update(&self, namespace: &str, name: &str, mut pod: Value) -> Result<Value> {
        // Get current pod to check it exists
        let current = self.get(namespace, name).await?;
        let uid = current["metadata"]["uid"].as_str().unwrap();
        let resource_version = current["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse::<i64>()?;
        
        let new_version = resource_version + 1;
        
        // Update metadata
        pod["metadata"]["uid"] = json!(uid);
        pod["metadata"]["namespace"] = json!(namespace);
        pod["metadata"]["resourceVersion"] = json!(new_version.to_string());
        
        let labels = pod["metadata"]["labels"].to_string();
        let annotations = pod["metadata"]["annotations"].to_string();
        let spec = pod["spec"].to_string();
        let status = pod["status"].to_string();
        
        sqlx::query(
            "UPDATE pods SET resource_version = ?, labels = ?, annotations = ?, spec = ?, status = ?
             WHERE uid = ?"
        )
        .bind(new_version)
        .bind(&labels)
        .bind(&annotations)
        .bind(&spec)
        .bind(&status)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("pods", uid, name, namespace, "MODIFIED", new_version, &pod).await?;
        
        Ok(pod)
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<()> {
        let pod = self.get(namespace, name).await?;
        let uid = pod["metadata"]["uid"].as_str().unwrap();
        let resource_version = pod["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse::<i64>()?;
        
        let now = Utc::now().to_rfc3339();
        
        sqlx::query(
            "UPDATE pods SET deletion_timestamp = ? WHERE uid = ?"
        )
        .bind(&now)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("pods", uid, name, namespace, "DELETED", resource_version, &pod).await?;
        
        Ok(())
    }

    pub async fn update_status(&self, namespace: &str, name: &str, phase: &str, node_name: Option<&str>) -> Result<()> {
        let pod = self.get(namespace, name).await?;
        let uid = pod["metadata"]["uid"].as_str().unwrap();
        
        let mut status = pod["status"].clone();
        status["phase"] = json!(phase);
        
        if phase == "Running" {
            status["startTime"] = json!(Utc::now().to_rfc3339());
            status["conditions"] = json!([
                {
                    "type": "Ready",
                    "status": "True",
                    "lastTransitionTime": Utc::now().to_rfc3339()
                }
            ]);
        }
        
        sqlx::query(
            "UPDATE pods SET phase = ?, node_name = ?, status = ? WHERE uid = ?"
        )
        .bind(phase)
        .bind(node_name)
        .bind(&status.to_string())
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
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