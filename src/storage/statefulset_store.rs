use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct StatefulSetStore {
    pool: SqlitePool,
}

impl StatefulSetStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut statefulset: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = statefulset["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("StatefulSet name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Extract spec fields
        let replicas = statefulset["spec"]["replicas"].as_i64().unwrap_or(1);
        let selector = statefulset["spec"]["selector"].clone();
        if selector.is_null() {
            return Err(anyhow!("StatefulSet selector is required"));
        }
        
        let service_name = statefulset["spec"]["serviceName"]
            .as_str()
            .ok_or_else(|| anyhow!("StatefulSet serviceName is required"))?
            .to_string();
        
        let pod_management_policy = statefulset["spec"]
            .get("podManagementPolicy")
            .and_then(|v| v.as_str())
            .unwrap_or("OrderedReady")
            .to_string();
        
        let update_strategy = statefulset["spec"].get("updateStrategy")
            .cloned()
            .unwrap_or_else(|| json!({
                "type": "RollingUpdate",
                "rollingUpdate": {
                    "partition": 0
                }
            }));
        
        let revision_history_limit = statefulset["spec"]
            .get("revisionHistoryLimit")
            .and_then(|v| v.as_i64())
            .unwrap_or(10);
        
        let min_ready_seconds = statefulset["spec"]
            .get("minReadySeconds")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        
        let pvc_retention_policy = statefulset["spec"]
            .get("persistentVolumeClaimRetentionPolicy")
            .cloned();
        
        let ordinals = statefulset["spec"].get("ordinals").cloned();
        
        let template = statefulset["spec"]["template"].clone();
        if template.is_null() {
            return Err(anyhow!("StatefulSet template is required"));
        }
        
        let volume_claim_templates = statefulset["spec"]
            .get("volumeClaimTemplates")
            .cloned();
        
        let labels = statefulset["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = statefulset["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        // Insert into database
        let query = r#"
            INSERT INTO statefulsets (
                uid, namespace, name, replicas, selector, service_name, 
                pod_management_policy, update_strategy, revision_history_limit,
                min_ready_seconds, persistent_volume_claim_retention_policy, ordinals,
                template, volume_claim_templates, labels, annotations, 
                resource_version, creation_timestamp
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 1, ?17)
        "#;
        
        sqlx::query(query)
            .bind(&uid)
            .bind(namespace)
            .bind(&name)
            .bind(replicas)
            .bind(selector.to_string())
            .bind(&service_name)
            .bind(&pod_management_policy)
            .bind(update_strategy.to_string())
            .bind(revision_history_limit)
            .bind(min_ready_seconds)
            .bind(pvc_retention_policy.as_ref().map(|v| v.to_string()))
            .bind(ordinals.as_ref().map(|v| v.to_string()))
            .bind(template.to_string())
            .bind(volume_claim_templates.as_ref().map(|v| v.to_string()))
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(&now)
            .execute(&self.pool)
            .await?;

        // Build response
        statefulset["apiVersion"] = json!("apps/v1");
        statefulset["kind"] = json!("StatefulSet");
        statefulset["metadata"]["uid"] = json!(uid);
        statefulset["metadata"]["namespace"] = json!(namespace);
        statefulset["metadata"]["resourceVersion"] = json!("1");
        statefulset["metadata"]["generation"] = json!(1);
        statefulset["metadata"]["creationTimestamp"] = json!(now);
        statefulset["metadata"]["selfLink"] = json!(format!("/apis/apps/v1/namespaces/{}/statefulsets/{}", namespace, name));
        
        // Set default spec values if not present
        statefulset["spec"]["replicas"] = json!(replicas);
        statefulset["spec"]["podManagementPolicy"] = json!(pod_management_policy);
        statefulset["spec"]["updateStrategy"] = update_strategy;
        statefulset["spec"]["revisionHistoryLimit"] = json!(revision_history_limit);
        statefulset["spec"]["minReadySeconds"] = json!(min_ready_seconds);
        
        // Set initial status
        statefulset["status"] = json!({
            "observedGeneration": 0,
            "replicas": 0,
            "readyReplicas": 0,
            "currentReplicas": 0,
            "updatedReplicas": 0,
            "availableReplicas": 0,
            "collisionCount": 0
        });
        
        if !labels.is_null() && labels != json!({}) {
            statefulset["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            statefulset["metadata"]["annotations"] = annotations;
        }

        Ok(statefulset)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let query = r#"
            SELECT uid, replicas, selector, service_name, pod_management_policy,
                   update_strategy, revision_history_limit, min_ready_seconds,
                   persistent_volume_claim_retention_policy, ordinals,
                   template, volume_claim_templates,
                   observed_generation, replicas_status, ready_replicas, current_replicas,
                   updated_replicas, current_revision, update_revision, collision_count,
                   available_replicas, conditions,
                   labels, annotations, resource_version, generation, creation_timestamp 
            FROM statefulsets 
            WHERE namespace = ?1 AND name = ?2 AND deletion_timestamp IS NULL
        "#;
        
        let row = sqlx::query(query)
            .bind(namespace)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => {
                let statefulset = self.row_to_statefulset(row, namespace, name)?;
                Ok(statefulset)
            }
            None => Err(anyhow!("StatefulSet {}/{} not found", namespace, name)),
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if namespace.is_some() {
            r#"
                SELECT uid, namespace, name, replicas, selector, service_name, pod_management_policy,
                       update_strategy, revision_history_limit, min_ready_seconds,
                       persistent_volume_claim_retention_policy, ordinals,
                       template, volume_claim_templates,
                       observed_generation, replicas_status, ready_replicas, current_replicas,
                       updated_replicas, current_revision, update_revision, collision_count,
                       available_replicas, conditions,
                       labels, annotations, resource_version, generation, creation_timestamp 
                FROM statefulsets 
                WHERE namespace = ?1 AND deletion_timestamp IS NULL 
                ORDER BY name
            "#
        } else {
            r#"
                SELECT uid, namespace, name, replicas, selector, service_name, pod_management_policy,
                       update_strategy, revision_history_limit, min_ready_seconds,
                       persistent_volume_claim_retention_policy, ordinals,
                       template, volume_claim_templates,
                       observed_generation, replicas_status, ready_replicas, current_replicas,
                       updated_replicas, current_revision, update_revision, collision_count,
                       available_replicas, conditions,
                       labels, annotations, resource_version, generation, creation_timestamp 
                FROM statefulsets 
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
            let statefulset = self.row_to_statefulset(row, &ns, &name)?;
            items.push(statefulset);
        }

        Ok(json!({
            "apiVersion": "apps/v1",
            "kind": "StatefulSetList",
            "metadata": {
                "selfLink": if let Some(ns) = namespace {
                    format!("/apis/apps/v1/namespaces/{}/statefulsets", ns)
                } else {
                    "/apis/apps/v1/statefulsets".to_string()
                },
                "resourceVersion": "1",
            },
            "items": items,
        }))
    }

    pub async fn update(&self, namespace: &str, name: &str, statefulset: Value) -> Result<Value> {
        // Extract spec fields
        let replicas = statefulset["spec"]["replicas"].as_i64().unwrap_or(1);
        let selector = statefulset["spec"]["selector"].clone();
        if selector.is_null() {
            return Err(anyhow!("StatefulSet selector is required"));
        }
        
        let service_name = statefulset["spec"]["serviceName"]
            .as_str()
            .ok_or_else(|| anyhow!("StatefulSet serviceName is required"))?
            .to_string();
        
        let pod_management_policy = statefulset["spec"]
            .get("podManagementPolicy")
            .and_then(|v| v.as_str())
            .unwrap_or("OrderedReady")
            .to_string();
        
        let update_strategy = statefulset["spec"].get("updateStrategy")
            .cloned()
            .unwrap_or_else(|| json!({
                "type": "RollingUpdate",
                "rollingUpdate": {
                    "partition": 0
                }
            }));
        
        let template = statefulset["spec"]["template"].clone();
        if template.is_null() {
            return Err(anyhow!("StatefulSet template is required"));
        }
        
        let volume_claim_templates = statefulset["spec"]
            .get("volumeClaimTemplates")
            .cloned();
        
        let labels = statefulset["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = statefulset["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        let update_query = r#"
            UPDATE statefulsets 
            SET replicas = ?1, selector = ?2, service_name = ?3, pod_management_policy = ?4,
                update_strategy = ?5, template = ?6, volume_claim_templates = ?7,
                labels = ?8, annotations = ?9, 
                resource_version = resource_version + 1, generation = generation + 1
            WHERE namespace = ?10 AND name = ?11 AND deletion_timestamp IS NULL
        "#;

        let rows_affected = sqlx::query(update_query)
            .bind(replicas)
            .bind(selector.to_string())
            .bind(&service_name)
            .bind(&pod_management_policy)
            .bind(update_strategy.to_string())
            .bind(template.to_string())
            .bind(volume_claim_templates.as_ref().map(|v| v.to_string()))
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?
            .rows_affected();

        if rows_affected == 0 {
            return Err(anyhow!("StatefulSet {}/{} not found", namespace, name));
        }

        self.get(namespace, name).await
    }

    pub async fn update_scale(&self, namespace: &str, name: &str, replicas: i64) -> Result<Value> {
        let update_query = r#"
            UPDATE statefulsets 
            SET replicas = ?1, resource_version = resource_version + 1
            WHERE namespace = ?2 AND name = ?3 AND deletion_timestamp IS NULL
        "#;

        let rows_affected = sqlx::query(update_query)
            .bind(replicas)
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?
            .rows_affected();

        if rows_affected == 0 {
            return Err(anyhow!("StatefulSet {}/{} not found", namespace, name));
        }

        // Return scale object
        let statefulset = self.get(namespace, name).await?;
        Ok(json!({
            "apiVersion": "autoscaling/v1",
            "kind": "Scale",
            "metadata": statefulset["metadata"],
            "spec": {
                "replicas": replicas
            },
            "status": {
                "replicas": statefulset["status"]["replicas"],
                "selector": statefulset["spec"]["selector"]
            }
        }))
    }

    pub async fn update_status(&self, namespace: &str, name: &str, status: Value) -> Result<()> {
        let update_query = r#"
            UPDATE statefulsets 
            SET observed_generation = ?1, replicas_status = ?2, ready_replicas = ?3,
                current_replicas = ?4, updated_replicas = ?5, current_revision = ?6,
                update_revision = ?7, collision_count = ?8, available_replicas = ?9,
                conditions = ?10, resource_version = resource_version + 1
            WHERE namespace = ?11 AND name = ?12 AND deletion_timestamp IS NULL
        "#;

        sqlx::query(update_query)
            .bind(status["observedGeneration"].as_i64().unwrap_or(0))
            .bind(status["replicas"].as_i64().unwrap_or(0))
            .bind(status["readyReplicas"].as_i64().unwrap_or(0))
            .bind(status["currentReplicas"].as_i64().unwrap_or(0))
            .bind(status["updatedReplicas"].as_i64().unwrap_or(0))
            .bind(status.get("currentRevision").and_then(|v| v.as_str()))
            .bind(status.get("updateRevision").and_then(|v| v.as_str()))
            .bind(status["collisionCount"].as_i64().unwrap_or(0))
            .bind(status["availableReplicas"].as_i64().unwrap_or(0))
            .bind(status.get("conditions").map(|v| v.to_string()))
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        // Get the StatefulSet before deletion
        let statefulset = self.get(namespace, name).await?;

        // Soft delete
        let deletion_timestamp = Utc::now().to_rfc3339();
        let delete_query = "UPDATE statefulsets SET deletion_timestamp = ?1 WHERE namespace = ?2 AND name = ?3";
        
        sqlx::query(delete_query)
            .bind(&deletion_timestamp)
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(statefulset)
    }

    fn row_to_statefulset(&self, row: sqlx::sqlite::SqliteRow, namespace: &str, name: &str) -> Result<Value> {
        let uid: String = row.get("uid");
        let replicas: i64 = row.get("replicas");
        let selector_str: String = row.get("selector");
        let service_name: String = row.get("service_name");
        let pod_management_policy: String = row.get("pod_management_policy");
        let update_strategy_str: String = row.get("update_strategy");
        let revision_history_limit: i64 = row.get("revision_history_limit");
        let min_ready_seconds: i64 = row.get("min_ready_seconds");
        let pvc_retention_policy_str: Option<String> = row.get("persistent_volume_claim_retention_policy");
        let ordinals_str: Option<String> = row.get("ordinals");
        let template_str: String = row.get("template");
        let volume_claim_templates_str: Option<String> = row.get("volume_claim_templates");
        
        let observed_generation: i64 = row.get("observed_generation");
        let replicas_status: i64 = row.get("replicas_status");
        let ready_replicas: i64 = row.get("ready_replicas");
        let current_replicas: i64 = row.get("current_replicas");
        let updated_replicas: i64 = row.get("updated_replicas");
        let current_revision: Option<String> = row.get("current_revision");
        let update_revision: Option<String> = row.get("update_revision");
        let collision_count: i64 = row.get("collision_count");
        let available_replicas: i64 = row.get("available_replicas");
        let conditions_str: Option<String> = row.get("conditions");
        
        let labels_str: String = row.get("labels");
        let annotations_str: String = row.get("annotations");
        let resource_version: i64 = row.get("resource_version");
        let generation: i64 = row.get("generation");
        let creation_timestamp: String = row.get("creation_timestamp");

        let selector: Value = serde_json::from_str(&selector_str)?;
        let update_strategy: Value = serde_json::from_str(&update_strategy_str)?;
        let template: Value = serde_json::from_str(&template_str)?;
        let pvc_retention_policy: Option<Value> = pvc_retention_policy_str.map(|s| serde_json::from_str(&s)).transpose()?;
        let ordinals: Option<Value> = ordinals_str.map(|s| serde_json::from_str(&s)).transpose()?;
        let volume_claim_templates: Option<Value> = volume_claim_templates_str.map(|s| serde_json::from_str(&s)).transpose()?;
        let conditions: Option<Value> = conditions_str.map(|s| serde_json::from_str(&s)).transpose()?;
        let labels: Value = serde_json::from_str(&labels_str)?;
        let annotations: Value = serde_json::from_str(&annotations_str)?;

        let mut statefulset = json!({
            "apiVersion": "apps/v1",
            "kind": "StatefulSet",
            "metadata": {
                "name": name,
                "namespace": namespace,
                "uid": uid,
                "resourceVersion": resource_version.to_string(),
                "generation": generation,
                "creationTimestamp": creation_timestamp,
                "selfLink": format!("/apis/apps/v1/namespaces/{}/statefulsets/{}", namespace, name),
            },
            "spec": {
                "replicas": replicas,
                "selector": selector,
                "serviceName": service_name,
                "podManagementPolicy": pod_management_policy,
                "updateStrategy": update_strategy,
                "revisionHistoryLimit": revision_history_limit,
                "minReadySeconds": min_ready_seconds,
                "template": template
            },
            "status": {
                "observedGeneration": observed_generation,
                "replicas": replicas_status,
                "readyReplicas": ready_replicas,
                "currentReplicas": current_replicas,
                "updatedReplicas": updated_replicas,
                "collisionCount": collision_count,
                "availableReplicas": available_replicas
            }
        });
        
        // Add optional spec fields
        if let Some(pvc_rp) = pvc_retention_policy {
            statefulset["spec"]["persistentVolumeClaimRetentionPolicy"] = pvc_rp;
        }
        if let Some(ord) = ordinals {
            statefulset["spec"]["ordinals"] = ord;
        }
        if let Some(vct) = volume_claim_templates {
            statefulset["spec"]["volumeClaimTemplates"] = vct;
        }
        
        // Add optional status fields
        if let Some(cr) = current_revision {
            statefulset["status"]["currentRevision"] = json!(cr);
        }
        if let Some(ur) = update_revision {
            statefulset["status"]["updateRevision"] = json!(ur);
        }
        if let Some(cond) = conditions {
            statefulset["status"]["conditions"] = cond;
        }

        if !labels.is_null() && labels != json!({}) {
            statefulset["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            statefulset["metadata"]["annotations"] = annotations;
        }

        Ok(statefulset)
    }
}