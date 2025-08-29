use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct CronJobStore {
    pool: SqlitePool,
}

impl CronJobStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut cronjob: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = cronjob["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("CronJob name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Extract spec fields
        let schedule = cronjob["spec"]["schedule"]
            .as_str()
            .ok_or_else(|| anyhow!("Schedule is required"))?
            .to_string();
        
        let timezone = cronjob["spec"].get("timeZone")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let starting_deadline_seconds = cronjob["spec"].get("startingDeadlineSeconds")
            .and_then(|v| v.as_i64());
        
        let concurrency_policy = cronjob["spec"].get("concurrencyPolicy")
            .and_then(|v| v.as_str())
            .unwrap_or("Allow")
            .to_string();
        
        let suspend = cronjob["spec"].get("suspend")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        let job_template = cronjob["spec"]["jobTemplate"].clone();
        if job_template.is_null() {
            return Err(anyhow!("Job template is required"));
        }
        
        let successful_jobs_history_limit = cronjob["spec"].get("successfulJobsHistoryLimit")
            .and_then(|v| v.as_i64())
            .unwrap_or(3);
        
        let failed_jobs_history_limit = cronjob["spec"].get("failedJobsHistoryLimit")
            .and_then(|v| v.as_i64())
            .unwrap_or(1);
        
        let labels = cronjob["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = cronjob["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        // Insert into database
        let query = r#"
            INSERT INTO cronjobs (
                uid, namespace, name, schedule, timezone, starting_deadline_seconds,
                concurrency_policy, suspend, job_template, successful_jobs_history_limit,
                failed_jobs_history_limit, active, labels, annotations, 
                resource_version, creation_timestamp
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 1, ?15)
        "#;
        
        sqlx::query(query)
            .bind(&uid)
            .bind(namespace)
            .bind(&name)
            .bind(&schedule)
            .bind(timezone.clone())
            .bind(starting_deadline_seconds)
            .bind(&concurrency_policy)
            .bind(suspend)
            .bind(job_template.to_string())
            .bind(successful_jobs_history_limit)
            .bind(failed_jobs_history_limit)
            .bind(json!([]).to_string()) // active jobs
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(&now)
            .execute(&self.pool)
            .await?;

        // Build response
        cronjob["apiVersion"] = json!("batch/v1");
        cronjob["kind"] = json!("CronJob");
        cronjob["metadata"]["uid"] = json!(uid);
        cronjob["metadata"]["namespace"] = json!(namespace);
        cronjob["metadata"]["resourceVersion"] = json!("1");
        cronjob["metadata"]["generation"] = json!(1);
        cronjob["metadata"]["creationTimestamp"] = json!(now);
        cronjob["metadata"]["selfLink"] = json!(format!("/apis/batch/v1/namespaces/{}/cronjobs/{}", namespace, name));
        
        // Set spec defaults
        cronjob["spec"]["concurrencyPolicy"] = json!(concurrency_policy);
        cronjob["spec"]["suspend"] = json!(suspend);
        cronjob["spec"]["successfulJobsHistoryLimit"] = json!(successful_jobs_history_limit);
        cronjob["spec"]["failedJobsHistoryLimit"] = json!(failed_jobs_history_limit);
        
        if let Some(tz) = timezone {
            cronjob["spec"]["timeZone"] = json!(tz);
        }
        
        // Set initial status
        cronjob["status"] = json!({
            "active": []
        });
        
        if !labels.is_null() && labels != json!({}) {
            cronjob["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            cronjob["metadata"]["annotations"] = annotations;
        }

        Ok(cronjob)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let query = r#"
            SELECT uid, schedule, timezone, starting_deadline_seconds, concurrency_policy,
                   suspend, job_template, successful_jobs_history_limit, failed_jobs_history_limit,
                   active, last_schedule_time, last_successful_time,
                   labels, annotations, resource_version, generation, creation_timestamp 
            FROM cronjobs 
            WHERE namespace = ?1 AND name = ?2 AND deletion_timestamp IS NULL
        "#;
        
        let row = sqlx::query(query)
            .bind(namespace)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => {
                let cronjob = self.row_to_cronjob(row, namespace, name)?;
                Ok(cronjob)
            }
            None => Err(anyhow!("CronJob {}/{} not found", namespace, name)),
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if namespace.is_some() {
            r#"
                SELECT uid, namespace, name, schedule, timezone, starting_deadline_seconds, concurrency_policy,
                       suspend, job_template, successful_jobs_history_limit, failed_jobs_history_limit,
                       active, last_schedule_time, last_successful_time,
                       labels, annotations, resource_version, generation, creation_timestamp 
                FROM cronjobs 
                WHERE namespace = ?1 AND deletion_timestamp IS NULL 
                ORDER BY name
            "#
        } else {
            r#"
                SELECT uid, namespace, name, schedule, timezone, starting_deadline_seconds, concurrency_policy,
                       suspend, job_template, successful_jobs_history_limit, failed_jobs_history_limit,
                       active, last_schedule_time, last_successful_time,
                       labels, annotations, resource_version, generation, creation_timestamp 
                FROM cronjobs 
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
            let cronjob = self.row_to_cronjob(row, &ns, &name)?;
            items.push(cronjob);
        }

        Ok(json!({
            "apiVersion": "batch/v1",
            "kind": "CronJobList",
            "metadata": {
                "selfLink": if let Some(ns) = namespace {
                    format!("/apis/batch/v1/namespaces/{}/cronjobs", ns)
                } else {
                    "/apis/batch/v1/cronjobs".to_string()
                },
                "resourceVersion": "1",
            },
            "items": items,
        }))
    }

    pub async fn update_status(&self, namespace: &str, name: &str, status: Value) -> Result<()> {
        let update_query = r#"
            UPDATE cronjobs 
            SET active = ?1, last_schedule_time = ?2, last_successful_time = ?3,
                resource_version = resource_version + 1
            WHERE namespace = ?4 AND name = ?5 AND deletion_timestamp IS NULL
        "#;

        sqlx::query(update_query)
            .bind(status.get("active").map(|v| v.to_string()).unwrap_or_else(|| "[]".to_string()))
            .bind(status.get("lastScheduleTime").and_then(|v| v.as_str()))
            .bind(status.get("lastSuccessfulTime").and_then(|v| v.as_str()))
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        // Get the CronJob before deletion
        let cronjob = self.get(namespace, name).await?;

        // Soft delete
        let deletion_timestamp = Utc::now().to_rfc3339();
        let delete_query = "UPDATE cronjobs SET deletion_timestamp = ?1 WHERE namespace = ?2 AND name = ?3";
        
        sqlx::query(delete_query)
            .bind(&deletion_timestamp)
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(cronjob)
    }

    fn row_to_cronjob(&self, row: sqlx::sqlite::SqliteRow, namespace: &str, name: &str) -> Result<Value> {
        let uid: String = row.get("uid");
        let schedule: String = row.get("schedule");
        let timezone: Option<String> = row.get("timezone");
        let starting_deadline_seconds: Option<i64> = row.get("starting_deadline_seconds");
        let concurrency_policy: String = row.get("concurrency_policy");
        let suspend: bool = row.get("suspend");
        let job_template_str: String = row.get("job_template");
        let successful_jobs_history_limit: i64 = row.get("successful_jobs_history_limit");
        let failed_jobs_history_limit: i64 = row.get("failed_jobs_history_limit");
        
        let active_str: Option<String> = row.get("active");
        let last_schedule_time: Option<String> = row.get("last_schedule_time");
        let last_successful_time: Option<String> = row.get("last_successful_time");
        
        let labels_str: String = row.get("labels");
        let annotations_str: String = row.get("annotations");
        let resource_version: i64 = row.get("resource_version");
        let generation: i64 = row.get("generation");
        let creation_timestamp: String = row.get("creation_timestamp");

        let job_template: Value = serde_json::from_str(&job_template_str)?;
        let active: Value = active_str
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_else(|| json!([]));
        let labels: Value = serde_json::from_str(&labels_str)?;
        let annotations: Value = serde_json::from_str(&annotations_str)?;

        let mut cronjob = json!({
            "apiVersion": "batch/v1",
            "kind": "CronJob",
            "metadata": {
                "name": name,
                "namespace": namespace,
                "uid": uid,
                "resourceVersion": resource_version.to_string(),
                "generation": generation,
                "creationTimestamp": creation_timestamp,
                "selfLink": format!("/apis/batch/v1/namespaces/{}/cronjobs/{}", namespace, name),
            },
            "spec": {
                "schedule": schedule,
                "concurrencyPolicy": concurrency_policy,
                "suspend": suspend,
                "jobTemplate": job_template,
                "successfulJobsHistoryLimit": successful_jobs_history_limit,
                "failedJobsHistoryLimit": failed_jobs_history_limit
            },
            "status": {
                "active": active
            }
        });
        
        // Add optional spec fields
        if let Some(tz) = timezone {
            cronjob["spec"]["timeZone"] = json!(tz);
        }
        if let Some(sds) = starting_deadline_seconds {
            cronjob["spec"]["startingDeadlineSeconds"] = json!(sds);
        }
        
        // Add status fields
        if let Some(lst) = last_schedule_time {
            cronjob["status"]["lastScheduleTime"] = json!(lst);
        }
        if let Some(lst) = last_successful_time {
            cronjob["status"]["lastSuccessfulTime"] = json!(lst);
        }

        if !labels.is_null() && labels != json!({}) {
            cronjob["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            cronjob["metadata"]["annotations"] = annotations;
        }

        Ok(cronjob)
    }
}