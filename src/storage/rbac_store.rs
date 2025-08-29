use anyhow::Result;
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

// Role Store
pub struct RoleStore {
    pool: SqlitePool,
}

impl RoleStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut role: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        
        // Set metadata fields
        role["metadata"]["uid"] = json!(uid);
        role["metadata"]["namespace"] = json!(namespace);
        role["metadata"]["creationTimestamp"] = json!(now);
        role["metadata"]["resourceVersion"] = json!("1");
        
        let name = role["metadata"]["name"].as_str().unwrap();
        let rules = role["rules"].to_string();
        let labels = role["metadata"]["labels"].to_string();
        let annotations = role["metadata"]["annotations"].to_string();
        
        sqlx::query(
            "INSERT INTO roles (uid, name, namespace, rules, labels, annotations, resource_version, creation_timestamp)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&uid)
        .bind(name)
        .bind(namespace)
        .bind(rules)
        .bind(labels)
        .bind(annotations)
        .bind(1i64)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        
        Ok(role)
    }
    
    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let row = sqlx::query(
            "SELECT * FROM roles WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(namespace)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        
        match row {
            Some(row) => {
                let mut role = json!({
                    "apiVersion": "rbac.authorization.k8s.io/v1",
                    "kind": "Role",
                    "metadata": {
                        "name": row.get::<String, _>("name"),
                        "namespace": row.get::<String, _>("namespace"),
                        "uid": row.get::<String, _>("uid"),
                        "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                        "creationTimestamp": row.get::<String, _>("creation_timestamp")
                    },
                    "rules": serde_json::from_str::<Value>(&row.get::<String, _>("rules"))?
                });
                
                if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                    if !labels.is_null() {
                        role["metadata"]["labels"] = labels;
                    }
                }
                
                if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                    if !annotations.is_null() {
                        role["metadata"]["annotations"] = annotations;
                    }
                }
                
                Ok(role)
            }
            None => anyhow::bail!("Role {}/{} not found", namespace, name)
        }
    }
    
    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let rows = if let Some(ns) = namespace {
            sqlx::query(
                "SELECT * FROM roles WHERE namespace = ? AND deletion_timestamp IS NULL ORDER BY creation_timestamp DESC"
            )
            .bind(ns)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT * FROM roles WHERE deletion_timestamp IS NULL ORDER BY creation_timestamp DESC"
            )
            .fetch_all(&self.pool)
            .await?
        };
        
        let mut items = Vec::new();
        for row in rows {
            let mut role = json!({
                "apiVersion": "rbac.authorization.k8s.io/v1",
                "kind": "Role",
                "metadata": {
                    "name": row.get::<String, _>("name"),
                    "namespace": row.get::<String, _>("namespace"),
                    "uid": row.get::<String, _>("uid"),
                    "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                    "creationTimestamp": row.get::<String, _>("creation_timestamp")
                },
                "rules": serde_json::from_str::<Value>(&row.get::<String, _>("rules"))?
            });
            
            if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                if !labels.is_null() {
                    role["metadata"]["labels"] = labels;
                }
            }
            
            if let Ok(annotations) = serde_json::from_str::<Value>(&row.get::<String, _>("annotations")) {
                if !annotations.is_null() {
                    role["metadata"]["annotations"] = annotations;
                }
            }
            
            items.push(role);
        }
        
        Ok(json!({
            "apiVersion": "rbac.authorization.k8s.io/v1",
            "kind": "RoleList",
            "metadata": {
                "resourceVersion": "1"
            },
            "items": items
        }))
    }
    
    pub async fn update(&self, namespace: &str, name: &str, mut role: Value) -> Result<Value> {
        let current = self.get(namespace, name).await?;
        let uid = current["metadata"]["uid"].as_str().unwrap();
        let current_version: i64 = current["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse()?;
        
        let new_version = current_version + 1;
        
        // Update metadata
        role["metadata"]["resourceVersion"] = json!(new_version.to_string());
        role["metadata"]["uid"] = json!(uid);
        
        let rules = role["rules"].to_string();
        let labels = role["metadata"]["labels"].to_string();
        let annotations = role["metadata"]["annotations"].to_string();
        
        sqlx::query(
            "UPDATE roles SET rules = ?, labels = ?, annotations = ?, resource_version = ?
             WHERE uid = ?"
        )
        .bind(rules)
        .bind(labels)
        .bind(annotations)
        .bind(new_version)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
        Ok(role)
    }
    
    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        let mut role = self.get(namespace, name).await?;
        let uid = role["metadata"]["uid"].as_str().unwrap().to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Set deletion timestamp in the object
        role["metadata"]["deletionTimestamp"] = json!(now.clone());
        
        sqlx::query(
            "UPDATE roles SET deletion_timestamp = ? WHERE uid = ?"
        )
        .bind(&now)
        .bind(&uid)
        .execute(&self.pool)
        .await?;
        
        Ok(role)
    }
}

// RoleBinding Store
pub struct RoleBindingStore {
    pool: SqlitePool,
}

impl RoleBindingStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut rolebinding: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        
        rolebinding["metadata"]["uid"] = json!(uid);
        rolebinding["metadata"]["namespace"] = json!(namespace);
        rolebinding["metadata"]["creationTimestamp"] = json!(now);
        rolebinding["metadata"]["resourceVersion"] = json!("1");
        
        let name = rolebinding["metadata"]["name"].as_str().unwrap();
        let subjects = rolebinding["subjects"].to_string();
        let role_ref = rolebinding["roleRef"].to_string();
        let labels = rolebinding["metadata"]["labels"].to_string();
        let annotations = rolebinding["metadata"]["annotations"].to_string();
        
        sqlx::query(
            "INSERT INTO rolebindings (uid, name, namespace, subjects, role_ref, labels, annotations, resource_version, creation_timestamp)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&uid)
        .bind(name)
        .bind(namespace)
        .bind(subjects)
        .bind(role_ref)
        .bind(labels)
        .bind(annotations)
        .bind(1i64)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        
        Ok(rolebinding)
    }
    
    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let row = sqlx::query(
            "SELECT * FROM rolebindings WHERE namespace = ? AND name = ? AND deletion_timestamp IS NULL"
        )
        .bind(namespace)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        
        match row {
            Some(row) => {
                let mut rolebinding = json!({
                    "apiVersion": "rbac.authorization.k8s.io/v1",
                    "kind": "RoleBinding",
                    "metadata": {
                        "name": row.get::<String, _>("name"),
                        "namespace": row.get::<String, _>("namespace"),
                        "uid": row.get::<String, _>("uid"),
                        "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                        "creationTimestamp": row.get::<String, _>("creation_timestamp")
                    },
                    "subjects": serde_json::from_str::<Value>(&row.get::<String, _>("subjects"))?,
                    "roleRef": serde_json::from_str::<Value>(&row.get::<String, _>("role_ref"))?
                });
                
                if let Ok(labels) = serde_json::from_str::<Value>(&row.get::<String, _>("labels")) {
                    if !labels.is_null() {
                        rolebinding["metadata"]["labels"] = labels;
                    }
                }
                
                Ok(rolebinding)
            }
            None => anyhow::bail!("RoleBinding {}/{} not found", namespace, name)
        }
    }
    
    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let rows = if let Some(ns) = namespace {
            sqlx::query(
                "SELECT * FROM rolebindings WHERE namespace = ? AND deletion_timestamp IS NULL"
            )
            .bind(ns)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT * FROM rolebindings WHERE deletion_timestamp IS NULL"
            )
            .fetch_all(&self.pool)
            .await?
        };
        
        let mut items = Vec::new();
        for row in rows {
            let mut rolebinding = json!({
                "apiVersion": "rbac.authorization.k8s.io/v1",
                "kind": "RoleBinding",
                "metadata": {
                    "name": row.get::<String, _>("name"),
                    "namespace": row.get::<String, _>("namespace"),
                    "uid": row.get::<String, _>("uid"),
                    "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                    "creationTimestamp": row.get::<String, _>("creation_timestamp")
                },
                "subjects": serde_json::from_str::<Value>(&row.get::<String, _>("subjects"))?,
                "roleRef": serde_json::from_str::<Value>(&row.get::<String, _>("role_ref"))?
            });
            
            items.push(rolebinding);
        }
        
        Ok(json!({
            "apiVersion": "rbac.authorization.k8s.io/v1",
            "kind": "RoleBindingList",
            "metadata": {
                "resourceVersion": "1"
            },
            "items": items
        }))
    }
    
    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        let mut rolebinding = self.get(namespace, name).await?;
        let uid = rolebinding["metadata"]["uid"].as_str().unwrap().to_string();
        
        let now = Utc::now().to_rfc3339();
        rolebinding["metadata"]["deletionTimestamp"] = json!(now.clone());
        
        sqlx::query(
            "UPDATE rolebindings SET deletion_timestamp = ? WHERE uid = ?"
        )
        .bind(&now)
        .bind(&uid)
        .execute(&self.pool)
        .await?;
        
        Ok(rolebinding)
    }
}

// ClusterRole Store
pub struct ClusterRoleStore {
    pool: SqlitePool,
}

impl ClusterRoleStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, mut clusterrole: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        
        clusterrole["metadata"]["uid"] = json!(uid);
        clusterrole["metadata"]["creationTimestamp"] = json!(now);
        clusterrole["metadata"]["resourceVersion"] = json!("1");
        
        let name = clusterrole["metadata"]["name"].as_str().unwrap();
        let rules = clusterrole["rules"].to_string();
        let aggregation_rule = clusterrole.get("aggregationRule").map(|a| a.to_string());
        let labels = clusterrole["metadata"]["labels"].to_string();
        let annotations = clusterrole["metadata"]["annotations"].to_string();
        
        sqlx::query(
            "INSERT INTO clusterroles (uid, name, rules, aggregation_rule, labels, annotations, resource_version, creation_timestamp)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&uid)
        .bind(name)
        .bind(rules)
        .bind(aggregation_rule)
        .bind(labels)
        .bind(annotations)
        .bind(1i64)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        
        Ok(clusterrole)
    }
    
    pub async fn get(&self, name: &str) -> Result<Value> {
        let row = sqlx::query(
            "SELECT * FROM clusterroles WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        
        match row {
            Some(row) => {
                let mut clusterrole = json!({
                    "apiVersion": "rbac.authorization.k8s.io/v1",
                    "kind": "ClusterRole",
                    "metadata": {
                        "name": row.get::<String, _>("name"),
                        "uid": row.get::<String, _>("uid"),
                        "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                        "creationTimestamp": row.get::<String, _>("creation_timestamp")
                    },
                    "rules": serde_json::from_str::<Value>(&row.get::<String, _>("rules"))?
                });
                
                if let Some(agg_rule) = row.get::<Option<String>, _>("aggregation_rule") {
                    if let Ok(rule) = serde_json::from_str::<Value>(&agg_rule) {
                        clusterrole["aggregationRule"] = rule;
                    }
                }
                
                Ok(clusterrole)
            }
            None => anyhow::bail!("ClusterRole {} not found", name)
        }
    }
    
    pub async fn list(&self) -> Result<Value> {
        let rows = sqlx::query(
            "SELECT * FROM clusterroles WHERE deletion_timestamp IS NULL"
        )
        .fetch_all(&self.pool)
        .await?;
        
        let mut items = Vec::new();
        for row in rows {
            let mut clusterrole = json!({
                "apiVersion": "rbac.authorization.k8s.io/v1",
                "kind": "ClusterRole",
                "metadata": {
                    "name": row.get::<String, _>("name"),
                    "uid": row.get::<String, _>("uid"),
                    "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                    "creationTimestamp": row.get::<String, _>("creation_timestamp")
                },
                "rules": serde_json::from_str::<Value>(&row.get::<String, _>("rules"))?
            });
            
            items.push(clusterrole);
        }
        
        Ok(json!({
            "apiVersion": "rbac.authorization.k8s.io/v1",
            "kind": "ClusterRoleList",
            "metadata": {
                "resourceVersion": "1"
            },
            "items": items
        }))
    }
    
    pub async fn update(&self, name: &str, mut clusterrole: Value) -> Result<Value> {
        let current = self.get(name).await?;
        let uid = current["metadata"]["uid"].as_str().unwrap();
        let current_version: i64 = current["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse()?;
        
        let new_version = current_version + 1;
        clusterrole["metadata"]["resourceVersion"] = json!(new_version.to_string());
        clusterrole["metadata"]["uid"] = json!(uid);
        
        let rules = clusterrole["rules"].to_string();
        
        sqlx::query(
            "UPDATE clusterroles SET rules = ?, resource_version = ? WHERE uid = ?"
        )
        .bind(rules)
        .bind(new_version)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        
        Ok(clusterrole)
    }
    
    pub async fn delete(&self, name: &str) -> Result<Value> {
        let mut clusterrole = self.get(name).await?;
        let uid = clusterrole["metadata"]["uid"].as_str().unwrap().to_string();
        
        let now = Utc::now().to_rfc3339();
        clusterrole["metadata"]["deletionTimestamp"] = json!(now.clone());
        
        sqlx::query(
            "UPDATE clusterroles SET deletion_timestamp = ? WHERE uid = ?"
        )
        .bind(&now)
        .bind(&uid)
        .execute(&self.pool)
        .await?;
        
        Ok(clusterrole)
    }
}

// ClusterRoleBinding Store
pub struct ClusterRoleBindingStore {
    pool: SqlitePool,
}

impl ClusterRoleBindingStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, mut clusterrolebinding: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        
        clusterrolebinding["metadata"]["uid"] = json!(uid);
        clusterrolebinding["metadata"]["creationTimestamp"] = json!(now);
        clusterrolebinding["metadata"]["resourceVersion"] = json!("1");
        
        let name = clusterrolebinding["metadata"]["name"].as_str().unwrap();
        let subjects = clusterrolebinding["subjects"].to_string();
        let role_ref = clusterrolebinding["roleRef"].to_string();
        
        sqlx::query(
            "INSERT INTO clusterrolebindings (uid, name, subjects, role_ref, labels, annotations, resource_version, creation_timestamp)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&uid)
        .bind(name)
        .bind(subjects)
        .bind(role_ref)
        .bind(clusterrolebinding["metadata"]["labels"].to_string())
        .bind(clusterrolebinding["metadata"]["annotations"].to_string())
        .bind(1i64)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        
        Ok(clusterrolebinding)
    }
    
    pub async fn get(&self, name: &str) -> Result<Value> {
        let row = sqlx::query(
            "SELECT * FROM clusterrolebindings WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        
        match row {
            Some(row) => {
                let clusterrolebinding = json!({
                    "apiVersion": "rbac.authorization.k8s.io/v1",
                    "kind": "ClusterRoleBinding",
                    "metadata": {
                        "name": row.get::<String, _>("name"),
                        "uid": row.get::<String, _>("uid"),
                        "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                        "creationTimestamp": row.get::<String, _>("creation_timestamp")
                    },
                    "subjects": serde_json::from_str::<Value>(&row.get::<String, _>("subjects"))?,
                    "roleRef": serde_json::from_str::<Value>(&row.get::<String, _>("role_ref"))?
                });
                
                Ok(clusterrolebinding)
            }
            None => anyhow::bail!("ClusterRoleBinding {} not found", name)
        }
    }
    
    pub async fn list(&self) -> Result<Value> {
        let rows = sqlx::query(
            "SELECT * FROM clusterrolebindings WHERE deletion_timestamp IS NULL"
        )
        .fetch_all(&self.pool)
        .await?;
        
        let mut items = Vec::new();
        for row in rows {
            let clusterrolebinding = json!({
                "apiVersion": "rbac.authorization.k8s.io/v1",
                "kind": "ClusterRoleBinding",
                "metadata": {
                    "name": row.get::<String, _>("name"),
                    "uid": row.get::<String, _>("uid"),
                    "resourceVersion": row.get::<i64, _>("resource_version").to_string(),
                    "creationTimestamp": row.get::<String, _>("creation_timestamp")
                },
                "subjects": serde_json::from_str::<Value>(&row.get::<String, _>("subjects"))?,
                "roleRef": serde_json::from_str::<Value>(&row.get::<String, _>("role_ref"))?
            });
            
            items.push(clusterrolebinding);
        }
        
        Ok(json!({
            "apiVersion": "rbac.authorization.k8s.io/v1",
            "kind": "ClusterRoleBindingList",
            "metadata": {
                "resourceVersion": "1"
            },
            "items": items
        }))
    }
    
    pub async fn delete(&self, name: &str) -> Result<Value> {
        let mut clusterrolebinding = self.get(name).await?;
        let uid = clusterrolebinding["metadata"]["uid"].as_str().unwrap().to_string();
        
        let now = Utc::now().to_rfc3339();
        clusterrolebinding["metadata"]["deletionTimestamp"] = json!(now.clone());
        
        sqlx::query(
            "UPDATE clusterrolebindings SET deletion_timestamp = ? WHERE uid = ?"
        )
        .bind(&now)
        .bind(&uid)
        .execute(&self.pool)
        .await?;
        
        Ok(clusterrolebinding)
    }
}
