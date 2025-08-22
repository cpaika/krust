use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespace {
    pub uid: String,
    pub name: String,
    pub resource_version: i64,
    pub creation_timestamp: DateTime<Utc>,
    pub deletion_timestamp: Option<DateTime<Utc>>,
    pub labels: Option<Value>,
    pub annotations: Option<Value>,
    pub spec: Value,
    pub status: Option<Value>,
}

impl Namespace {
    pub fn new(name: String, spec: Value) -> Self {
        Self {
            uid: Uuid::new_v4().to_string(),
            name,
            resource_version: 1,
            creation_timestamp: Utc::now(),
            deletion_timestamp: None,
            labels: None,
            annotations: None,
            spec,
            status: Some(serde_json::json!({"phase": "Active"})),
        }
    }
}