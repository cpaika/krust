use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct NetworkPolicyStore {
    pool: SqlitePool,
}

impl NetworkPolicyStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut policy: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = policy["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("NetworkPolicy name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Extract spec fields
        let pod_selector = policy["spec"]["podSelector"].clone();
        if pod_selector.is_null() {
            return Err(anyhow!("Pod selector is required"));
        }
        
        let policy_types = policy["spec"].get("policyTypes")
            .cloned()
            .unwrap_or_else(|| {
                // Default policy types based on presence of ingress/egress rules
                let mut types = Vec::new();
                if policy["spec"].get("ingress").is_some() {
                    types.push(json!("Ingress"));
                }
                if policy["spec"].get("egress").is_some() {
                    types.push(json!("Egress"));
                }
                if types.is_empty() {
                    types.push(json!("Ingress")); // Default to Ingress if no rules specified
                }
                json!(types)
            });
        
        let ingress = policy["spec"].get("ingress").cloned();
        let egress = policy["spec"].get("egress").cloned();
        
        let labels = policy["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = policy["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        // Insert into database
        let query = r#"
            INSERT INTO networkpolicies (
                uid, namespace, name, pod_selector, policy_types, ingress, egress,
                labels, annotations, resource_version, creation_timestamp
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10)
        "#;
        
        sqlx::query(query)
            .bind(&uid)
            .bind(namespace)
            .bind(&name)
            .bind(pod_selector.to_string())
            .bind(policy_types.to_string())
            .bind(ingress.as_ref().map(|v| v.to_string()))
            .bind(egress.as_ref().map(|v| v.to_string()))
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(&now)
            .execute(&self.pool)
            .await?;

        // Build response
        policy["apiVersion"] = json!("networking.k8s.io/v1");
        policy["kind"] = json!("NetworkPolicy");
        policy["metadata"]["uid"] = json!(uid);
        policy["metadata"]["namespace"] = json!(namespace);
        policy["metadata"]["resourceVersion"] = json!("1");
        policy["metadata"]["generation"] = json!(1);
        policy["metadata"]["creationTimestamp"] = json!(now);
        policy["metadata"]["selfLink"] = json!(format!("/apis/networking.k8s.io/v1/namespaces/{}/networkpolicies/{}", namespace, name));
        
        // Set spec fields
        policy["spec"]["policyTypes"] = policy_types;
        
        if !labels.is_null() && labels != json!({}) {
            policy["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            policy["metadata"]["annotations"] = annotations;
        }

        Ok(policy)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let query = r#"
            SELECT uid, pod_selector, policy_types, ingress, egress,
                   labels, annotations, resource_version, generation, creation_timestamp 
            FROM networkpolicies 
            WHERE namespace = ?1 AND name = ?2 AND deletion_timestamp IS NULL
        "#;
        
        let row = sqlx::query(query)
            .bind(namespace)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => {
                let policy = self.row_to_networkpolicy(row, namespace, name)?;
                Ok(policy)
            }
            None => Err(anyhow!("NetworkPolicy {}/{} not found", namespace, name)),
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if namespace.is_some() {
            r#"
                SELECT uid, namespace, name, pod_selector, policy_types, ingress, egress,
                       labels, annotations, resource_version, generation, creation_timestamp 
                FROM networkpolicies 
                WHERE namespace = ?1 AND deletion_timestamp IS NULL 
                ORDER BY name
            "#
        } else {
            r#"
                SELECT uid, namespace, name, pod_selector, policy_types, ingress, egress,
                       labels, annotations, resource_version, generation, creation_timestamp 
                FROM networkpolicies 
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
            let policy = self.row_to_networkpolicy(row, &ns, &name)?;
            items.push(policy);
        }

        Ok(json!({
            "apiVersion": "networking.k8s.io/v1",
            "kind": "NetworkPolicyList",
            "metadata": {
                "selfLink": if let Some(ns) = namespace {
                    format!("/apis/networking.k8s.io/v1/namespaces/{}/networkpolicies", ns)
                } else {
                    "/apis/networking.k8s.io/v1/networkpolicies".to_string()
                },
                "resourceVersion": "1",
            },
            "items": items,
        }))
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        // Get the NetworkPolicy before deletion
        let policy = self.get(namespace, name).await?;

        // Soft delete
        let deletion_timestamp = Utc::now().to_rfc3339();
        let delete_query = "UPDATE networkpolicies SET deletion_timestamp = ?1 WHERE namespace = ?2 AND name = ?3";
        
        sqlx::query(delete_query)
            .bind(&deletion_timestamp)
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(policy)
    }

    fn row_to_networkpolicy(&self, row: sqlx::sqlite::SqliteRow, namespace: &str, name: &str) -> Result<Value> {
        let uid: String = row.get("uid");
        let pod_selector_str: String = row.get("pod_selector");
        let policy_types_str: String = row.get("policy_types");
        let ingress_str: Option<String> = row.get("ingress");
        let egress_str: Option<String> = row.get("egress");
        
        let labels_str: String = row.get("labels");
        let annotations_str: String = row.get("annotations");
        let resource_version: i64 = row.get("resource_version");
        let generation: i64 = row.get("generation");
        let creation_timestamp: String = row.get("creation_timestamp");

        let pod_selector: Value = serde_json::from_str(&pod_selector_str)?;
        let policy_types: Value = serde_json::from_str(&policy_types_str)?;
        let ingress: Option<Value> = ingress_str.map(|s| serde_json::from_str(&s)).transpose()?;
        let egress: Option<Value> = egress_str.map(|s| serde_json::from_str(&s)).transpose()?;
        let labels: Value = serde_json::from_str(&labels_str)?;
        let annotations: Value = serde_json::from_str(&annotations_str)?;

        let mut policy = json!({
            "apiVersion": "networking.k8s.io/v1",
            "kind": "NetworkPolicy",
            "metadata": {
                "name": name,
                "namespace": namespace,
                "uid": uid,
                "resourceVersion": resource_version.to_string(),
                "generation": generation,
                "creationTimestamp": creation_timestamp,
                "selfLink": format!("/apis/networking.k8s.io/v1/namespaces/{}/networkpolicies/{}", namespace, name),
            },
            "spec": {
                "podSelector": pod_selector,
                "policyTypes": policy_types
            }
        });
        
        // Add optional spec fields
        if let Some(ing) = ingress {
            policy["spec"]["ingress"] = ing;
        }
        if let Some(eg) = egress {
            policy["spec"]["egress"] = eg;
        }

        if !labels.is_null() && labels != json!({}) {
            policy["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            policy["metadata"]["annotations"] = annotations;
        }

        Ok(policy)
    }
}