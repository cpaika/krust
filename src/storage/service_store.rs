use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;
use std::collections::HashSet;

pub struct ServiceStore {
    pool: SqlitePool,
    allocated_ips: std::sync::Arc<std::sync::Mutex<HashSet<String>>>,
}

impl ServiceStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { 
            pool,
            allocated_ips: std::sync::Arc::new(std::sync::Mutex::new(HashSet::new())),
        }
    }

    pub async fn create(&self, namespace: &str, mut service: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = service["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("Service name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Set metadata fields
        service["metadata"]["uid"] = json!(uid);
        service["metadata"]["namespace"] = json!(namespace);
        service["metadata"]["resourceVersion"] = json!("1");
        service["metadata"]["creationTimestamp"] = json!(now);
        service["metadata"]["selfLink"] = json!(format!("/api/v1/namespaces/{}/services/{}", namespace, name));
        
        // Allocate ClusterIP if type is ClusterIP (default)
        let service_type = service["spec"]["type"].as_str().unwrap_or("ClusterIP");
        let cluster_ip = if service_type == "ClusterIP" {
            let ip = self.allocate_cluster_ip()?;
            service["spec"]["clusterIP"] = json!(ip.clone());
            Some(ip)
        } else {
            None
        };
        
        // Set default ports if not specified
        if service["spec"]["ports"].is_null() {
            service["spec"]["ports"] = json!([]);
        }
        
        // Set status
        service["status"] = json!({
            "loadBalancer": {}
        });
        
        let labels = service["metadata"]["labels"].to_string();
        let annotations = service["metadata"]["annotations"].to_string();
        let spec = service["spec"].to_string();
        let status = service["status"].to_string();
        
        sqlx::query(
            "INSERT INTO services (uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, cluster_ip)
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
        .bind(&cluster_ip)
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("services", &uid, &name, namespace, "ADDED", 1, &service).await?;
        
        // Create corresponding endpoints if selector exists
        if !service["spec"]["selector"].is_null() {
            let endpoints = json!({
                "metadata": {
                    "name": name.clone(),
                    "namespace": namespace
                },
                "subsets": []
            });
            
            // Try to create endpoints (ignore if already exists)
            let _ = sqlx::query(
                "INSERT INTO endpoints (uid, name, namespace, resource_version, creation_timestamp, labels, annotations, subsets)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(name, namespace) DO NOTHING"
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&name)
            .bind(namespace)
            .bind(1i64)
            .bind(&now)
            .bind("{}")
            .bind("{}")
            .bind("[]")
            .execute(&self.pool)
            .await;
        }
        
        Ok(service)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let row = sqlx::query(
            "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, cluster_ip
             FROM services WHERE name = ? AND namespace = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .bind(namespace)
        .fetch_optional(&self.pool)
        .await?;
        
        match row {
            Some(row) => {
                let mut service = json!({
                    "apiVersion": "v1",
                    "kind": "Service",
                    "metadata": {
                        "uid": row.get::<String, _>("uid"),
                        "name": row.get::<String, _>("name"),
                        "namespace": row.get::<String, _>("namespace"),
                        "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                        "creationTimestamp": row.get::<String, _>("creation_timestamp"),
                        "selfLink": format!("/api/v1/namespaces/{}/services/{}", namespace, name)
                    },
                    "spec": serde_json::from_str::<Value>(&row.get::<String, _>("spec"))?,
                    "status": serde_json::from_str::<Value>(&row.get::<String, _>("status"))?
                });
                
                if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                    if !labels.is_null() {
                        service["metadata"]["labels"] = labels;
                    }
                }
                
                if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                    if !annotations.is_null() {
                        service["metadata"]["annotations"] = annotations;
                    }
                }
                
                Ok(service)
            }
            None => Err(anyhow!("Service not found"))
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if let Some(ns) = namespace {
            sqlx::query(
                "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, cluster_ip
                 FROM services WHERE namespace = ? AND deletion_timestamp IS NULL"
            )
            .bind(ns)
        } else {
            sqlx::query(
                "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, spec, status, cluster_ip
                 FROM services WHERE deletion_timestamp IS NULL"
            )
        };
        
        let rows = query.fetch_all(&self.pool).await?;
        
        let mut items = Vec::new();
        for row in rows {
            let mut service = json!({
                "apiVersion": "v1",
                "kind": "Service",
                "metadata": {
                    "uid": row.get::<String, _>("uid"),
                    "name": row.get::<String, _>("name"),
                    "namespace": row.get::<String, _>("namespace"),
                    "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                    "creationTimestamp": row.get::<String, _>("creation_timestamp"),
                    "selfLink": format!("/api/v1/namespaces/{}/services/{}", 
                        row.get::<String, _>("namespace"), 
                        row.get::<String, _>("name"))
                },
                "spec": serde_json::from_str::<Value>(&row.get::<String, _>("spec"))?,
                "status": serde_json::from_str::<Value>(&row.get::<String, _>("status"))?
            });
            
            if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                if !labels.is_null() {
                    service["metadata"]["labels"] = labels;
                }
            }
            
            if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                if !annotations.is_null() {
                    service["metadata"]["annotations"] = annotations;
                }
            }
            
            items.push(service);
        }
        
        Ok(json!({
            "apiVersion": "v1",
            "kind": "ServiceList",
            "metadata": {
                "resourceVersion": "1"
            },
            "items": items
        }))
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<()> {
        let service = self.get(namespace, name).await?;
        let uid = service["metadata"]["uid"].as_str().unwrap();
        
        // Release ClusterIP
        if let Some(cluster_ip) = service["spec"]["clusterIP"].as_str() {
            self.release_cluster_ip(cluster_ip);
        }
        
        let now = Utc::now().to_rfc3339();
        
        sqlx::query(
            "UPDATE services SET deletion_timestamp = ? WHERE uid = ?"
        )
        .bind(&now)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        let resource_version = service["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse::<i64>()?;
        self.record_event("services", uid, name, namespace, "DELETED", resource_version, &service).await?;
        
        Ok(())
    }

    fn allocate_cluster_ip(&self) -> Result<String> {
        let mut allocated = self.allocated_ips.lock().unwrap();
        
        // Simple allocation from 10.96.0.0/12 range
        for i in 10..250 {
            let ip = format!("10.96.0.{}", i);
            if !allocated.contains(&ip) {
                allocated.insert(ip.clone());
                return Ok(ip);
            }
        }
        
        Err(anyhow!("No available ClusterIP addresses"))
    }

    fn release_cluster_ip(&self, ip: &str) {
        let mut allocated = self.allocated_ips.lock().unwrap();
        allocated.remove(ip);
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

    pub async fn get_endpoints_for_service(&self, namespace: &str, name: &str) -> Result<Vec<String>> {
        // Get the service to find its selector
        let service = self.get(namespace, name).await?;
        let selector = &service["spec"]["selector"];
        
        if selector.is_null() {
            return Ok(Vec::new());
        }
        
        // Find pods matching the selector
        let mut endpoints = Vec::new();
        
        // Query pods with matching labels
        let pods_query = sqlx::query(
            "SELECT name, labels, status FROM pods 
             WHERE namespace = ? AND deletion_timestamp IS NULL AND phase = 'Running'"
        )
        .bind(namespace)
        .fetch_all(&self.pool)
        .await?;
        
        for row in pods_query {
            let labels_str: String = row.get("labels");
            if let Ok(pod_labels) = serde_json::from_str::<Value>(&labels_str) {
                // Check if pod labels match service selector
                if Self::labels_match(&pod_labels, selector) {
                    let status_str: String = row.get("status");
                    if let Ok(status) = serde_json::from_str::<Value>(&status_str) {
                        if let Some(pod_ip) = status["podIP"].as_str() {
                            endpoints.push(pod_ip.to_string());
                        }
                    }
                }
            }
        }
        
        Ok(endpoints)
    }

    fn labels_match(pod_labels: &Value, selector: &Value) -> bool {
        if let Some(selector_obj) = selector.as_object() {
            for (key, value) in selector_obj {
                if pod_labels[key] != *value {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }
}