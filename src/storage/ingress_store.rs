use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct IngressStore {
    pool: SqlitePool,
}

impl IngressStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, namespace: &str, mut ingress: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = ingress["metadata"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("Ingress name is required"))?
            .to_string();
        
        let now = Utc::now().to_rfc3339();
        
        // Extract spec fields
        let ingress_class_name = ingress["spec"].get("ingressClassName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let default_backend = ingress["spec"].get("defaultBackend").cloned();
        let rules = ingress["spec"].get("rules").cloned();
        let tls = ingress["spec"].get("tls").cloned();
        
        let labels = ingress["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = ingress["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        // Insert into database
        let query = r#"
            INSERT INTO ingresses (
                uid, namespace, name, ingress_class_name, default_backend, rules, tls,
                load_balancer, labels, annotations, resource_version, creation_timestamp
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11)
        "#;
        
        sqlx::query(query)
            .bind(&uid)
            .bind(namespace)
            .bind(&name)
            .bind(ingress_class_name.clone())
            .bind(default_backend.as_ref().map(|v| v.to_string()))
            .bind(rules.as_ref().map(|v| v.to_string()))
            .bind(tls.as_ref().map(|v| v.to_string()))
            .bind(json!({"ingress": []}).to_string()) // Initial load balancer status
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(&now)
            .execute(&self.pool)
            .await?;

        // Build response
        ingress["apiVersion"] = json!("networking.k8s.io/v1");
        ingress["kind"] = json!("Ingress");
        ingress["metadata"]["uid"] = json!(uid);
        ingress["metadata"]["namespace"] = json!(namespace);
        ingress["metadata"]["resourceVersion"] = json!("1");
        ingress["metadata"]["generation"] = json!(1);
        ingress["metadata"]["creationTimestamp"] = json!(now);
        ingress["metadata"]["selfLink"] = json!(format!("/apis/networking.k8s.io/v1/namespaces/{}/ingresses/{}", namespace, name));
        
        // Set spec defaults
        if let Some(class_name) = ingress_class_name {
            ingress["spec"]["ingressClassName"] = json!(class_name);
        }
        
        // Set initial status
        ingress["status"] = json!({
            "loadBalancer": {
                "ingress": []
            }
        });
        
        if !labels.is_null() && labels != json!({}) {
            ingress["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            ingress["metadata"]["annotations"] = annotations;
        }

        Ok(ingress)
    }

    pub async fn get(&self, namespace: &str, name: &str) -> Result<Value> {
        let query = r#"
            SELECT uid, ingress_class_name, default_backend, rules, tls, load_balancer,
                   labels, annotations, resource_version, generation, creation_timestamp 
            FROM ingresses 
            WHERE namespace = ?1 AND name = ?2 AND deletion_timestamp IS NULL
        "#;
        
        let row = sqlx::query(query)
            .bind(namespace)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => {
                let ingress = self.row_to_ingress(row, namespace, name)?;
                Ok(ingress)
            }
            None => Err(anyhow!("Ingress {}/{} not found", namespace, name)),
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> Result<Value> {
        let query = if namespace.is_some() {
            r#"
                SELECT uid, namespace, name, ingress_class_name, default_backend, rules, tls, load_balancer,
                       labels, annotations, resource_version, generation, creation_timestamp 
                FROM ingresses 
                WHERE namespace = ?1 AND deletion_timestamp IS NULL 
                ORDER BY name
            "#
        } else {
            r#"
                SELECT uid, namespace, name, ingress_class_name, default_backend, rules, tls, load_balancer,
                       labels, annotations, resource_version, generation, creation_timestamp 
                FROM ingresses 
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
            let ingress = self.row_to_ingress(row, &ns, &name)?;
            items.push(ingress);
        }

        Ok(json!({
            "apiVersion": "networking.k8s.io/v1",
            "kind": "IngressList",
            "metadata": {
                "selfLink": if let Some(ns) = namespace {
                    format!("/apis/networking.k8s.io/v1/namespaces/{}/ingresses", ns)
                } else {
                    "/apis/networking.k8s.io/v1/ingresses".to_string()
                },
                "resourceVersion": "1",
            },
            "items": items,
        }))
    }

    pub async fn update(&self, namespace: &str, name: &str, mut ingress: Value) -> Result<Value> {
        // Get existing ingress to preserve UID and creation timestamp
        let existing = self.get(namespace, name).await?;
        let uid = existing["metadata"]["uid"].as_str().unwrap();
        let creation_timestamp = existing["metadata"]["creationTimestamp"].as_str().unwrap();
        let current_version: i64 = existing["metadata"]["resourceVersion"]
            .as_str()
            .unwrap()
            .parse()?;
        
        // Extract spec fields
        let ingress_class_name = ingress["spec"].get("ingressClassName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let default_backend = ingress["spec"].get("defaultBackend").cloned();
        let rules = ingress["spec"].get("rules").cloned();
        let tls = ingress["spec"].get("tls").cloned();
        
        let labels = ingress["metadata"].get("labels").unwrap_or(&json!({})).clone();
        let annotations = ingress["metadata"].get("annotations").unwrap_or(&json!({})).clone();

        // Update in database
        let update_query = r#"
            UPDATE ingresses 
            SET ingress_class_name = ?1, default_backend = ?2, rules = ?3, tls = ?4,
                labels = ?5, annotations = ?6, resource_version = ?7, generation = generation + 1
            WHERE namespace = ?8 AND name = ?9 AND deletion_timestamp IS NULL
        "#;
        
        let new_version = current_version + 1;
        
        sqlx::query(update_query)
            .bind(ingress_class_name.clone())
            .bind(default_backend.as_ref().map(|v| v.to_string()))
            .bind(rules.as_ref().map(|v| v.to_string()))
            .bind(tls.as_ref().map(|v| v.to_string()))
            .bind(labels.to_string())
            .bind(annotations.to_string())
            .bind(new_version)
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        // Build response
        ingress["apiVersion"] = json!("networking.k8s.io/v1");
        ingress["kind"] = json!("Ingress");
        ingress["metadata"]["uid"] = json!(uid);
        ingress["metadata"]["namespace"] = json!(namespace);
        ingress["metadata"]["name"] = json!(name);
        ingress["metadata"]["resourceVersion"] = json!(new_version.to_string());
        ingress["metadata"]["creationTimestamp"] = json!(creation_timestamp);
        ingress["metadata"]["selfLink"] = json!(format!("/apis/networking.k8s.io/v1/namespaces/{}/ingresses/{}", namespace, name));
        
        // Set spec defaults
        if let Some(class_name) = ingress_class_name {
            ingress["spec"]["ingressClassName"] = json!(class_name);
        }
        
        // Preserve status
        ingress["status"] = existing["status"].clone();
        
        if !labels.is_null() && labels != json!({}) {
            ingress["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            ingress["metadata"]["annotations"] = annotations;
        }

        Ok(ingress)
    }

    pub async fn delete(&self, namespace: &str, name: &str) -> Result<Value> {
        // Get the Ingress before deletion
        let ingress = self.get(namespace, name).await?;

        // Soft delete
        let deletion_timestamp = Utc::now().to_rfc3339();
        let delete_query = "UPDATE ingresses SET deletion_timestamp = ?1 WHERE namespace = ?2 AND name = ?3";
        
        sqlx::query(delete_query)
            .bind(&deletion_timestamp)
            .bind(namespace)
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(ingress)
    }

    fn row_to_ingress(&self, row: sqlx::sqlite::SqliteRow, namespace: &str, name: &str) -> Result<Value> {
        let uid: String = row.get("uid");
        let ingress_class_name: Option<String> = row.get("ingress_class_name");
        let default_backend_str: Option<String> = row.get("default_backend");
        let rules_str: Option<String> = row.get("rules");
        let tls_str: Option<String> = row.get("tls");
        let load_balancer_str: String = row.get("load_balancer");
        
        let labels_str: String = row.get("labels");
        let annotations_str: String = row.get("annotations");
        let resource_version: i64 = row.get("resource_version");
        let generation: i64 = row.get("generation");
        let creation_timestamp: String = row.get("creation_timestamp");

        let default_backend: Option<Value> = default_backend_str.map(|s| serde_json::from_str(&s)).transpose()?;
        let rules: Option<Value> = rules_str.map(|s| serde_json::from_str(&s)).transpose()?;
        let tls: Option<Value> = tls_str.map(|s| serde_json::from_str(&s)).transpose()?;
        let load_balancer: Value = serde_json::from_str(&load_balancer_str)?;
        let labels: Value = serde_json::from_str(&labels_str)?;
        let annotations: Value = serde_json::from_str(&annotations_str)?;

        let mut ingress = json!({
            "apiVersion": "networking.k8s.io/v1",
            "kind": "Ingress",
            "metadata": {
                "name": name,
                "namespace": namespace,
                "uid": uid,
                "resourceVersion": resource_version.to_string(),
                "generation": generation,
                "creationTimestamp": creation_timestamp,
                "selfLink": format!("/apis/networking.k8s.io/v1/namespaces/{}/ingresses/{}", namespace, name),
            },
            "spec": {},
            "status": {
                "loadBalancer": load_balancer
            }
        });
        
        // Add optional spec fields
        if let Some(class_name) = ingress_class_name {
            ingress["spec"]["ingressClassName"] = json!(class_name);
        }
        if let Some(backend) = default_backend {
            ingress["spec"]["defaultBackend"] = backend;
        }
        if let Some(r) = rules {
            ingress["spec"]["rules"] = r;
        }
        if let Some(t) = tls {
            ingress["spec"]["tls"] = t;
        }

        if !labels.is_null() && labels != json!({}) {
            ingress["metadata"]["labels"] = labels;
        }

        if !annotations.is_null() && annotations != json!({}) {
            ingress["metadata"]["annotations"] = annotations;
        }

        Ok(ingress)
    }
}