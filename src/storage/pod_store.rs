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
        
        // Set default status with proper conditions
        if pod["status"].is_null() {
            pod["status"] = json!({
                "phase": "Pending",
                "conditions": [
                    {
                        "type": "PodScheduled",
                        "status": "False",
                        "lastProbeTime": null,
                        "lastTransitionTime": now,
                        "reason": "Unscheduled",
                        "message": "Pod has not been scheduled yet"
                    },
                    {
                        "type": "Ready",
                        "status": "False",
                        "lastProbeTime": null,
                        "lastTransitionTime": now,
                        "reason": "ContainersNotReady",
                        "message": "Containers are not ready"
                    },
                    {
                        "type": "ContainersReady",
                        "status": "False",
                        "lastProbeTime": null,
                        "lastTransitionTime": now,
                        "reason": "ContainersNotReady",
                        "message": "Containers are not ready"
                    },
                    {
                        "type": "Initialized",
                        "status": "True",
                        "lastProbeTime": null,
                        "lastTransitionTime": now,
                        "reason": "PodInitialized",
                        "message": "Pod has been initialized"
                    }
                ],
                "containerStatuses": [],
                "hostIP": "127.0.0.1",
                "podIP": "",
                "podIPs": []
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
                let mut spec = serde_json::from_str::<Value>(&row.get::<String, _>("spec"))?;
                let mut status = serde_json::from_str::<Value>(&row.get::<String, _>("status"))?;
                
                // Add node_name to spec if scheduled
                if let Ok(node_name) = row.try_get::<Option<String>, _>("node_name") {
                    if let Some(node) = node_name {
                        spec["nodeName"] = json!(node);
                    }
                }
                
                // Add qosClass to status based on resource requests/limits
                let qos_class = Self::calculate_qos_class(&spec);
                status["qosClass"] = json!(qos_class);
                
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
                    "spec": spec,
                    "status": status
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
            let mut spec = serde_json::from_str::<Value>(&row.get::<String, _>("spec"))?;
            let mut status = serde_json::from_str::<Value>(&row.get::<String, _>("status"))?;
            
            // Add node_name to spec if scheduled
            if let Ok(node_name) = row.try_get::<Option<String>, _>("node_name") {
                if let Some(node) = node_name {
                    spec["nodeName"] = json!(node);
                }
            }
            
            // Add qosClass to status
            let qos_class = Self::calculate_qos_class(&spec);
            status["qosClass"] = json!(qos_class);
            
            // Calculate ready containers for kubectl display
            let (ready_containers, total_containers) = Self::calculate_ready_status(&spec, &status);
            
            // Calculate total restarts
            let total_restarts = Self::calculate_total_restarts(&status);
            
            // Calculate age for kubectl display
            let creation_timestamp = row.get::<String, _>("creation_timestamp");
            let age = Self::calculate_age(&creation_timestamp);
            
            // Add computed fields for kubectl display
            status["_computed"] = json!({
                "readyContainers": ready_containers,
                "totalContainers": total_containers,
                "totalRestarts": total_restarts,
                "age": age
            });
            
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
                "spec": spec,
                "status": status
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
    
    pub async fn set_status(&self, namespace: &str, name: &str, status: Value) -> Result<Value> {
        let mut pod = self.get(namespace, name).await?;
        let uid = pod["metadata"]["uid"].as_str().unwrap().to_string();
        let current_version: i64 = pod["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse()?;
        
        let new_version = current_version + 1;
        
        // Update the status
        pod["status"] = status.clone();
        pod["metadata"]["resourceVersion"] = json!(new_version.to_string());
        
        // Update in database
        sqlx::query(
            "UPDATE pods SET status = ?, resource_version = ? WHERE uid = ?"
        )
        .bind(status.to_string())
        .bind(new_version)
        .bind(&uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("Pod", &uid, name, namespace, "MODIFIED", new_version, &pod).await?;
        
        Ok(pod)
    }
    
    pub async fn get_status(&self, namespace: &str, name: &str) -> Result<Value> {
        self.get(namespace, name).await
    }
    
    pub async fn update_ephemeral_containers(&self, namespace: &str, name: &str, ephemeral_containers: Value) -> Result<Value> {
        let mut pod = self.get(namespace, name).await?;
        let uid = pod["metadata"]["uid"].as_str().unwrap().to_string();
        let current_version: i64 = pod["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse()?;
        
        let new_version = current_version + 1;
        
        // Add ephemeral containers to spec
        pod["spec"]["ephemeralContainers"] = ephemeral_containers;
        pod["metadata"]["resourceVersion"] = json!(new_version.to_string());
        
        let spec = pod["spec"].to_string();
        
        // Update in database
        sqlx::query(
            "UPDATE pods SET spec = ?, resource_version = ? WHERE uid = ?"
        )
        .bind(spec)
        .bind(new_version)
        .bind(&uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("Pod", &uid, name, namespace, "MODIFIED", new_version, &pod).await?;
        
        Ok(pod)
    }
    
    pub async fn bind_to_node(&self, namespace: &str, name: &str, node_name: &str) -> Result<()> {
        let mut pod = self.get(namespace, name).await?;
        let uid = pod["metadata"]["uid"].as_str().unwrap().to_string();
        let current_version: i64 = pod["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse()?;
        
        let new_version = current_version + 1;
        
        // Set nodeName in spec
        pod["spec"]["nodeName"] = json!(node_name);
        pod["metadata"]["resourceVersion"] = json!(new_version.to_string());
        
        // Update status to scheduled
        pod["status"]["phase"] = json!("Pending");
        pod["status"]["conditions"] = json!([
            {
                "type": "PodScheduled",
                "status": "True",
                "lastTransitionTime": Utc::now().to_rfc3339(),
                "reason": "Scheduled",
                "message": format!("Successfully assigned to {}", node_name)
            }
        ]);
        
        let spec = pod["spec"].to_string();
        let status = pod["status"].to_string();
        
        // Update in database
        sqlx::query(
            "UPDATE pods SET spec = ?, status = ?, node_name = ?, resource_version = ? WHERE uid = ?"
        )
        .bind(spec)
        .bind(status)
        .bind(node_name)
        .bind(new_version)
        .bind(&uid)
        .execute(&self.pool)
        .await?;
        
        // Record event
        self.record_event("Pod", &uid, name, namespace, "SCHEDULED", new_version, &pod).await?;
        
        Ok(())
    }
    
    fn calculate_qos_class(spec: &Value) -> &'static str {
        // Kubernetes QoS classes:
        // - Guaranteed: Every container has memory/cpu limits and requests, and they are equal
        // - Burstable: At least one container has memory or cpu request
        // - BestEffort: No containers have memory or cpu requests or limits
        
        if let Some(containers) = spec["containers"].as_array() {
            let mut has_requests = false;
            let mut all_guaranteed = true;
            
            for container in containers {
                let resources = &container["resources"];
                let requests = &resources["requests"];
                let limits = &resources["limits"];
                
                let has_cpu_request = !requests["cpu"].is_null();
                let has_mem_request = !requests["memory"].is_null();
                let has_cpu_limit = !limits["cpu"].is_null();
                let has_mem_limit = !limits["memory"].is_null();
                
                if has_cpu_request || has_mem_request {
                    has_requests = true;
                }
                
                // For Guaranteed, needs both cpu and memory, and limits == requests
                if !(has_cpu_request && has_mem_request && has_cpu_limit && has_mem_limit
                    && requests["cpu"] == limits["cpu"] 
                    && requests["memory"] == limits["memory"]) {
                    all_guaranteed = false;
                }
            }
            
            if all_guaranteed && has_requests {
                "Guaranteed"
            } else if has_requests {
                "Burstable"
            } else {
                "BestEffort"
            }
        } else {
            "BestEffort"
        }
    }
    
    fn calculate_ready_status(spec: &Value, status: &Value) -> (i32, i32) {
        // Calculate ready containers count
        let total_containers = spec["containers"].as_array()
            .map(|c| c.len() as i32)
            .unwrap_or(0);
        
        let ready_containers = if let Some(container_statuses) = status["containerStatuses"].as_array() {
            container_statuses.iter()
                .filter(|cs| cs["ready"].as_bool().unwrap_or(false))
                .count() as i32
        } else {
            0
        };
        
        (ready_containers, total_containers)
    }
    
    fn calculate_total_restarts(status: &Value) -> i32 {
        // Calculate total restart count across all containers
        if let Some(container_statuses) = status["containerStatuses"].as_array() {
            container_statuses.iter()
                .map(|cs| cs["restartCount"].as_i64().unwrap_or(0) as i32)
                .sum()
        } else {
            0
        }
    }
    
    fn calculate_age(creation_timestamp: &str) -> String {
        // Calculate age as a human-readable string
        use chrono::{DateTime, Utc};
        
        if let Ok(created) = DateTime::parse_from_rfc3339(creation_timestamp) {
            let now = Utc::now();
            let duration = now.signed_duration_since(created);
            
            if duration.num_days() > 0 {
                format!("{}d", duration.num_days())
            } else if duration.num_hours() > 0 {
                format!("{}h", duration.num_hours())
            } else if duration.num_minutes() > 0 {
                format!("{}m", duration.num_minutes())
            } else {
                format!("{}s", duration.num_seconds())
            }
        } else {
            "unknown".to_string()
        }
    }
}