use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct JobStore {
    pool: SqlitePool,
}

impl JobStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut job: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = job["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("Job name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Extract spec fields
        let parallelism = job["spec"].get("parallelism")
            .and_then(|v| v.as_i64())
            .unwrap_or(1);
        
        let completions = job["spec"].get("completions")
            .and_then(|v| v.as_i64());
        
        let active_deadline_seconds = job["spec"].get("activeDeadlineSeconds")
            .and_then(|v| v.as_i64());
        
        let backoff_limit = job["spec"].get("backoffLimit")
            .and_then(|v| v.as_i64())
            .unwrap_or(6);
        
        let selector = job["spec"].get("selector").cloned();
        
        let manual_selector = job["spec"].get("manualSelector")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        let template = job["spec"]["template"].clone();
        if template.is_null() {
            return Err(anyhow!("Job template is required"));
        }
        
        let ttl_seconds_after_finished = job["spec"].get("ttlSecondsAfterFinished")
            .and_then(|v| v.as_i64());
        
        let completion_mode = job["spec"].get("completionMode")
            .and_then(|v| v.as_str())
            .unwrap_or("NonIndexed")
            .to_string();
        
        let suspend = job["spec"].get("suspend")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        let labels = job["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = job["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        // Auto-generate selector if not provided and not manual
        let final_selector = if selector.is_none() && !manual_selector {
            json!({
                "matchLabels": {
                    "controller-uid": &uid
                }
            })
        } else {
            selector.unwrap_or_else(|| json!({}))
        };

        // Insert into database
        let query = r#"
            INSERT INTO jobs (
                uid, namespace, name, parallelism, completions, active_deadline_seconds,
                backoff_limit, selector, manual_selector, template, ttl_seconds_after_finished,
                completion_mode, suspend, labels, annotations, resource_version, creation_timestamp
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 1, ?16)
        "#;
        
        sqlx::query(query)
            .bind(&uid)
            .bind(namespace)
            .bind(&name)
            .bind(parallelism)
            .bind(completions)
            .bind(active_deadline_seconds)
            .bind(backoff_limit)
            .bind(final_selector.to_string())
            .bind(manual_selector)
            .bind(template.to_string())
            .bind(ttl_seconds_after_finished)
            .bind(&completion_mode)
            .bind(suspend)
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(&now)
            .execute(&self.pool)
            .await?;

        // Build response
        job["apiVersion"] = json!("batch/v1");
        job["kind"] = json!("Job");
        job["metadata"]["uid"] = json!(uid);
        job["metadata"]["namespace"] = json!(namespace);
        job["metadata"]["resourceVersion"] = json!("1");
        job["metadata"]["generation"] = json!(1);
        job["metadata"]["creationTimestamp"] = json!(now);
        job["metadata"]["selfLink"] = json!(format!("/apis/batch/v1/namespaces/{}/jobs/{}", namespace, name));
        
        // Set spec defaults
        job["spec"]["parallelism"] = json!(parallelism);
        job["spec"]["backoffLimit"] = json!(backoff_limit);
        job["spec"]["completionMode"] = json!(completion_mode);
        job["spec"]["suspend"] = json!(suspend);
        job["spec"]["selector"] = final_selector;
        
        // Set initial status
        job["status"] = json!({
            "active": 0,
            "succeeded": 0,
            "failed": 0,
            "conditions": []
        });
        
        if !labels.is_null() && labels != json!({}) {
            job["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            job["metadata"]["annotations"] = annotations;
        }

        Ok(job)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let query = r#"
            SELECT uid, parallelism, completions, active_deadline_seconds, backoff_limit,
                   selector, manual_selector, template, ttl_seconds_after_finished,
                   completion_mode, suspend, conditions, start_time, completion_time,
                   active, succeeded, failed, completed_indexes, uncounted_terminated_pods, ready,
                   labels, annotations, resource_version, generation, creation_timestamp 
            FROM jobs 
            WHERE namespace = ?1 AND name = ?2 AND deletion_timestamp IS NULL
        "#;
        
        let row = sqlx::query(query)
            .bind(namespace)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => {
                let job = self.row_to_job(row, namespace, name)?;
                Ok(job)
            }
            None => Err(anyhow!("Job {}/{} not found", namespace, name)),
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if namespace.is_some() {
            r#"
                SELECT uid, namespace, name, parallelism, completions, active_deadline_seconds, backoff_limit,
                       selector, manual_selector, template, ttl_seconds_after_finished,
                       completion_mode, suspend, conditions, start_time, completion_time,
                       active, succeeded, failed, completed_indexes, uncounted_terminated_pods, ready,
                       labels, annotations, resource_version, generation, creation_timestamp 
                FROM jobs 
                WHERE namespace = ?1 AND deletion_timestamp IS NULL 
                ORDER BY name
            "#
        } else {
            r#"
                SELECT uid, namespace, name, parallelism, completions, active_deadline_seconds, backoff_limit,
                       selector, manual_selector, template, ttl_seconds_after_finished,
                       completion_mode, suspend, conditions, start_time, completion_time,
                       active, succeeded, failed, completed_indexes, uncounted_terminated_pods, ready,
                       labels, annotations, resource_version, generation, creation_timestamp 
                FROM jobs 
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
            let job = self.row_to_job(row, &ns, &name)?;
            items.push(job);
        }

        Ok(json!({
            "apiVersion": "batch/v1",
            "kind": "JobList",
            "metadata": {
                "selfLink": if let Some(ns) = namespace {
                    format!("/apis/batch/v1/namespaces/{}/jobs", ns)
                } else {
                    "/apis/batch/v1/jobs".to_string()
                },
                "resourceVersion": "1",
            },
            "items": items,
        }))
    }

    pub async fn update_status(&self, namespace: &str, name: &str, status: Value) -> Result<()> {
        let update_query = r#"
            UPDATE jobs 
            SET conditions = ?1, start_time = ?2, completion_time = ?3,
                active = ?4, succeeded = ?5, failed = ?6,
                completed_indexes = ?7, uncounted_terminated_pods = ?8, ready = ?9,
                resource_version = resource_version + 1
            WHERE namespace = ?10 AND name = ?11 AND deletion_timestamp IS NULL
        "#;

        sqlx::query(update_query)
            .bind(status.get("conditions").map(|v| v.to_string()))
            .bind(status.get("startTime").and_then(|v| v.as_str()))
            .bind(status.get("completionTime").and_then(|v| v.as_str()))
            .bind(status.get("active").and_then(|v| v.as_i64()).unwrap_or(0))
            .bind(status.get("succeeded").and_then(|v| v.as_i64()).unwrap_or(0))
            .bind(status.get("failed").and_then(|v| v.as_i64()).unwrap_or(0))
            .bind(status.get("completedIndexes").map(|v| v.to_string()))
            .bind(status.get("uncountedTerminatedPods").map(|v| v.to_string()))
            .bind(status.get("ready").and_then(|v| v.as_i64()).unwrap_or(0))
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        // Get the Job before deletion
        let job = self.get(namespace, name).await?;

        // Soft delete
        let deletion_timestamp = Utc::now().to_rfc3339();
        let delete_query = "UPDATE jobs SET deletion_timestamp = ?1 WHERE namespace = ?2 AND name = ?3";
        
        sqlx::query(delete_query)
            .bind(&deletion_timestamp)
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(job)
    }

    fn row_to_job(&self, row: sqlx::sqlite::SqliteRow, namespace: &str, name: &str) -> Result<Value> {
        let uid: String = row.get("uid");
        let parallelism: i64 = row.get("parallelism");
        let completions: Option<i64> = row.get("completions");
        let active_deadline_seconds: Option<i64> = row.get("active_deadline_seconds");
        let backoff_limit: i64 = row.get("backoff_limit");
        let selector_str: String = row.get("selector");
        let manual_selector: bool = row.get("manual_selector");
        let template_str: String = row.get("template");
        let ttl_seconds_after_finished: Option<i64> = row.get("ttl_seconds_after_finished");
        let completion_mode: String = row.get("completion_mode");
        let suspend: bool = row.get("suspend");
        
        let conditions_str: Option<String> = row.get("conditions");
        let start_time: Option<String> = row.get("start_time");
        let completion_time: Option<String> = row.get("completion_time");
        let active: i64 = row.get("active");
        let succeeded: i64 = row.get("succeeded");
        let failed: i64 = row.get("failed");
        let completed_indexes: Option<String> = row.get("completed_indexes");
        let uncounted_terminated_pods_str: Option<String> = row.get("uncounted_terminated_pods");
        let ready: i64 = row.get("ready");
        
        let labels_str: String = row.get("labels");
        let annotations_str: String = row.get("annotations");
        let resource_version: i64 = row.get("resource_version");
        let generation: i64 = row.get("generation");
        let creation_timestamp: String = row.get("creation_timestamp");

        let selector: Value = serde_json::from_str(&selector_str)?;
        let template: Value = serde_json::from_str(&template_str)?;
        let conditions: Option<Value> = conditions_str.map(|s| serde_json::from_str(&s)).transpose()?;
        let uncounted_terminated_pods: Option<Value> = uncounted_terminated_pods_str.map(|s| serde_json::from_str(&s)).transpose()?;
        let labels: Value = serde_json::from_str(&labels_str)?;
        let annotations: Value = serde_json::from_str(&annotations_str)?;

        let mut job = json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": {
                "name": name,
                "namespace": namespace,
                "uid": uid,
                "resourceVersion": resource_version.to_string(),
                "generation": generation,
                "creationTimestamp": creation_timestamp,
                "selfLink": format!("/apis/batch/v1/namespaces/{}/jobs/{}", namespace, name),
            },
            "spec": {
                "parallelism": parallelism,
                "backoffLimit": backoff_limit,
                "selector": selector,
                "manualSelector": manual_selector,
                "template": template,
                "completionMode": completion_mode,
                "suspend": suspend
            },
            "status": {
                "active": active,
                "succeeded": succeeded,
                "failed": failed,
                "ready": ready
            }
        });
        
        // Add optional spec fields
        if let Some(c) = completions {
            job["spec"]["completions"] = json!(c);
        }
        if let Some(ads) = active_deadline_seconds {
            job["spec"]["activeDeadlineSeconds"] = json!(ads);
        }
        if let Some(ttl) = ttl_seconds_after_finished {
            job["spec"]["ttlSecondsAfterFinished"] = json!(ttl);
        }
        
        // Add status fields
        if let Some(cond) = conditions {
            job["status"]["conditions"] = cond;
        }
        if let Some(st) = start_time {
            job["status"]["startTime"] = json!(st);
        }
        if let Some(ct) = completion_time {
            job["status"]["completionTime"] = json!(ct);
        }
        if let Some(ci) = completed_indexes {
            job["status"]["completedIndexes"] = json!(ci);
        }
        if let Some(utp) = uncounted_terminated_pods {
            job["status"]["uncountedTerminatedPods"] = utp;
        }

        if !labels.is_null() && labels != json!({}) {
            job["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            job["metadata"]["annotations"] = annotations;
        }

        Ok(job)
    }
}