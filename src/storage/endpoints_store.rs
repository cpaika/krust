use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct EndpointsStore {
    pool: SqlitePool,
}

impl EndpointsStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut endpoints: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = endpoints["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("Endpoints name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Set metadata fields
        endpoints["metadata"]["uid"] = json!(uid);
        endpoints["metadata"]["namespace"] = json!(namespace);
        endpoints["metadata"]["resourceVersion"] = json!("1");
        endpoints["metadata"]["creationTimestamp"] = json!(now);
        endpoints["metadata"]["selfLink"] = json!(format!("/api/v1/namespaces/{}/endpoints/{}", namespace, name));
        
        // Set default subsets if not specified
        if endpoints["subsets"].is_null() {
            endpoints["subsets"] = json!([]);
        }
        
        let labels = endpoints["metadata"]["labels"].to_string();
        let annotations = endpoints["metadata"]["annotations"].to_string();
        let subsets = endpoints["subsets"].to_string();
        
        sqlx::query(
            "INSERT INTO endpoints (uid, name, namespace, resource_version, creation_timestamp, labels, annotations, subsets)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&uid)
        .bind(&name)
        .bind(namespace)
        .bind(1i64)
        .bind(&now)
        .bind(&labels)
        .bind(&annotations)
        .bind(&subsets)
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("endpoints", &uid, &name, namespace, "ADDED", 1, &endpoints).await?;
        
        Ok(endpoints)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let row = sqlx::query(
            "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, subsets
             FROM endpoints WHERE name = ? AND namespace = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .bind(namespace)
        .fetch_optional(&self.pool)
        .await?;
        
        match row {
            Some(row) => {
                let mut endpoints = json!({
                    "apiVersion": "v1",
                    "kind": "Endpoints",
                    "metadata": {
                        "uid": row.get::<String, _>("uid"),
                        "name": row.get::<String, _>("name"),
                        "namespace": row.get::<String, _>("namespace"),
                        "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                        "creationTimestamp": row.get::<String, _>("creation_timestamp"),
                        "selfLink": format!("/api/v1/namespaces/{}/endpoints/{}", namespace, name)
                    },
                    "subsets": serde_json::from_str::<Value>(&row.get::<String, _>("subsets"))?
                });
                
                if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                    if !labels.is_null() {
                        endpoints["metadata"]["labels"] = labels;
                    }
                }
                
                if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                    if !annotations.is_null() {
                        endpoints["metadata"]["annotations"] = annotations;
                    }
                }
                
                Ok(endpoints)
            }
            None => Err(anyhow!("Endpoints not found"))
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if let Some(ns) = namespace {
            sqlx::query(
                "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, subsets
                 FROM endpoints WHERE namespace = ? AND deletion_timestamp IS NULL"
            )
            .bind(ns)
        } else {
            sqlx::query(
                "SELECT uid, name, namespace, resource_version, creation_timestamp, labels, annotations, subsets
                 FROM endpoints WHERE deletion_timestamp IS NULL"
            )
        };
        
        let rows = query.fetch_all(&self.pool).await?;
        
        let mut items = Vec::new();
        for row in rows {
            let mut endpoints = json!({
                "apiVersion": "v1",
                "kind": "Endpoints",
                "metadata": {
                    "uid": row.get::<String, _>("uid"),
                    "name": row.get::<String, _>("name"),
                    "namespace": row.get::<String, _>("namespace"),
                    "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                    "creationTimestamp": row.get::<String, _>("creation_timestamp"),
                    "selfLink": format!("/api/v1/namespaces/{}/endpoints/{}", 
                        row.get::<String, _>("namespace"), 
                        row.get::<String, _>("name"))
                },
                "subsets": serde_json::from_str::<Value>(&row.get::<String, _>("subsets"))?
            });
            
            if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                if !labels.is_null() {
                    endpoints["metadata"]["labels"] = labels;
                }
            }
            
            if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                if !annotations.is_null() {
                    endpoints["metadata"]["annotations"] = annotations;
                }
            }
            
            items.push(endpoints);
        }
        
        Ok(json!({
            "apiVersion": "v1",
            "kind": "EndpointsList",
            "metadata": {
                "resourceVersion": "1"
            },
            "items": items
        }))
    }

    pub async fn update(&self, namespace: &str, name: &str, mut endpoints: Value) -> Result<Value> {
        // Get current endpoints to check it exists
        let current = self.get(namespace, name).await?;
        let uid = current["metadata"]["uid"].as_str().unwrap();
        let resource_version = current["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse::<i64>()?;
        
        let new_version = resource_version + 1;
        
        // Update metadata
        endpoints["metadata"]["uid"] = json!(uid);
        endpoints["metadata"]["namespace"] = json!(namespace);
        endpoints["metadata"]["resourceVersion"] = json!(new_version.to_string());
        
        let labels = endpoints["metadata"]["labels"].to_string();
        let annotations = endpoints["metadata"]["annotations"].to_string();
        let subsets = endpoints["subsets"].to_string();
        
        sqlx::query(
            "UPDATE endpoints SET resource_version = ?, labels = ?, annotations = ?, subsets = ?
             WHERE uid = ?"
        )
        .bind(new_version)
        .bind(&labels)
        .bind(&annotations)
        .bind(&subsets)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("endpoints", uid, name, namespace, "MODIFIED", new_version, &endpoints).await?;
        
        Ok(endpoints)
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<()> {
        let endpoints = self.get(namespace, name).await?;
        let uid = endpoints["metadata"]["uid"].as_str().unwrap();
        
        let now = Utc::now().to_rfc3339();
        
        sqlx::query(
            "UPDATE endpoints SET deletion_timestamp = ? WHERE uid = ?"
        )
        .bind(&now)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        let resource_version = endpoints["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse::<i64>()?;
        self.record_event("endpoints", uid, name, namespace, "DELETED", resource_version, &endpoints).await?;
        
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

    pub async fn update_for_service(&self, service_namespace: &str, service_name: &str, service_selector: &Value) -> Result<()> {
        // Find all running pods that match the selector
        let mut pod_ips = Vec::new();
        
        if !service_selector.is_null() && service_selector.is_object() {
            // Build the label query
            let selector_map = service_selector.as_object().unwrap();
            
            // Query pods with matching labels
            let rows = sqlx::query(
                "SELECT name, labels, status FROM pods 
                 WHERE namespace = ? AND deletion_timestamp IS NULL AND phase = 'Running'"
            )
            .bind(service_namespace)
            .fetch_all(&self.pool)
            .await?;
            
            for row in rows {
                let labels_str: String = row.get("labels");
                if let Ok(pod_labels) = serde_json::from_str::<Value>(&labels_str) {
                    // Check if pod labels match service selector
                    if Self::labels_match(&pod_labels, service_selector) {
                        let pod_name: String = row.get("name");
                        let status_str: String = row.get("status");
                        if let Ok(status) = serde_json::from_str::<Value>(&status_str) {
                            if let Some(pod_ip) = status["podIP"].as_str() {
                                pod_ips.push(json!({
                                    "ip": pod_ip,
                                    "targetRef": {
                                        "kind": "Pod",
                                        "namespace": service_namespace,
                                        "name": pod_name
                                    }
                                }));
                            }
                        }
                    }
                }
            }
        }
        
        // Build the endpoints subsets
        let subsets = if pod_ips.is_empty() {
            json!([])
        } else {
            // Get service ports
            let service_row = sqlx::query(
                "SELECT spec FROM services WHERE namespace = ? AND name = ?"
            )
            .bind(service_namespace)
            .bind(service_name)
            .fetch_optional(&self.pool)
            .await?;
            
            let ports = if let Some(row) = service_row {
                let spec_str: String = row.get("spec");
                if let Ok(spec) = serde_json::from_str::<Value>(&spec_str) {
                    if let Some(service_ports) = spec["ports"].as_array() {
                        service_ports.iter().map(|p| {
                            json!({
                                "port": p["targetPort"].as_i64().unwrap_or_else(|| p["port"].as_i64().unwrap_or(80)),
                                "protocol": p["protocol"].as_str().unwrap_or("TCP")
                            })
                        }).collect::<Vec<_>>()
                    } else {
                        vec![json!({"port": 80, "protocol": "TCP"})]
                    }
                } else {
                    vec![json!({"port": 80, "protocol": "TCP"})]
                }
            } else {
                vec![json!({"port": 80, "protocol": "TCP"})]
            };
            
            json!([{
                "addresses": pod_ips,
                "ports": ports
            }])
        };
        
        // Check if endpoints exist
        let existing = self.get(service_namespace, service_name).await;
        
        if existing.is_ok() {
            // Update existing endpoints
            let mut endpoints = existing.unwrap();
            endpoints["subsets"] = subsets;
            self.update(service_namespace, service_name, endpoints).await?;
        } else {
            // Create new endpoints
            let endpoints = json!({
                "metadata": {
                    "name": service_name,
                    "namespace": service_namespace
                },
                "subsets": subsets
            });
            self.create(service_namespace, endpoints).await?;
        }
        
        Ok(())
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