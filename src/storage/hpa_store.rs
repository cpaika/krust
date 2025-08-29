use anyhow::Result;
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct HpaStore {
    pool: SqlitePool,
}

impl HpaStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut hpa: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        
        // Set metadata fields
        hpa["metadata"]["uid"] = json!(uid);
        hpa["metadata"]["namespace"] = json!(namespace);
        hpa["metadata"]["creationTimestamp"] = json!(now);
        hpa["metadata"]["resourceVersion"] = json!("1");
        hpa["metadata"]["generation"] = json!(1);
        
        // Initialize status if not present
        if hpa["status"].is_null() {
            hpa["status"] = json!({
                "currentReplicas": 0,
                "desiredReplicas": 0,
                "conditions": []
            });
        }
        
        let name = hpa["metadata"]["name"].as_str().unwrap();
        let spec = hpa["spec"].to_string();
        let status = hpa["status"].to_string();
        let labels = hpa["metadata"]["labels"].to_string();
        let annotations = hpa["metadata"]["annotations"].to_string();
        
        sqlx::query(
            "INSERT INTO horizontalpodautoscalers (uid, name, namespace, spec, status, labels, annotations, resource_version, generation, creation_timestamp)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&uid)
        .bind(name)
        .bind(namespace)
        .bind(spec)
        .bind(status)
        .bind(labels)
        .bind(annotations)
        .bind(1i64)
        .bind(1i64)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("HorizontalPodAutoscaler", &uid, name, namespace, "ADDED", 1, &hpa).await?;
        
        Ok(hpa)
    }
    
    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let row = sqlx::query(
            "SELECT * FROM horizontalpodautoscalers WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(namespace)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        
        match row {
            Some(row) => {
                let mut hpa = json!({
                    "apiVersion": "autoscaling/v2",
                    "kind": "HorizontalPodAutoscaler",
                    "metadata": {
                        "name": row.get::<String, _>("name"),
                        "namespace": row.get::<String, _>("namespace"),
                        "uid": row.get::<String, _>("uid"),
                        "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                        "generation": row.get::<i64, _>("generation"),
                        "creationTimestamp": row.get::<String, _>("creation_timestamp")
                    },
                    "spec": serde_json::from_str::<Value>(&row.get::<String, _>("spec"))?,
                    "status": serde_json::from_str::<Value>(&row.get::<String, _>("status"))?
                });
                
                if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                    if !labels.is_null() {
                        hpa["metadata"]["labels"] = labels;
                    }
                }
                
                if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                    if !annotations.is_null() {
                        hpa["metadata"]["annotations"] = annotations;
                    }
                }
                
                Ok(hpa)
            }
            None => anyhow::bail!("HorizontalPodAutoscaler {}/{} not found", namespace, name)
        }
    }
    
    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let rows = if let Some(ns) = namespace {
            sqlx::query(
                "SELECT * FROM horizontalpodautoscalers WHERE namespace = ? AND deletion_timestamp IS NULL ORDER BY creation_timestamp DESC"
            )
            .bind(ns)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT * FROM horizontalpodautoscalers WHERE deletion_timestamp IS NULL ORDER BY creation_timestamp DESC"
            )
            .fetch_all(&self.pool)
            .await?
        };
        
        let mut items = Vec::new();
        for row in rows {
            let mut hpa = json!({
                "apiVersion": "autoscaling/v2",
                "kind": "HorizontalPodAutoscaler",
                "metadata": {
                    "name": row.get::<String, _>("name"),
                    "namespace": row.get::<String, _>("namespace"),
                    "uid": row.get::<String, _>("uid"),
                    "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                    "generation": row.get::<i64, _>("generation"),
                    "creationTimestamp": row.get::<String, _>("creation_timestamp")
                },
                "spec": serde_json::from_str::<Value>(&row.get::<String, _>("spec"))?,
                "status": serde_json::from_str::<Value>(&row.get::<String, _>("status"))?
            });
            
            if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                if !labels.is_null() {
                    hpa["metadata"]["labels"] = labels;
                }
            }
            
            if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                if !annotations.is_null() {
                    hpa["metadata"]["annotations"] = annotations;
                }
            }
            
            items.push(hpa);
        }
        
        Ok(json!({
            "apiVersion": "autoscaling/v2",
            "kind": "HorizontalPodAutoscalerList",
            "metadata": {
                "resourceVersion": "1"
            },
            "items": items
        }))
    }
    
    pub async fn update(&self, namespace: &str, name: &str, mut hpa: Value) -> Result<Value> {
        // Get current HPA to check it exists
        let current = self.get(namespace, name).await?;
        let uid = current["metadata"]["uid"].as_str().unwrap();
        let current_version: i64 = current["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse()?;
        let current_generation: i64 = current["metadata"]["generation"].as_i64().unwrap();
        
        let new_version = current_version + 1;
        let new_generation = if hpa["spec"] != current["spec"] {
            current_generation + 1
        } else {
            current_generation
        };
        
        // Update metadata
        hpa["metadata"]["resourceVersion"] = json!(new_version.to_string());
        hpa["metadata"]["generation"] = json!(new_generation);
        hpa["metadata"]["uid"] = json!(uid);
        
        let spec = hpa["spec"].to_string();
        let status = hpa["status"].to_string();
        let labels = hpa["metadata"]["labels"].to_string();
        let annotations = hpa["metadata"]["annotations"].to_string();
        
        sqlx::query(
            "UPDATE horizontalpodautoscalers SET spec = ?, status = ?, labels = ?, annotations = ?, resource_version = ?, generation = ?
             WHERE uid = ?"
        )
        .bind(spec)
        .bind(status)
        .bind(labels)
        .bind(annotations)
        .bind(new_version)
        .bind(new_generation)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("HorizontalPodAutoscaler", uid, name, namespace, "MODIFIED", new_version, &hpa).await?;
        
        Ok(hpa)
    }
    
    pub async fn update_status(&self, namespace: &str, name: &str, status: Value) -> Result<Value> {
        let mut hpa = self.get(namespace, name).await?;
        let uid = hpa["metadata"]["uid"].as_str().unwrap().to_string();
        let current_version: i64 = hpa["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse()?;
        
        let new_version = current_version + 1;
        
        // Update the status
        hpa["status"] = status.clone();
        hpa["metadata"]["resourceVersion"] = json!(new_version.to_string());
        
        // Update in database
        sqlx::query(
            "UPDATE horizontalpodautoscalers SET status = ?, resource_version = ? WHERE uid = ?"
        )
        .bind(status.to_string())
        .bind(new_version)
        .bind(&uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("HorizontalPodAutoscaler", &uid, name, namespace, "MODIFIED", new_version, &hpa).await?;
        
        Ok(hpa)
    }
    
    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        let mut hpa = self.get(namespace, name).await?;
        let uid = hpa["metadata"]["uid"].as_str().unwrap().to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Set deletion timestamp in the object
        hpa["metadata"]["deletionTimestamp"] = json!(now.clone());
        
        sqlx::query(
            "UPDATE horizontalpodautoscalers SET deletion_timestamp = ? WHERE uid = ?"
        )
        .bind(&now)
        .bind(&uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        let resource_version = hpa["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse::<i64>()?;
        self.record_event("HorizontalPodAutoscaler", &uid, name, namespace, "DELETED", resource_version, &hpa).await?;
        
        Ok(hpa)
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