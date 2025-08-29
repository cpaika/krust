use anyhow::Result;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct PdbStore {
    pool: SqlitePool,
}

impl PdbStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut pdb: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = pdb["metadata"]["name"].as_str().unwrap().to_string();
        let spec = pdb["spec"].clone();
        let min_available = spec.get("minAvailable").cloned();
        let max_unavailable = spec.get("maxUnavailable").cloned();
        let selector = spec["selector"].clone();
        let unhealthy_pod_eviction_policy = spec.get("unhealthyPodEvictionPolicy")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let labels = pdb["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = pdb["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "INSERT INTO poddisruptionbudgets (uid, name, namespace, spec, min_available, max_unavailable,
             selector, unhealthy_pod_eviction_policy, labels, annotations, resource_version, generation)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 1)"
        )
        .bind(&uid)
        .bind(&name)
        .bind(namespace)
        .bind(serde_json::to_string(&spec)?)
        .bind(min_available.as_ref().map(|v| serde_json::to_string(v).ok()).flatten())
        .bind(max_unavailable.as_ref().map(|v| serde_json::to_string(v).ok()).flatten())
        .bind(serde_json::to_string(&selector)?)
        .bind(unhealthy_pod_eviction_policy)
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .execute(&self.pool)
        .await?;

        pdb["metadata"]["uid"] = json!(uid);
        pdb["metadata"]["resourceVersion"] = json!("1");
        pdb["metadata"]["generation"] = json!(1);
        pdb["metadata"]["creationTimestamp"] = json!(chrono::Utc::now().to_rfc3339());
        
        // Initialize status
        if pdb["status"].is_null() {
            pdb["status"] = json!({
                "currentHealthy": 0,
                "desiredHealthy": 0,
                "disruptionsAllowed": 0,
                "expectedPods": 0,
                "observedGeneration": 1
            });
        }

        self.record_event(
            &uid,
            "PodDisruptionBudget",
            namespace,
            &name,
            "Created",
            "PodDisruptionBudget created"
        ).await?;

        Ok(pdb)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Option<Value>> {
        let row = sqlx::query(
            "SELECT uid, name, namespace, spec, status, labels, annotations,
             resource_version, generation, creation_timestamp, deletion_timestamp
             FROM poddisruptionbudgets
             WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(namespace)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let namespace: String = row.get("namespace");
            let spec: String = row.get("spec");
            let status: Option<String> = row.get("status");
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let mut pdb = json!({
                "apiVersion": "policy/v1",
                "kind": "PodDisruptionBudget",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "namespace": namespace,
                    "resourceVersion": resource_version.to_string(),
                    "generation": generation,
                    "creationTimestamp": creation_timestamp,
                    "labels": serde_json::from_str(&labels)?,
                    "annotations": serde_json::from_str(&annotations)?
                },
                "spec": serde_json::from_str(&spec)?
            });

            if let Some(status_str) = status {
                pdb["status"] = serde_json::from_str(&status_str)?;
            } else {
                pdb["status"] = json!({
                    "currentHealthy": 0,
                    "desiredHealthy": 0,
                    "disruptionsAllowed": 0,
                    "expectedPods": 0,
                    "observedGeneration": generation
                });
            }

            Ok(Some(pdb))
        } else {
            Ok(None)
        }
    }

    pub async fn update(&self, namespace: &str, name: &str, mut pdb: Value) -> Result<Value> {
        let current = self.get(namespace, name).await?
            .ok_or_else(|| anyhow::anyhow!("PodDisruptionBudget not found"))?;

        let uid = current["metadata"]["uid"].as_str().unwrap();
        let resource_version: i64 = current["metadata"]["resourceVersion"].as_str().unwrap().parse()?;
        let new_resource_version = resource_version + 1;
        let generation: i64 = current["metadata"]["generation"].as_i64().unwrap();
        let new_generation = if pdb["spec"] != current["spec"] { generation + 1 } else { generation };

        let spec = pdb["spec"].clone();
        let min_available = spec.get("minAvailable").cloned();
        let max_unavailable = spec.get("maxUnavailable").cloned();
        let selector = spec["selector"].clone();
        let unhealthy_pod_eviction_policy = spec.get("unhealthyPodEvictionPolicy")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let labels = pdb["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = pdb["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "UPDATE poddisruptionbudgets 
             SET spec = ?, min_available = ?, max_unavailable = ?, selector = ?,
                 unhealthy_pod_eviction_policy = ?, labels = ?, annotations = ?,
                 resource_version = ?, generation = ?
             WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(serde_json::to_string(&spec)?)
        .bind(min_available.as_ref().map(|v| serde_json::to_string(v).ok()).flatten())
        .bind(max_unavailable.as_ref().map(|v| serde_json::to_string(v).ok()).flatten())
        .bind(serde_json::to_string(&selector)?)
        .bind(unhealthy_pod_eviction_policy)
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .bind(new_resource_version)
        .bind(new_generation)
        .bind(namespace)
        .bind(name)
        .execute(&self.pool)
        .await?;

        pdb["metadata"]["uid"] = json!(uid);
        pdb["metadata"]["resourceVersion"] = json!(new_resource_version.to_string());
        pdb["metadata"]["generation"] = json!(new_generation);
        pdb["metadata"]["creationTimestamp"] = current["metadata"]["creationTimestamp"].clone();

        self.record_event(
            uid,
            "PodDisruptionBudget",
            namespace,
            name,
            "Updated",
            "PodDisruptionBudget updated"
        ).await?;

        Ok(pdb)
    }

    pub async fn update_status(&self, namespace: &str, name: &str, status: Value) -> Result<Value> {
        let mut current = self.get(namespace, name).await?
            .ok_or_else(|| anyhow::anyhow!("PodDisruptionBudget not found"))?;

        let uid = current["metadata"]["uid"].as_str().unwrap().to_string();
        let resource_version: i64 = current["metadata"]["resourceVersion"].as_str().unwrap().parse()?;
        let new_resource_version = resource_version + 1;

        // Extract status fields for indexing
        let current_healthy = status.get("currentHealthy").and_then(|v| v.as_i64());
        let desired_healthy = status.get("desiredHealthy").and_then(|v| v.as_i64());
        let disruptions_allowed = status.get("disruptionsAllowed").and_then(|v| v.as_i64());
        let expected_pods = status.get("expectedPods").and_then(|v| v.as_i64());
        let observed_generation = status.get("observedGeneration").and_then(|v| v.as_i64());

        sqlx::query(
            "UPDATE poddisruptionbudgets 
             SET status = ?, current_healthy = ?, desired_healthy = ?, disruptions_allowed = ?,
                 expected_pods = ?, observed_generation = ?, resource_version = ?
             WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(serde_json::to_string(&status)?)
        .bind(current_healthy)
        .bind(desired_healthy)
        .bind(disruptions_allowed)
        .bind(expected_pods)
        .bind(observed_generation)
        .bind(new_resource_version)
        .bind(namespace)
        .bind(name)
        .execute(&self.pool)
        .await?;

        current["metadata"]["resourceVersion"] = json!(new_resource_version.to_string());
        current["status"] = status;

        self.record_event(
            &uid,
            "PodDisruptionBudget",
            namespace,
            name,
            "StatusUpdated",
            "PodDisruptionBudget status updated"
        ).await?;

        Ok(current)
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        let pdb = self.get(namespace, name).await?
            .ok_or_else(|| anyhow::anyhow!("PodDisruptionBudget not found"))?;

        let uid = pdb["metadata"]["uid"].as_str().unwrap();

        sqlx::query(
            "UPDATE poddisruptionbudgets 
             SET deletion_timestamp = CURRENT_TIMESTAMP 
             WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(namespace)
        .bind(name)
        .execute(&self.pool)
        .await?;

        self.record_event(
            uid,
            "PodDisruptionBudget",
            namespace,
            name,
            "Deleted",
            "PodDisruptionBudget deleted"
        ).await?;

        Ok(pdb)
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if let Some(ns) = namespace {
            sqlx::query(
                "SELECT uid, name, namespace, spec, status, labels, annotations,
                 resource_version, generation, creation_timestamp
                 FROM poddisruptionbudgets
                 WHERE namespace = ? AND deletion_timestamp IS NULL"
            )
            .bind(ns)
        } else {
            sqlx::query(
                "SELECT uid, name, namespace, spec, status, labels, annotations,
                 resource_version, generation, creation_timestamp
                 FROM poddisruptionbudgets
                 WHERE deletion_timestamp IS NULL"
            )
        };

        let rows = query.fetch_all(&self.pool).await?;
        let mut items = Vec::new();

        for row in rows {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let namespace: String = row.get("namespace");
            let spec: String = row.get("spec");
            let status: Option<String> = row.get("status");
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let mut pdb = json!({
                "apiVersion": "policy/v1",
                "kind": "PodDisruptionBudget",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "namespace": namespace,
                    "resourceVersion": resource_version.to_string(),
                    "generation": generation,
                    "creationTimestamp": creation_timestamp,
                    "labels": serde_json::from_str(&labels)?,
                    "annotations": serde_json::from_str(&annotations)?
                },
                "spec": serde_json::from_str(&spec)?
            });

            if let Some(status_str) = status {
                pdb["status"] = serde_json::from_str(&status_str)?;
            } else {
                pdb["status"] = json!({
                    "currentHealthy": 0,
                    "desiredHealthy": 0,
                    "disruptionsAllowed": 0,
                    "expectedPods": 0,
                    "observedGeneration": generation
                });
            }

            items.push(pdb);
        }

        Ok(json!({
            "apiVersion": "policy/v1",
            "kind": "PodDisruptionBudgetList",
            "items": items
        }))
    }

    async fn record_event(&self, uid: &str, resource_type: &str, namespace: &str, name: &str, reason: &str, message: &str) -> Result<()> {
        let event_uid = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO events (uid, namespace, involved_object_uid, involved_object_kind, involved_object_name, reason, message, event_time, first_timestamp, last_timestamp, count, type)
             VALUES (?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1, 'Normal')"
        )
        .bind(&event_uid)
        .bind(namespace)
        .bind(uid)
        .bind(resource_type)
        .bind(name)
        .bind(reason)
        .bind(message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}