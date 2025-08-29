use anyhow::Result;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct LimitRangeStore {
    pool: SqlitePool,
}

impl LimitRangeStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut limitrange: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = limitrange["metadata"]["name"].as_str().unwrap().to_string();
        let spec = limitrange["spec"].clone();
        let limits = spec["limits"].clone();
        let labels = limitrange["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = limitrange["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "INSERT INTO limitranges (uid, name, namespace, spec, limits, labels, annotations, resource_version, generation)
             VALUES (?, ?, ?, ?, ?, ?, ?, 1, 1)"
        )
        .bind(&uid)
        .bind(&name)
        .bind(namespace)
        .bind(serde_json::to_string(&spec)?)
        .bind(serde_json::to_string(&limits)?)
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .execute(&self.pool)
        .await?;

        limitrange["metadata"]["uid"] = json!(uid);
        limitrange["metadata"]["resourceVersion"] = json!("1");
        limitrange["metadata"]["generation"] = json!(1);
        limitrange["metadata"]["creationTimestamp"] = json!(chrono::Utc::now().to_rfc3339());

        self.record_event(
            &uid,
            "LimitRange",
            namespace,
            &name,
            "Created",
            "LimitRange created"
        ).await?;

        Ok(limitrange)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Option<Value>> {
        let row = sqlx::query(
            "SELECT uid, name, namespace, spec, limits, labels, annotations, 
             resource_version, generation, creation_timestamp, deletion_timestamp
             FROM limitranges
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
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let limitrange = json!({
                "apiVersion": "v1",
                "kind": "LimitRange",
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

            Ok(Some(limitrange))
        } else {
            Ok(None)
        }
    }

    pub async fn update(&self, namespace: &str, name: &str, mut limitrange: Value) -> Result<Value> {
        let current = self.get(namespace, name).await?
            .ok_or_else(|| anyhow::anyhow!("LimitRange not found"))?;

        let uid = current["metadata"]["uid"].as_str().unwrap();
        let resource_version: i64 = current["metadata"]["resourceVersion"].as_str().unwrap().parse()?;
        let new_resource_version = resource_version + 1;
        let generation: i64 = current["metadata"]["generation"].as_i64().unwrap();
        let new_generation = if limitrange["spec"] != current["spec"] { generation + 1 } else { generation };

        let spec = limitrange["spec"].clone();
        let limits = spec["limits"].clone();
        let labels = limitrange["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = limitrange["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "UPDATE limitranges 
             SET spec = ?, limits = ?, labels = ?, annotations = ?, 
                 resource_version = ?, generation = ?
             WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(serde_json::to_string(&spec)?)
        .bind(serde_json::to_string(&limits)?)
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .bind(new_resource_version)
        .bind(new_generation)
        .bind(namespace)
        .bind(name)
        .execute(&self.pool)
        .await?;

        limitrange["metadata"]["uid"] = json!(uid);
        limitrange["metadata"]["resourceVersion"] = json!(new_resource_version.to_string());
        limitrange["metadata"]["generation"] = json!(new_generation);
        limitrange["metadata"]["creationTimestamp"] = current["metadata"]["creationTimestamp"].clone();

        self.record_event(
            uid,
            "LimitRange",
            namespace,
            name,
            "Updated",
            "LimitRange updated"
        ).await?;

        Ok(limitrange)
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        let limitrange = self.get(namespace, name).await?
            .ok_or_else(|| anyhow::anyhow!("LimitRange not found"))?;

        let uid = limitrange["metadata"]["uid"].as_str().unwrap();

        sqlx::query(
            "UPDATE limitranges 
             SET deletion_timestamp = CURRENT_TIMESTAMP 
             WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(namespace)
        .bind(name)
        .execute(&self.pool)
        .await?;

        self.record_event(
            uid,
            "LimitRange",
            namespace,
            name,
            "Deleted",
            "LimitRange deleted"
        ).await?;

        Ok(limitrange)
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if let Some(ns) = namespace {
            sqlx::query(
                "SELECT uid, name, namespace, spec, limits, labels, annotations,
                 resource_version, generation, creation_timestamp
                 FROM limitranges
                 WHERE namespace = ? AND deletion_timestamp IS NULL"
            )
            .bind(ns)
        } else {
            sqlx::query(
                "SELECT uid, name, namespace, spec, limits, labels, annotations,
                 resource_version, generation, creation_timestamp
                 FROM limitranges
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
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let limitrange = json!({
                "apiVersion": "v1",
                "kind": "LimitRange",
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

            items.push(limitrange);
        }

        Ok(json!({
            "apiVersion": "v1",
            "kind": "LimitRangeList",
            "items": items
        }))
    }

    pub async fn get_for_namespace(&self, namespace: &str) -> Result<Vec<Value>> {
        let rows = sqlx::query(
            "SELECT spec FROM limitranges
             WHERE namespace = ? AND deletion_timestamp IS NULL"
        )
        .bind(namespace)
        .fetch_all(&self.pool)
        .await?;

        let mut limitranges = Vec::new();
        for row in rows {
            let spec: String = row.get("spec");
            limitranges.push(serde_json::from_str(&spec)?);
        }

        Ok(limitranges)
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