#![allow(unused_imports)]
use anyhow::Result;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct ServiceAccountStore {
    pool: SqlitePool,
}

impl ServiceAccountStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut sa: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = sa["metadata"]["name"].as_str().unwrap().to_string();
        let secrets = sa.get("secrets").cloned().unwrap_or(json!([]));
        let image_pull_secrets = sa.get("imagePullSecrets").cloned().unwrap_or(json!([]));
        let automount = sa.get("automountServiceAccountToken").and_then(|v| v.as_bool()).unwrap_or(true);
        let labels = sa["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = sa["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "INSERT INTO serviceaccounts (uid, name, namespace, secrets, image_pull_secrets, 
             automount_service_account_token, labels, annotations, resource_version, generation)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, 1)"
        )
        .bind(&uid)
        .bind(&name)
        .bind(namespace)
        .bind(serde_json::to_string(&secrets)?)
        .bind(serde_json::to_string(&image_pull_secrets)?)
        .bind(automount)
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .execute(&self.pool)
        .await?;

        sa["metadata"]["uid"] = json!(uid);
        sa["metadata"]["resourceVersion"] = json!("1");
        sa["metadata"]["generation"] = json!(1);
        sa["metadata"]["creationTimestamp"] = json!(chrono::Utc::now().to_rfc3339());

        self.record_event(
            &uid,
            "ServiceAccount",
            namespace,
            &name,
            "Created",
            "ServiceAccount created"
        ).await?;

        Ok(sa)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Option<Value>> {
        let row = sqlx::query_as::<_, (String, String, String, String, String, bool, String, String, i64, i64, String, Option<String>)>(
            "SELECT uid, name, namespace, secrets, image_pull_secrets, automount_service_account_token,
             labels, annotations, resource_version, generation, creation_timestamp, deletion_timestamp
             FROM serviceaccounts
             WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(namespace)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((uid, name, namespace, secrets, image_pull_secrets, automount, labels, annotations, resource_version, generation, creation_timestamp, _deletion_timestamp)) = row {
            tracing::debug!("Got SA data - labels: {}, annotations: {}, secrets: {}", labels, annotations, secrets);

            let sa = json!({
                "apiVersion": "v1",
                "kind": "ServiceAccount",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "namespace": namespace,
                    "resourceVersion": resource_version.to_string(),
                    "generation": generation,
                    "creationTimestamp": creation_timestamp,
                    "labels": serde_json::from_str::<Value>(&labels)?,
                    "annotations": serde_json::from_str::<Value>(&annotations)?
                },
                "secrets": serde_json::from_str::<Value>(&secrets)?,
                "imagePullSecrets": serde_json::from_str::<Value>(&image_pull_secrets)?,
                "automountServiceAccountToken": automount
            });

            Ok(Some(sa))
        } else {
            Ok(None)
        }
    }

    pub async fn update(&self, namespace: &str, name: &str, mut sa: Value) -> Result<Value> {
        let current = self.get(namespace, name).await?
            .ok_or_else(|| anyhow::anyhow!("ServiceAccount not found"))?;

        let uid = current["metadata"]["uid"].as_str().unwrap();
        let resource_version: i64 = current["metadata"]["resourceVersion"].as_str().unwrap().parse()?;
        let new_resource_version = resource_version + 1;
        let generation: i64 = current["metadata"]["generation"].as_i64().unwrap();
        
        // Check if spec changed (secrets, imagePullSecrets, automount)
        let spec_changed = sa.get("secrets") != current.get("secrets") ||
                          sa.get("imagePullSecrets") != current.get("imagePullSecrets") ||
                          sa.get("automountServiceAccountToken") != current.get("automountServiceAccountToken");
        
        let new_generation = if spec_changed { generation + 1 } else { generation };

        let secrets = sa.get("secrets").cloned().unwrap_or(json!([]));
        let image_pull_secrets = sa.get("imagePullSecrets").cloned().unwrap_or(json!([]));
        let automount = sa.get("automountServiceAccountToken").and_then(|v| v.as_bool()).unwrap_or(true);
        let labels = sa["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = sa["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "UPDATE serviceaccounts 
             SET secrets = ?, image_pull_secrets = ?, automount_service_account_token = ?,
                 labels = ?, annotations = ?, resource_version = ?, generation = ?
             WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(serde_json::to_string(&secrets)?)
        .bind(serde_json::to_string(&image_pull_secrets)?)
        .bind(automount)
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .bind(new_resource_version)
        .bind(new_generation)
        .bind(namespace)
        .bind(name)
        .execute(&self.pool)
        .await?;

        sa["metadata"]["uid"] = json!(uid);
        sa["metadata"]["resourceVersion"] = json!(new_resource_version.to_string());
        sa["metadata"]["generation"] = json!(new_generation);
        sa["metadata"]["creationTimestamp"] = current["metadata"]["creationTimestamp"].clone();

        self.record_event(
            uid,
            "ServiceAccount",
            namespace,
            name,
            "Updated",
            "ServiceAccount updated"
        ).await?;

        Ok(sa)
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        let sa = self.get(namespace, name).await?
            .ok_or_else(|| anyhow::anyhow!("ServiceAccount not found"))?;

        let uid = sa["metadata"]["uid"].as_str().unwrap();

        sqlx::query(
            "UPDATE serviceaccounts 
             SET deletion_timestamp = CURRENT_TIMESTAMP 
             WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(namespace)
        .bind(name)
        .execute(&self.pool)
        .await?;

        self.record_event(
            uid,
            "ServiceAccount",
            namespace,
            name,
            "Deleted",
            "ServiceAccount deleted"
        ).await?;

        Ok(sa)
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let rows = if let Some(ns) = namespace {
            sqlx::query_as::<_, (String, String, String, String, String, bool, String, String, i64, i64, String)>(
                "SELECT uid, name, namespace, secrets, image_pull_secrets, automount_service_account_token,
                 labels, annotations, resource_version, generation, creation_timestamp
                 FROM serviceaccounts
                 WHERE namespace = ? AND deletion_timestamp IS NULL"
            )
            .bind(ns)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, (String, String, String, String, String, bool, String, String, i64, i64, String)>(
                "SELECT uid, name, namespace, secrets, image_pull_secrets, automount_service_account_token,
                 labels, annotations, resource_version, generation, creation_timestamp
                 FROM serviceaccounts
                 WHERE deletion_timestamp IS NULL"
            )
            .fetch_all(&self.pool)
            .await?
        };
        
        let mut items = Vec::new();

        for (uid, name, namespace, secrets, image_pull_secrets, automount, labels, annotations, resource_version, generation, creation_timestamp) in rows {

            let sa = json!({
                "apiVersion": "v1",
                "kind": "ServiceAccount",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "namespace": namespace,
                    "resourceVersion": resource_version.to_string(),
                    "generation": generation,
                    "creationTimestamp": creation_timestamp,
                    "labels": serde_json::from_str::<Value>(&labels)?,
                    "annotations": serde_json::from_str::<Value>(&annotations)?
                },
                "secrets": serde_json::from_str::<Value>(&secrets)?,
                "imagePullSecrets": serde_json::from_str::<Value>(&image_pull_secrets)?,
                "automountServiceAccountToken": automount
            });

            items.push(sa);
        }

        Ok(json!({
            "apiVersion": "v1",
            "kind": "ServiceAccountList",
            "items": items
        }))
    }

    pub async fn create_token(&self, namespace: &str, name: &str, token_request: Value) -> Result<Value> {
        // Verify ServiceAccount exists
        let sa = self.get(namespace, name).await?
            .ok_or_else(|| anyhow::anyhow!("ServiceAccount not found"))?;

        let sa_uid = sa["metadata"]["uid"].as_str().unwrap();
        let token_uid = Uuid::new_v4().to_string();
        
        // Generate a simple token (in production, this would be a proper JWT)
        let token = format!("eyJ0eXAiOiJKV1QiLCJhbGciOiJSUzI1NiJ9.{}", Uuid::new_v4());
        
        let spec = token_request.get("spec").cloned().unwrap_or(json!({}));
        let audiences = spec.get("audiences").cloned().unwrap_or(json!(["api"]));
        let expiration_seconds = spec.get("expirationSeconds").and_then(|v| v.as_i64()).unwrap_or(3600);
        let bound_object_ref = spec.get("boundObjectRef").cloned();
        
        let expiration_timestamp = chrono::Utc::now() + chrono::Duration::seconds(expiration_seconds);
        
        sqlx::query(
            "INSERT INTO tokenrequests (uid, service_account_uid, namespace, service_account_name,
             audiences, expiration_seconds, bound_object_ref, token, expiration_timestamp)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&token_uid)
        .bind(sa_uid)
        .bind(namespace)
        .bind(name)
        .bind(serde_json::to_string(&audiences)?)
        .bind(expiration_seconds)
        .bind(bound_object_ref.as_ref().map(|o| serde_json::to_string(o).ok()).flatten())
        .bind(&token)
        .bind(expiration_timestamp.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(json!({
            "apiVersion": "authentication.k8s.io/v1",
            "kind": "TokenRequest",
            "metadata": {
                "name": name,
                "namespace": namespace,
                "creationTimestamp": chrono::Utc::now().to_rfc3339()
            },
            "spec": spec,
            "status": {
                "token": token,
                "expirationTimestamp": expiration_timestamp.to_rfc3339()
            }
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