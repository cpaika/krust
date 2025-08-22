use anyhow::Result;
use chrono::{Duration, Utc};
use futures::Stream;
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use std::pin::Pin;
use tokio_stream::StreamExt;
use uuid::Uuid;

pub struct WatchStore {
    pool: SqlitePool,
}

impl WatchStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_watch_cursor(&self, resource_type: &str) -> Result<String> {
        let cursor_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = now + Duration::minutes(10);
        
        // Get the latest event ID for this resource type
        let last_event_id = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(id), 0) FROM events WHERE resource_type = ?"
        )
        .bind(resource_type)
        .fetch_one(&self.pool)
        .await?;
        
        sqlx::query(
            "INSERT INTO watch_cursors (id, resource_type, last_event_id, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&cursor_id)
        .bind(resource_type)
        .bind(last_event_id)
        .bind(now.to_rfc3339())
        .bind(expires_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        
        Ok(cursor_id)
    }

    pub async fn get_events_since(
        &self,
        resource_type: &str,
        resource_version: Option<String>,
    ) -> Result<Vec<Value>> {
        let since_id = if let Some(rv) = resource_version {
            rv.parse::<i64>().unwrap_or(0)
        } else {
            0
        };
        
        let rows = sqlx::query(
            "SELECT event_type, object FROM events 
             WHERE resource_type = ? AND id > ?
             ORDER BY id ASC
             LIMIT 100"
        )
        .bind(resource_type)
        .bind(since_id)
        .fetch_all(&self.pool)
        .await?;
        
        let mut events = Vec::new();
        for row in rows {
            let event_type: String = row.get("event_type");
            let object_str: String = row.get("object");
            let object: Value = serde_json::from_str(&object_str)?;
            
            events.push(serde_json::json!({
                "type": event_type,
                "object": object
            }));
        }
        
        Ok(events)
    }

    pub async fn watch_stream(
        &self,
        resource_type: String,
        namespace: Option<String>,
        resource_version: Option<String>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Value>> + Send>>> {
        let pool = self.pool.clone();
        let mut last_id = if let Some(rv) = resource_version {
            rv.parse::<i64>().unwrap_or(0)
        } else {
            0
        };
        
        let stream = async_stream::stream! {
            loop {
                let query = if let Some(ref ns) = namespace {
                    sqlx::query(
                        "SELECT id, event_type, object FROM events 
                         WHERE resource_type = ? AND resource_namespace = ? AND id > ?
                         ORDER BY id ASC
                         LIMIT 10"
                    )
                    .bind(&resource_type)
                    .bind(ns)
                    .bind(last_id)
                } else {
                    sqlx::query(
                        "SELECT id, event_type, object FROM events 
                         WHERE resource_type = ? AND id > ?
                         ORDER BY id ASC
                         LIMIT 10"
                    )
                    .bind(&resource_type)
                    .bind(last_id)
                };
                
                match query.fetch_all(&pool).await {
                    Ok(rows) => {
                        for row in rows {
                            let id: i64 = row.get("id");
                            let event_type: String = row.get("event_type");
                            let object_str: String = row.get("object");
                            
                            if let Ok(mut object) = serde_json::from_str::<Value>(&object_str) {
                                // Update resource version to the event ID
                                if let Some(metadata) = object.get_mut("metadata") {
                                    metadata["resourceVersion"] = serde_json::json!(id.to_string());
                                }
                                
                                let event = serde_json::json!({
                                    "type": event_type,
                                    "object": object
                                });
                                
                                last_id = id;
                                yield Ok(event);
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(anyhow::anyhow!("Database error: {}", e));
                        break;
                    }
                }
                
                // Wait a bit before checking for new events
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        };
        
        Ok(Box::pin(stream))
    }

    pub async fn cleanup_expired_cursors(&self) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        
        sqlx::query("DELETE FROM watch_cursors WHERE expires_at < ?")
            .bind(now)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}