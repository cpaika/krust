use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pod {
    pub uid: String,
    pub name: String,
    pub namespace: String,
    pub resource_version: i64,
    pub creation_timestamp: DateTime<Utc>,
    pub deletion_timestamp: Option<DateTime<Utc>>,
    pub labels: Option<Value>,
    pub annotations: Option<Value>,
    pub spec: Value,
    pub status: Option<Value>,
    pub node_name: Option<String>,
    pub phase: String,
}

impl Pod {
    pub fn new(name: String, namespace: String, spec: Value) -> Self {
        Self {
            uid: Uuid::new_v4().to_string(),
            name,
            namespace,
            resource_version: 1,
            creation_timestamp: Utc::now(),
            deletion_timestamp: None,
            labels: None,
            annotations: None,
            spec,
            status: None,
            node_name: None,
            phase: "Pending".to_string(),
        }
    }
}