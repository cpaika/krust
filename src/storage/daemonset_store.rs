use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct DaemonSetStore {
    pool: SqlitePool,
}

impl DaemonSetStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut daemonset: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = daemonset["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("DaemonSet name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Extract spec fields
        let selector = daemonset["spec"]["selector"].clone();
        if selector.is_null() {
            return Err(anyhow!("DaemonSet selector is required"));
        }
        
        let template = daemonset["spec"]["template"].clone();
        if template.is_null() {
            return Err(anyhow!("DaemonSet template is required"));
        }
        
        let update_strategy = daemonset["spec"].get("updateStrategy")
            .cloned()
            .unwrap_or_else(|| json!({
                "type": "RollingUpdate",
                "rollingUpdate": {
                    "maxUnavailable": 1
                }
            }));
        
        let min_ready_seconds = daemonset["spec"]
            .get("minReadySeconds")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        
        let revision_history_limit = daemonset["spec"]
            .get("revisionHistoryLimit")
            .and_then(|v| v.as_i64())
            .unwrap_or(10);
        
        let labels = daemonset["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = daemonset["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        // Insert into database
        let query = r#"
            INSERT INTO daemonsets (
                uid, namespace, name, selector, template, update_strategy,
                min_ready_seconds, revision_history_limit,
                labels, annotations, resource_version, creation_timestamp
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11)
        "#;
        
        sqlx::query(query)
            .bind(&uid)
            .bind(namespace)
            .bind(&name)
            .bind(selector.to_string())
            .bind(template.to_string())
            .bind(update_strategy.to_string())
            .bind(min_ready_seconds)
            .bind(revision_history_limit)
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(&now)
            .execute(&self.pool)
            .await?;

        // Build response
        daemonset["apiVersion"] = json!("apps/v1");
        daemonset["kind"] = json!("DaemonSet");
        daemonset["metadata"]["uid"] = json!(uid);
        daemonset["metadata"]["namespace"] = json!(namespace);
        daemonset["metadata"]["resourceVersion"] = json!("1");
        daemonset["metadata"]["generation"] = json!(1);
        daemonset["metadata"]["creationTimestamp"] = json!(now);
        daemonset["metadata"]["selfLink"] = json!(format!("/apis/apps/v1/namespaces/{}/daemonsets/{}", namespace, name));
        
        // Set default spec values if not present
        daemonset["spec"]["updateStrategy"] = update_strategy;
        daemonset["spec"]["minReadySeconds"] = json!(min_ready_seconds);
        daemonset["spec"]["revisionHistoryLimit"] = json!(revision_history_limit);
        
        // Set initial status
        daemonset["status"] = json!({
            "currentNumberScheduled": 0,
            "numberMisscheduled": 0,
            "desiredNumberScheduled": 0,
            "numberReady": 0,
            "observedGeneration": 0,
            "updatedNumberScheduled": 0,
            "numberAvailable": 0,
            "numberUnavailable": 0,
            "collisionCount": 0
        });
        
        if !labels.is_null() && labels != json!({}) {
            daemonset["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            daemonset["metadata"]["annotations"] = annotations;
        }

        Ok(daemonset)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let query = r#"
            SELECT uid, selector, template, update_strategy,
                   min_ready_seconds, revision_history_limit,
                   current_number_scheduled, number_misscheduled, desired_number_scheduled,
                   number_ready, observed_generation, updated_number_scheduled,
                   number_available, number_unavailable, collision_count, conditions,
                   labels, annotations, resource_version, generation, creation_timestamp 
            FROM daemonsets 
            WHERE namespace = ?1 AND name = ?2 AND deletion_timestamp IS NULL
        "#;
        
        let row = sqlx::query(query)
            .bind(namespace)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => {
                let daemonset = self.row_to_daemonset(row, namespace, name)?;
                Ok(daemonset)
            }
            None => Err(anyhow!("DaemonSet {}/{} not found", namespace, name)),
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if namespace.is_some() {
            r#"
                SELECT uid, namespace, name, selector, template, update_strategy,
                       min_ready_seconds, revision_history_limit,
                       current_number_scheduled, number_misscheduled, desired_number_scheduled,
                       number_ready, observed_generation, updated_number_scheduled,
                       number_available, number_unavailable, collision_count, conditions,
                       labels, annotations, resource_version, generation, creation_timestamp 
                FROM daemonsets 
                WHERE namespace = ?1 AND deletion_timestamp IS NULL 
                ORDER BY name
            "#
        } else {
            r#"
                SELECT uid, namespace, name, selector, template, update_strategy,
                       min_ready_seconds, revision_history_limit,
                       current_number_scheduled, number_misscheduled, desired_number_scheduled,
                       number_ready, observed_generation, updated_number_scheduled,
                       number_available, number_unavailable, collision_count, conditions,
                       labels, annotations, resource_version, generation, creation_timestamp 
                FROM daemonsets 
                WHERE deletion_timestamp IS NULL 
                ORDER BY namespace, name
            "#
        };

        let rows = if let Some(ns) = namespace {
            sqlx::query(query)
                .bind(ns)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query(query)
                .fetch_all(&self.pool)
                .await?
        };

        let mut items = Vec::new();
        for row in rows {
            let ns: String = row.get("namespace");
            let name: String = row.get("name");
            let daemonset = self.row_to_daemonset(row, &ns, &name)?;
            items.push(daemonset);
        }

        Ok(json!({
            "apiVersion": "apps/v1",
            "kind": "DaemonSetList",
            "metadata": {
                "selfLink": if let Some(ns) = namespace {
                    format!("/apis/apps/v1/namespaces/{}/daemonsets", ns)
                } else {
                    "/apis/apps/v1/daemonsets".to_string()
                },
                "resourceVersion": "1",
            },
            "items": items,
        }))
    }

    pub async fn update(&self, namespace: &str, name: &str, daemonset: Value) -> Result<Value> {
        // Extract spec fields
        let selector = daemonset["spec"]["selector"].clone();
        if selector.is_null() {
            return Err(anyhow!("DaemonSet selector is required"));
        }
        
        let template = daemonset["spec"]["template"].clone();
        if template.is_null() {
            return Err(anyhow!("DaemonSet template is required"));
        }
        
        let update_strategy = daemonset["spec"].get("updateStrategy")
            .cloned()
            .unwrap_or_else(|| json!({
                "type": "RollingUpdate",
                "rollingUpdate": {
                    "maxUnavailable": 1
                }
            }));
        
        let min_ready_seconds = daemonset["spec"]
            .get("minReadySeconds")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        
        let revision_history_limit = daemonset["spec"]
            .get("revisionHistoryLimit")
            .and_then(|v| v.as_i64())
            .unwrap_or(10);
        
        let labels = daemonset["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = daemonset["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        let update_query = r#"
            UPDATE daemonsets 
            SET selector = ?1, template = ?2, update_strategy = ?3,
                min_ready_seconds = ?4, revision_history_limit = ?5,
                labels = ?6, annotations = ?7, 
                resource_version = resource_version + 1, generation = generation + 1
            WHERE namespace = ?8 AND name = ?9 AND deletion_timestamp IS NULL
        "#;

        let rows_affected = sqlx::query(update_query)
            .bind(selector.to_string())
            .bind(template.to_string())
            .bind(update_strategy.to_string())
            .bind(min_ready_seconds)
            .bind(revision_history_limit)
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?
            .rows_affected();

        if rows_affected == 0 {
            return Err(anyhow!("DaemonSet {}/{} not found", namespace, name));
        }

        self.get(namespace, name).await
    }

    pub async fn update_status(&self, namespace: &str, name: &str, status: Value) -> Result<()> {
        let update_query = r#"
            UPDATE daemonsets 
            SET current_number_scheduled = ?1, number_misscheduled = ?2,
                desired_number_scheduled = ?3, number_ready = ?4,
                observed_generation = ?5, updated_number_scheduled = ?6,
                number_available = ?7, number_unavailable = ?8,
                collision_count = ?9, conditions = ?10,
                resource_version = resource_version + 1
            WHERE namespace = ?11 AND name = ?12 AND deletion_timestamp IS NULL
        "#;

        sqlx::query(update_query)
            .bind(status["currentNumberScheduled"].as_i64().unwrap_or(0))
            .bind(status["numberMisscheduled"].as_i64().unwrap_or(0))
            .bind(status["desiredNumberScheduled"].as_i64().unwrap_or(0))
            .bind(status["numberReady"].as_i64().unwrap_or(0))
            .bind(status["observedGeneration"].as_i64().unwrap_or(0))
            .bind(status["updatedNumberScheduled"].as_i64().unwrap_or(0))
            .bind(status["numberAvailable"].as_i64().unwrap_or(0))
            .bind(status["numberUnavailable"].as_i64().unwrap_or(0))
            .bind(status["collisionCount"].as_i64().unwrap_or(0))
            .bind(status.get("conditions").map(|v| v.to_string()))
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        // Get the DaemonSet before deletion
        let daemonset = self.get(namespace, name).await?;

        // Soft delete
        let deletion_timestamp = Utc::now().to_rfc3339();
        let delete_query = "UPDATE daemonsets SET deletion_timestamp = ?1 WHERE namespace = ?2 AND name = ?3";
        
        sqlx::query(delete_query)
            .bind(&deletion_timestamp)
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(daemonset)
    }

    fn row_to_daemonset(&self, row: sqlx::sqlite::SqliteRow, namespace: &str, name: &str) -> Result<Value> {
        let uid: String = row.get("uid");
        let selector_str: String = row.get("selector");
        let template_str: String = row.get("template");
        let update_strategy_str: String = row.get("update_strategy");
        let min_ready_seconds: i64 = row.get("min_ready_seconds");
        let revision_history_limit: i64 = row.get("revision_history_limit");
        
        let current_number_scheduled: i64 = row.get("current_number_scheduled");
        let number_misscheduled: i64 = row.get("number_misscheduled");
        let desired_number_scheduled: i64 = row.get("desired_number_scheduled");
        let number_ready: i64 = row.get("number_ready");
        let observed_generation: i64 = row.get("observed_generation");
        let updated_number_scheduled: i64 = row.get("updated_number_scheduled");
        let number_available: i64 = row.get("number_available");
        let number_unavailable: i64 = row.get("number_unavailable");
        let collision_count: i64 = row.get("collision_count");
        let conditions_str: Option<String> = row.get("conditions");
        
        let labels_str: String = row.get("labels");
        let annotations_str: String = row.get("annotations");
        let resource_version: i64 = row.get("resource_version");
        let generation: i64 = row.get("generation");
        let creation_timestamp: String = row.get("creation_timestamp");

        let selector: Value = serde_json::from_str(&selector_str)?;
        let template: Value = serde_json::from_str(&template_str)?;
        let update_strategy: Value = serde_json::from_str(&update_strategy_str)?;
        let conditions: Option<Value> = conditions_str.map(|s| serde_json::from_str(&s)).transpose()?;
        let labels: Value = serde_json::from_str(&labels_str)?;
        let annotations: Value = serde_json::from_str(&annotations_str)?;

        let mut daemonset = json!({
            "apiVersion": "apps/v1",
            "kind": "DaemonSet",
            "metadata": {
                "name": name,
                "namespace": namespace,
                "uid": uid,
                "resourceVersion": resource_version.to_string(),
                "generation": generation,
                "creationTimestamp": creation_timestamp,
                "selfLink": format!("/apis/apps/v1/namespaces/{}/daemonsets/{}", namespace, name),
            },
            "spec": {
                "selector": selector,
                "template": template,
                "updateStrategy": update_strategy,
                "minReadySeconds": min_ready_seconds,
                "revisionHistoryLimit": revision_history_limit
            },
            "status": {
                "currentNumberScheduled": current_number_scheduled,
                "numberMisscheduled": number_misscheduled,
                "desiredNumberScheduled": desired_number_scheduled,
                "numberReady": number_ready,
                "observedGeneration": observed_generation,
                "updatedNumberScheduled": updated_number_scheduled,
                "numberAvailable": number_available,
                "numberUnavailable": number_unavailable,
                "collisionCount": collision_count
            }
        });
        
        if let Some(cond) = conditions {
            daemonset["status"]["conditions"] = cond;
        }

        if !labels.is_null() && labels != json!({}) {
            daemonset["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            daemonset["metadata"]["annotations"] = annotations;
        }

        Ok(daemonset)
    }
}