use anyhow::Result;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct ResourceQuotaStore {
    pool: SqlitePool,
}

impl ResourceQuotaStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut quota: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = quota["metadata"]["name"].as_str().unwrap().to_string();
        let spec = quota["spec"].clone();
        let hard = spec["hard"].clone();
        let scope_selector = spec.get("scopeSelector").cloned();
        let labels = quota["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = quota["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "INSERT INTO resourcequotas (uid, name, namespace, spec, hard, scope_selector, labels, annotations, resource_version, generation)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, 1)"
        )
        .bind(&uid)
        .bind(&name)
        .bind(namespace)
        .bind(serde_json::to_string(&spec)?)
        .bind(serde_json::to_string(&hard)?)
        .bind(scope_selector.as_ref().map(|s| serde_json::to_string(s).ok()).flatten())
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .execute(&self.pool)
        .await?;

        quota["metadata"]["uid"] = json!(uid);
        quota["metadata"]["resourceVersion"] = json!("1");
        quota["metadata"]["generation"] = json!(1);
        quota["metadata"]["creationTimestamp"] = json!(chrono::Utc::now().to_rfc3339());
        
        if quota["status"].is_null() {
            quota["status"] = json!({
                "hard": hard,
                "used": {}
            });
        }

        self.record_event(
            &uid,
            "ResourceQuota",
            namespace,
            &name,
            "Created",
            "ResourceQuota created"
        ).await?;

        Ok(quota)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Option<Value>> {
        let row = sqlx::query(
            "SELECT uid, name, namespace, spec, status, hard, used, scope_selector, labels, annotations, 
             resource_version, generation, creation_timestamp, deletion_timestamp
             FROM resourcequotas
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
            let hard: String = row.get("hard");
            let used: Option<String> = row.get("used");
            let scope_selector: Option<String> = row.get("scope_selector");
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let mut quota = json!({
                "apiVersion": "v1",
                "kind": "ResourceQuota",
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
                quota["status"] = serde_json::from_str(&status_str)?;
            } else {
                quota["status"] = json!({
                    "hard": serde_json::from_str(&hard)?,
                    "used": used.as_ref().map(|u| serde_json::from_str(u).ok()).flatten().unwrap_or(json!({}))
                });
            }

            Ok(Some(quota))
        } else {
            Ok(None)
        }
    }

    pub async fn update(&self, namespace: &str, name: &str, mut quota: Value) -> Result<Value> {
        let current = self.get(namespace, name).await?
            .ok_or_else(|| anyhow::anyhow!("ResourceQuota not found"))?;

        let uid = current["metadata"]["uid"].as_str().unwrap();
        let resource_version: i64 = current["metadata"]["resourceVersion"].as_str().unwrap().parse()?;
        let new_resource_version = resource_version + 1;
        let generation: i64 = current["metadata"]["generation"].as_i64().unwrap();
        let new_generation = if quota["spec"] != current["spec"] { generation + 1 } else { generation };

        let spec = quota["spec"].clone();
        let hard = spec["hard"].clone();
        let scope_selector = spec.get("scopeSelector").cloned();
        let labels = quota["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = quota["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "UPDATE resourcequotas 
             SET spec = ?, hard = ?, scope_selector = ?, labels = ?, annotations = ?, 
                 resource_version = ?, generation = ?
             WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(serde_json::to_string(&spec)?)
        .bind(serde_json::to_string(&hard)?)
        .bind(scope_selector.as_ref().map(|s| serde_json::to_string(s).ok()).flatten())
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .bind(new_resource_version)
        .bind(new_generation)
        .bind(namespace)
        .bind(name)
        .execute(&self.pool)
        .await?;

        quota["metadata"]["uid"] = json!(uid);
        quota["metadata"]["resourceVersion"] = json!(new_resource_version.to_string());
        quota["metadata"]["generation"] = json!(new_generation);
        quota["metadata"]["creationTimestamp"] = current["metadata"]["creationTimestamp"].clone();

        self.record_event(
            uid,
            "ResourceQuota",
            namespace,
            name,
            "Updated",
            "ResourceQuota updated"
        ).await?;

        Ok(quota)
    }

    pub async fn update_status(&self, namespace: &str, name: &str, status: Value) -> Result<Value> {
        let mut current = self.get(namespace, name).await?
            .ok_or_else(|| anyhow::anyhow!("ResourceQuota not found"))?;

        let uid = current["metadata"]["uid"].as_str().unwrap().to_string();
        let resource_version: i64 = current["metadata"]["resourceVersion"].as_str().unwrap().parse()?;
        let new_resource_version = resource_version + 1;

        let used = status.get("used").cloned().unwrap_or(json!({}));

        sqlx::query(
            "UPDATE resourcequotas 
             SET status = ?, used = ?, resource_version = ?
             WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(serde_json::to_string(&status)?)
        .bind(serde_json::to_string(&used)?)
        .bind(new_resource_version)
        .bind(namespace)
        .bind(name)
        .execute(&self.pool)
        .await?;

        current["metadata"]["resourceVersion"] = json!(new_resource_version.to_string());
        current["status"] = status;

        self.record_event(
            &uid,
            "ResourceQuota",
            namespace,
            name,
            "StatusUpdated",
            "ResourceQuota status updated"
        ).await?;

        Ok(current)
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        let quota = self.get(namespace, name).await?
            .ok_or_else(|| anyhow::anyhow!("ResourceQuota not found"))?;

        let uid = quota["metadata"]["uid"].as_str().unwrap();

        sqlx::query(
            "UPDATE resourcequotas 
             SET deletion_timestamp = CURRENT_TIMESTAMP 
             WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(namespace)
        .bind(name)
        .execute(&self.pool)
        .await?;

        self.record_event(
            uid,
            "ResourceQuota",
            namespace,
            name,
            "Deleted",
            "ResourceQuota deleted"
        ).await?;

        Ok(quota)
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if let Some(ns) = namespace {
            sqlx::query(
                "SELECT uid, name, namespace, spec, status, hard, used, scope_selector, labels, annotations,
                 resource_version, generation, creation_timestamp
                 FROM resourcequotas
                 WHERE namespace = ? AND deletion_timestamp IS NULL"
            )
            .bind(ns)
        } else {
            sqlx::query(
                "SELECT uid, name, namespace, spec, status, hard, used, scope_selector, labels, annotations,
                 resource_version, generation, creation_timestamp
                 FROM resourcequotas
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
            let hard: String = row.get("hard");
            let used: Option<String> = row.get("used");
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let mut quota = json!({
                "apiVersion": "v1",
                "kind": "ResourceQuota",
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
                quota["status"] = serde_json::from_str(&status_str)?;
            } else {
                quota["status"] = json!({
                    "hard": serde_json::from_str(&hard)?,
                    "used": used.as_ref().map(|u| serde_json::from_str(u).ok()).flatten().unwrap_or(json!({}))
                });
            }

            items.push(quota);
        }

        Ok(json!({
            "apiVersion": "v1",
            "kind": "ResourceQuotaList",
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