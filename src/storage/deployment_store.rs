use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct DeploymentStore {
    pool: SqlitePool,
}

impl DeploymentStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut deployment: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = deployment["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("Deployment name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Set metadata fields
        deployment["metadata"]["uid"] = json!(uid);
        deployment["metadata"]["namespace"] = json!(namespace);
        deployment["metadata"]["resourceVersion"] = json!("1");
        deployment["metadata"]["creationTimestamp"] = json!(now);
        deployment["metadata"]["generation"] = json!(1);
        deployment["metadata"]["selfLink"] = json!(format!("/apis/apps/v1/namespaces/{}/deployments/{}", namespace, name));
        
        // Set default replicas if not specified
        if deployment["spec"]["replicas"].is_null() {
            deployment["spec"]["replicas"] = json!(1);
        }
        
        // Set default status
        deployment["status"] = json!({
            "observedGeneration": 1,
            "replicas": 0,
            "updatedReplicas": 0,
            "readyReplicas": 0,
            "availableReplicas": 0,
            "conditions": [
                {
                    "type": "Progressing",
                    "status": "True",
                    "lastUpdateTime": now,
                    "lastTransitionTime": now,
                    "reason": "NewDeploymentCreated",
                    "message": "Deployment is being created"
                }
            ]
        });
        
        let labels = deployment["metadata"]["labels"].to_string();
        let annotations = deployment["metadata"]["annotations"].to_string();
        let spec = deployment["spec"].to_string();
        let status = deployment["status"].to_string();
        
        sqlx::query(
            "INSERT INTO deployments (uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, generation, replicas)
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
        .bind(1i64)
        .bind(deployment["spec"]["replicas"].as_i64().unwrap_or(1))
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("deployments", &uid, &name, namespace, "ADDED", 1, &deployment).await?;
        
        Ok(deployment)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let row = sqlx::query(
            "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, generation
             FROM deployments WHERE name = ? AND namespace = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .bind(namespace)
        .fetch_optional(&self.pool)
        .await?;
        
        match row {
            Some(row) => {
                let mut deployment = json!({
                    "apiVersion": "apps/v1",
                    "kind": "Deployment",
                    "metadata": {
                        "uid": row.get::<String, _>("uid"),
                        "name": row.get::<String, _>("name"),
                        "namespace": row.get::<String, _>("namespace"),
                        "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                        "creationTimestamp": row.get::<String, _>("creation_timestamp"),
                        "generation": row.get::<i64, _>("generation"),
                        "selfLink": format!("/apis/apps/v1/namespaces/{}/deployments/{}", namespace, name)
                    },
                    "spec": serde_json::from_str::<Value>(&row.get::<String, _>("spec"))?,
                    "status": serde_json::from_str::<Value>(&row.get::<String, _>("status"))?
                });
                
                if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                    if !labels.is_null() {
                        deployment["metadata"]["labels"] = labels;
                    }
                }
                
                if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                    if !annotations.is_null() {
                        deployment["metadata"]["annotations"] = annotations;
                    }
                }
                
                Ok(deployment)
            }
            None => Err(anyhow!("Deployment not found"))
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if let Some(ns) = namespace {
            sqlx::query(
                "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, generation
                 FROM deployments WHERE namespace = ? AND deletion_timestamp IS NULL"
            )
            .bind(ns)
        } else {
            sqlx::query(
                "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, generation
                 FROM deployments WHERE deletion_timestamp IS NULL"
            )
        };
        
        let rows = query.fetch_all(&self.pool).await?;
        
        let mut items = Vec::new();
        for row in rows {
            let mut deployment = json!({
                "apiVersion": "apps/v1",
                "kind": "Deployment",
                "metadata": {
                    "uid": row.get::<String, _>("uid"),
                    "name": row.get::<String, _>("name"),
                    "namespace": row.get::<String, _>("namespace"),
                    "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                    "creationTimestamp": row.get::<String, _>("creation_timestamp"),
                    "generation": row.get::<i64, _>("generation"),
                    "selfLink": format!("/apis/apps/v1/namespaces/{}/deployments/{}", 
                        row.get::<String, _>("namespace"), 
                        row.get::<String, _>("name"))
                },
                "spec": serde_json::from_str::<Value>(&row.get::<String, _>("spec"))?,
                "status": serde_json::from_str::<Value>(&row.get::<String, _>("status"))?
            });
            
            if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                if !labels.is_null() {
                    deployment["metadata"]["labels"] = labels;
                }
            }
            
            if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                if !annotations.is_null() {
                    deployment["metadata"]["annotations"] = annotations;
                }
            }
            
            items.push(deployment);
        }
        
        Ok(json!({
            "apiVersion": "apps/v1",
            "kind": "DeploymentList",
            "metadata": {
                "resourceVersion": "1"
            },
            "items": items
        }))
    }

    pub async fn update(&self, namespace: &str, name: &str, mut deployment: Value) -> Result<Value> {
        // Get current deployment to check it exists
        let current = self.get(namespace, name).await?;
        let uid = current["metadata"]["uid"].as_str().unwrap();
        let resource_version = current["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse::<i64>()?;
        let generation = current["metadata"]["generation"].as_i64().unwrap();
        
        let new_version = resource_version + 1;
        let new_generation = if deployment["spec"] != current["spec"] {
            generation + 1
        } else {
            generation
        };
        
        // Update metadata
        deployment["metadata"]["uid"] = json!(uid);
        deployment["metadata"]["namespace"] = json!(namespace);
        deployment["metadata"]["resourceVersion"] = json!(new_version.to_string());
        deployment["metadata"]["generation"] = json!(new_generation);
        
        let labels = deployment["metadata"]["labels"].to_string();
        let annotations = deployment["metadata"]["annotations"].to_string();
        let spec = deployment["spec"].to_string();
        let status = deployment["status"].to_string();
        let replicas = deployment["spec"]["replicas"].as_i64().unwrap_or(1);
        
        sqlx::query(
            "UPDATE deployments SET resource_version = ?, generation = ?, labels = ?, annotations = ?, spec = ?, status = ?, replicas = ?
             WHERE uid = ?"
        )
        .bind(new_version)
        .bind(new_generation)
        .bind(&labels)
        .bind(&annotations)
        .bind(&spec)
        .bind(&status)
        .bind(replicas)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("deployments", uid, name, namespace, "MODIFIED", new_version, &deployment).await?;
        
        Ok(deployment)
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<()> {
        let deployment = self.get(namespace, name).await?;
        let uid = deployment["metadata"]["uid"].as_str().unwrap();
        
        let now = Utc::now().to_rfc3339();
        
        sqlx::query(
            "UPDATE deployments SET deletion_timestamp = ? WHERE uid = ?"
        )
        .bind(&now)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        let resource_version = deployment["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse::<i64>()?;
        self.record_event("deployments", uid, name, namespace, "DELETED", resource_version, &deployment).await?;
        
        Ok(())
    }

    pub async fn update_status(&self, namespace: &str, name: &str, status: Value) -> Result<()> {
        let deployment = self.get(namespace, name).await?;
        let uid = deployment["metadata"]["uid"].as_str().unwrap();
        
        sqlx::query(
            "UPDATE deployments SET status = ? WHERE uid = ?"
        )
        .bind(status.to_string())
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