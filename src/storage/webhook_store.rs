use anyhow::Result;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

// ValidatingWebhookConfiguration storage
pub struct ValidatingWebhookStore {
    pool: SqlitePool,
}

impl ValidatingWebhookStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, mut vwc: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = vwc["metadata"]["name"].as_str().unwrap().to_string();
        let webhooks = vwc["webhooks"].clone();
        let labels = vwc["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = vwc["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "INSERT INTO validatingwebhookconfigurations (uid, name, webhooks, labels, annotations, resource_version, generation)
             VALUES (?, ?, ?, ?, ?, 1, 1)"
        )
        .bind(&uid)
        .bind(&name)
        .bind(serde_json::to_string(&webhooks)?)
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .execute(&self.pool)
        .await?;

        vwc["metadata"]["uid"] = json!(uid);
        vwc["metadata"]["resourceVersion"] = json!("1");
        vwc["metadata"]["generation"] = json!(1);
        vwc["metadata"]["creationTimestamp"] = json!(chrono::Utc::now().to_rfc3339());

        self.record_event(&uid, "ValidatingWebhookConfiguration", &name, "Created", "ValidatingWebhookConfiguration created").await?;
        Ok(vwc)
    }

    pub async fn get(&self, name: &str) -> Result<Option<Value>> {
        let row = sqlx::query(
            "SELECT uid, name, webhooks, labels, annotations, resource_version, generation, creation_timestamp
             FROM validatingwebhookconfigurations WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let webhooks: String = row.get("webhooks");
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let vwc = json!({
                "apiVersion": "admissionregistration.k8s.io/v1",
                "kind": "ValidatingWebhookConfiguration",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "resourceVersion": resource_version.to_string(),
                    "generation": generation,
                    "creationTimestamp": creation_timestamp,
                    "labels": serde_json::from_str(&labels)?,
                    "annotations": serde_json::from_str(&annotations)?
                },
                "webhooks": serde_json::from_str(&webhooks)?
            });

            Ok(Some(vwc))
        } else {
            Ok(None)
        }
    }

    pub async fn update(&self, name: &str, mut vwc: Value) -> Result<Value> {
        let current = self.get(name).await?
            .ok_or_else(|| anyhow::anyhow!("ValidatingWebhookConfiguration not found"))?;

        let uid = current["metadata"]["uid"].as_str().unwrap();
        let resource_version: i64 = current["metadata"]["resourceVersion"].as_str().unwrap().parse()?;
        let new_resource_version = resource_version + 1;
        let generation: i64 = current["metadata"]["generation"].as_i64().unwrap();
        let new_generation = if vwc["webhooks"] != current["webhooks"] { generation + 1 } else { generation };

        let webhooks = vwc["webhooks"].clone();
        let labels = vwc["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = vwc["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "UPDATE validatingwebhookconfigurations 
             SET webhooks = ?, labels = ?, annotations = ?, resource_version = ?, generation = ?
             WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(serde_json::to_string(&webhooks)?)
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .bind(new_resource_version)
        .bind(new_generation)
        .bind(name)
        .execute(&self.pool)
        .await?;

        vwc["metadata"]["uid"] = json!(uid);
        vwc["metadata"]["resourceVersion"] = json!(new_resource_version.to_string());
        vwc["metadata"]["generation"] = json!(new_generation);
        vwc["metadata"]["creationTimestamp"] = current["metadata"]["creationTimestamp"].clone();

        self.record_event(uid, "ValidatingWebhookConfiguration", name, "Updated", "ValidatingWebhookConfiguration updated").await?;
        Ok(vwc)
    }

    pub async fn delete(&self, name: &str) -> Result<Value> {
        let vwc = self.get(name).await?
            .ok_or_else(|| anyhow::anyhow!("ValidatingWebhookConfiguration not found"))?;

        let uid = vwc["metadata"]["uid"].as_str().unwrap();

        sqlx::query(
            "UPDATE validatingwebhookconfigurations SET deletion_timestamp = CURRENT_TIMESTAMP 
             WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .execute(&self.pool)
        .await?;

        self.record_event(uid, "ValidatingWebhookConfiguration", name, "Deleted", "ValidatingWebhookConfiguration deleted").await?;
        Ok(vwc)
    }

    pub async fn list(&self) -> Result<Value> {
        let rows = sqlx::query(
            "SELECT uid, name, webhooks, labels, annotations, resource_version, generation, creation_timestamp
             FROM validatingwebhookconfigurations WHERE deletion_timestamp IS NULL"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut items = Vec::new();
        for row in rows {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let webhooks: String = row.get("webhooks");
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let vwc = json!({
                "apiVersion": "admissionregistration.k8s.io/v1",
                "kind": "ValidatingWebhookConfiguration",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "resourceVersion": resource_version.to_string(),
                    "generation": generation,
                    "creationTimestamp": creation_timestamp,
                    "labels": serde_json::from_str(&labels)?,
                    "annotations": serde_json::from_str(&annotations)?
                },
                "webhooks": serde_json::from_str(&webhooks)?
            });

            items.push(vwc);
        }

        Ok(json!({
            "apiVersion": "admissionregistration.k8s.io/v1",
            "kind": "ValidatingWebhookConfigurationList",
            "items": items
        }))
    }

    async fn record_event(&self, uid: &str, resource_type: &str, name: &str, reason: &str, message: &str) -> Result<()> {
        let event_uid = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO events (uid, namespace, involved_object_uid, involved_object_kind, 
             involved_object_name, reason, message, event_time, first_timestamp, last_timestamp, count, type)
             VALUES (?, '', ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1, 'Normal')"
        )
        .bind(&event_uid)
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

// MutatingWebhookConfiguration storage
pub struct MutatingWebhookStore {
    pool: SqlitePool,
}

impl MutatingWebhookStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, mut mwc: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = mwc["metadata"]["name"].as_str().unwrap().to_string();
        let webhooks = mwc["webhooks"].clone();
        let labels = mwc["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = mwc["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "INSERT INTO mutatingwebhookconfigurations (uid, name, webhooks, labels, annotations, resource_version, generation)
             VALUES (?, ?, ?, ?, ?, 1, 1)"
        )
        .bind(&uid)
        .bind(&name)
        .bind(serde_json::to_string(&webhooks)?)
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .execute(&self.pool)
        .await?;

        mwc["metadata"]["uid"] = json!(uid);
        mwc["metadata"]["resourceVersion"] = json!("1");
        mwc["metadata"]["generation"] = json!(1);
        mwc["metadata"]["creationTimestamp"] = json!(chrono::Utc::now().to_rfc3339());

        self.record_event(&uid, "MutatingWebhookConfiguration", &name, "Created", "MutatingWebhookConfiguration created").await?;
        Ok(mwc)
    }

    pub async fn get(&self, name: &str) -> Result<Option<Value>> {
        let row = sqlx::query(
            "SELECT uid, name, webhooks, labels, annotations, resource_version, generation, creation_timestamp
             FROM mutatingwebhookconfigurations WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let webhooks: String = row.get("webhooks");
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let mwc = json!({
                "apiVersion": "admissionregistration.k8s.io/v1",
                "kind": "MutatingWebhookConfiguration",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "resourceVersion": resource_version.to_string(),
                    "generation": generation,
                    "creationTimestamp": creation_timestamp,
                    "labels": serde_json::from_str(&labels)?,
                    "annotations": serde_json::from_str(&annotations)?
                },
                "webhooks": serde_json::from_str(&webhooks)?
            });

            Ok(Some(mwc))
        } else {
            Ok(None)
        }
    }

    pub async fn update(&self, name: &str, mut mwc: Value) -> Result<Value> {
        let current = self.get(name).await?
            .ok_or_else(|| anyhow::anyhow!("MutatingWebhookConfiguration not found"))?;

        let uid = current["metadata"]["uid"].as_str().unwrap();
        let resource_version: i64 = current["metadata"]["resourceVersion"].as_str().unwrap().parse()?;
        let new_resource_version = resource_version + 1;
        let generation: i64 = current["metadata"]["generation"].as_i64().unwrap();
        let new_generation = if mwc["webhooks"] != current["webhooks"] { generation + 1 } else { generation };

        let webhooks = mwc["webhooks"].clone();
        let labels = mwc["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = mwc["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "UPDATE mutatingwebhookconfigurations 
             SET webhooks = ?, labels = ?, annotations = ?, resource_version = ?, generation = ?
             WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(serde_json::to_string(&webhooks)?)
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .bind(new_resource_version)
        .bind(new_generation)
        .bind(name)
        .execute(&self.pool)
        .await?;

        mwc["metadata"]["uid"] = json!(uid);
        mwc["metadata"]["resourceVersion"] = json!(new_resource_version.to_string());
        mwc["metadata"]["generation"] = json!(new_generation);
        mwc["metadata"]["creationTimestamp"] = current["metadata"]["creationTimestamp"].clone();

        self.record_event(uid, "MutatingWebhookConfiguration", name, "Updated", "MutatingWebhookConfiguration updated").await?;
        Ok(mwc)
    }

    pub async fn delete(&self, name: &str) -> Result<Value> {
        let mwc = self.get(name).await?
            .ok_or_else(|| anyhow::anyhow!("MutatingWebhookConfiguration not found"))?;

        let uid = mwc["metadata"]["uid"].as_str().unwrap();

        sqlx::query(
            "UPDATE mutatingwebhookconfigurations SET deletion_timestamp = CURRENT_TIMESTAMP 
             WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .execute(&self.pool)
        .await?;

        self.record_event(uid, "MutatingWebhookConfiguration", name, "Deleted", "MutatingWebhookConfiguration deleted").await?;
        Ok(mwc)
    }

    pub async fn list(&self) -> Result<Value> {
        let rows = sqlx::query(
            "SELECT uid, name, webhooks, labels, annotations, resource_version, generation, creation_timestamp
             FROM mutatingwebhookconfigurations WHERE deletion_timestamp IS NULL"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut items = Vec::new();
        for row in rows {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let webhooks: String = row.get("webhooks");
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let mwc = json!({
                "apiVersion": "admissionregistration.k8s.io/v1",
                "kind": "MutatingWebhookConfiguration",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "resourceVersion": resource_version.to_string(),
                    "generation": generation,
                    "creationTimestamp": creation_timestamp,
                    "labels": serde_json::from_str(&labels)?,
                    "annotations": serde_json::from_str(&annotations)?
                },
                "webhooks": serde_json::from_str(&webhooks)?
            });

            items.push(mwc);
        }

        Ok(json!({
            "apiVersion": "admissionregistration.k8s.io/v1",
            "kind": "MutatingWebhookConfigurationList",
            "items": items
        }))
    }

    async fn record_event(&self, uid: &str, resource_type: &str, name: &str, reason: &str, message: &str) -> Result<()> {
        let event_uid = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO events (uid, namespace, involved_object_uid, involved_object_kind, 
             involved_object_name, reason, message, event_time, first_timestamp, last_timestamp, count, type)
             VALUES (?, '', ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1, 'Normal')"
        )
        .bind(&event_uid)
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