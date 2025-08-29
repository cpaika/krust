use anyhow::Result;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

// PriorityClass storage
pub struct PriorityClassStore {
    pool: SqlitePool,
}

impl PriorityClassStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, mut pc: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = pc["metadata"]["name"].as_str().unwrap().to_string();
        let value = pc["value"].as_i64().unwrap_or(0);
        let global_default = pc.get("globalDefault").and_then(|v| v.as_bool()).unwrap_or(false);
        let description = pc.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
        let preemption_policy = pc.get("preemptionPolicy").and_then(|v| v.as_str())
            .unwrap_or("PreemptLowerPriority").to_string();
        let labels = pc["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = pc["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "INSERT INTO priorityclasses (uid, name, value, global_default, description,
             preemption_policy, labels, annotations, resource_version, generation)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, 1)"
        )
        .bind(&uid)
        .bind(&name)
        .bind(value)
        .bind(global_default)
        .bind(description)
        .bind(preemption_policy)
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .execute(&self.pool)
        .await?;

        pc["metadata"]["uid"] = json!(uid);
        pc["metadata"]["resourceVersion"] = json!("1");
        pc["metadata"]["generation"] = json!(1);
        pc["metadata"]["creationTimestamp"] = json!(chrono::Utc::now().to_rfc3339());

        self.record_event(&uid, "PriorityClass", &name, "Created", "PriorityClass created").await?;
        Ok(pc)
    }

    pub async fn get(&self, name: &str) -> Result<Option<Value>> {
        let row = sqlx::query(
            "SELECT uid, name, value, global_default, description, preemption_policy,
             labels, annotations, resource_version, generation, creation_timestamp
             FROM priorityclasses WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let value: i64 = row.get("value");
            let global_default: bool = row.get("global_default");
            let description: Option<String> = row.get("description");
            let preemption_policy: String = row.get("preemption_policy");
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let mut pc = json!({
                "apiVersion": "scheduling.k8s.io/v1",
                "kind": "PriorityClass",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "resourceVersion": resource_version.to_string(),
                    "generation": generation,
                    "creationTimestamp": creation_timestamp,
                    "labels": serde_json::from_str(&labels)?,
                    "annotations": serde_json::from_str(&annotations)?
                },
                "value": value,
                "globalDefault": global_default,
                "preemptionPolicy": preemption_policy
            });

            if let Some(desc) = description {
                pc["description"] = json!(desc);
            }

            Ok(Some(pc))
        } else {
            Ok(None)
        }
    }

    pub async fn delete(&self, name: &str) -> Result<Value> {
        let pc = self.get(name).await?
            .ok_or_else(|| anyhow::anyhow!("PriorityClass not found"))?;

        let uid = pc["metadata"]["uid"].as_str().unwrap();

        sqlx::query(
            "UPDATE priorityclasses SET deletion_timestamp = CURRENT_TIMESTAMP 
             WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .execute(&self.pool)
        .await?;

        self.record_event(uid, "PriorityClass", name, "Deleted", "PriorityClass deleted").await?;
        Ok(pc)
    }

    pub async fn list(&self) -> Result<Value> {
        let rows = sqlx::query(
            "SELECT uid, name, value, global_default, description, preemption_policy,
             labels, annotations, resource_version, generation, creation_timestamp
             FROM priorityclasses WHERE deletion_timestamp IS NULL"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut items = Vec::new();
        for row in rows {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let value: i64 = row.get("value");
            let global_default: bool = row.get("global_default");
            let description: Option<String> = row.get("description");
            let preemption_policy: String = row.get("preemption_policy");
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let mut pc = json!({
                "apiVersion": "scheduling.k8s.io/v1",
                "kind": "PriorityClass",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "resourceVersion": resource_version.to_string(),
                    "generation": generation,
                    "creationTimestamp": creation_timestamp,
                    "labels": serde_json::from_str(&labels)?,
                    "annotations": serde_json::from_str(&annotations)?
                },
                "value": value,
                "globalDefault": global_default,
                "preemptionPolicy": preemption_policy
            });

            if let Some(desc) = description {
                pc["description"] = json!(desc);
            }

            items.push(pc);
        }

        Ok(json!({
            "apiVersion": "scheduling.k8s.io/v1",
            "kind": "PriorityClassList",
            "items": items
        }))
    }

    async fn record_event(&self, uid: &str, resource_type: &str, name: &str, reason: &str, message: &str) -> Result<()> {
        let event_uid = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO events (uid, namespace, involved_object_uid, involved_object_kind, 
             involved_object_name, reason, message, event_time, first_timestamp, last_timestamp, count, type)
             VALUES (?, '', ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1, 'Normal')"
        )
        .bind(&event_uid)
        .bind(uid)
        .bind(resource_type)
        .bind(name)
        .bind(reason)
        .bind(message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

// StorageClass storage
pub struct StorageClassStore {
    pool: SqlitePool,
}

impl StorageClassStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, mut sc: Value) -> Result<Value> {
        let uid = Uuid::new_v4().to_string();
        let name = sc["metadata"]["name"].as_str().unwrap().to_string();
        let provisioner = sc["provisioner"].as_str().unwrap().to_string();
        let parameters = sc.get("parameters").cloned();
        let reclaim_policy = sc.get("reclaimPolicy").and_then(|v| v.as_str())
            .unwrap_or("Delete").to_string();
        let mount_options = sc.get("mountOptions").cloned();
        let allow_volume_expansion = sc.get("allowVolumeExpansion").and_then(|v| v.as_bool()).unwrap_or(false);
        let volume_binding_mode = sc.get("volumeBindingMode").and_then(|v| v.as_str())
            .unwrap_or("Immediate").to_string();
        let allowed_topologies = sc.get("allowedTopologies").cloned();
        let labels = sc["metadata"].get("labels").cloned().unwrap_or(json!({}));
        let annotations = sc["metadata"].get("annotations").cloned().unwrap_or(json!({}));

        sqlx::query(
            "INSERT INTO storageclasses (uid, name, provisioner, parameters, reclaim_policy,
             mount_options, allow_volume_expansion, volume_binding_mode, allowed_topologies,
             labels, annotations, resource_version, generation)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 1)"
        )
        .bind(&uid)
        .bind(&name)
        .bind(&provisioner)
        .bind(parameters.as_ref().map(|p| serde_json::to_string(p).ok()).flatten())
        .bind(&reclaim_policy)
        .bind(mount_options.as_ref().map(|m| serde_json::to_string(m).ok()).flatten())
        .bind(allow_volume_expansion)
        .bind(&volume_binding_mode)
        .bind(allowed_topologies.as_ref().map(|t| serde_json::to_string(t).ok()).flatten())
        .bind(serde_json::to_string(&labels)?)
        .bind(serde_json::to_string(&annotations)?)
        .execute(&self.pool)
        .await?;

        sc["metadata"]["uid"] = json!(uid);
        sc["metadata"]["resourceVersion"] = json!("1");
        sc["metadata"]["generation"] = json!(1);
        sc["metadata"]["creationTimestamp"] = json!(chrono::Utc::now().to_rfc3339());

        self.record_event(&uid, "StorageClass", &name, "Created", "StorageClass created").await?;
        Ok(sc)
    }

    pub async fn get(&self, name: &str) -> Result<Option<Value>> {
        let row = sqlx::query(
            "SELECT uid, name, provisioner, parameters, reclaim_policy, mount_options,
             allow_volume_expansion, volume_binding_mode, allowed_topologies,
             labels, annotations, resource_version, generation, creation_timestamp
             FROM storageclasses WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let provisioner: String = row.get("provisioner");
            let parameters: Option<String> = row.get("parameters");
            let reclaim_policy: String = row.get("reclaim_policy");
            let mount_options: Option<String> = row.get("mount_options");
            let allow_volume_expansion: bool = row.get("allow_volume_expansion");
            let volume_binding_mode: String = row.get("volume_binding_mode");
            let allowed_topologies: Option<String> = row.get("allowed_topologies");
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let mut sc = json!({
                "apiVersion": "storage.k8s.io/v1",
                "kind": "StorageClass",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "resourceVersion": resource_version.to_string(),
                    "generation": generation,
                    "creationTimestamp": creation_timestamp,
                    "labels": serde_json::from_str(&labels)?,
                    "annotations": serde_json::from_str(&annotations)?
                },
                "provisioner": provisioner,
                "reclaimPolicy": reclaim_policy,
                "allowVolumeExpansion": allow_volume_expansion,
                "volumeBindingMode": volume_binding_mode
            });

            if let Some(params) = parameters {
                sc["parameters"] = serde_json::from_str(&params)?;
            }
            if let Some(mount_opts) = mount_options {
                sc["mountOptions"] = serde_json::from_str(&mount_opts)?;
            }
            if let Some(topologies) = allowed_topologies {
                sc["allowedTopologies"] = serde_json::from_str(&topologies)?;
            }

            Ok(Some(sc))
        } else {
            Ok(None)
        }
    }

    pub async fn delete(&self, name: &str) -> Result<Value> {
        let sc = self.get(name).await?
            .ok_or_else(|| anyhow::anyhow!("StorageClass not found"))?;

        let uid = sc["metadata"]["uid"].as_str().unwrap();

        sqlx::query(
            "UPDATE storageclasses SET deletion_timestamp = CURRENT_TIMESTAMP 
             WHERE name = ? AND deletion_timestamp IS NULL"
        )
        .bind(name)
        .execute(&self.pool)
        .await?;

        self.record_event(uid, "StorageClass", name, "Deleted", "StorageClass deleted").await?;
        Ok(sc)
    }

    pub async fn list(&self) -> Result<Value> {
        let rows = sqlx::query(
            "SELECT uid, name, provisioner, parameters, reclaim_policy, mount_options,
             allow_volume_expansion, volume_binding_mode, allowed_topologies,
             labels, annotations, resource_version, generation, creation_timestamp
             FROM storageclasses WHERE deletion_timestamp IS NULL"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut items = Vec::new();
        for row in rows {
            let uid: String = row.get("uid");
            let name: String = row.get("name");
            let provisioner: String = row.get("provisioner");
            let parameters: Option<String> = row.get("parameters");
            let reclaim_policy: String = row.get("reclaim_policy");
            let mount_options: Option<String> = row.get("mount_options");
            let allow_volume_expansion: bool = row.get("allow_volume_expansion");
            let volume_binding_mode: String = row.get("volume_binding_mode");
            let allowed_topologies: Option<String> = row.get("allowed_topologies");
            let labels: String = row.get("labels");
            let annotations: String = row.get("annotations");
            let resource_version: i64 = row.get("resource_version");
            let generation: i64 = row.get("generation");
            let creation_timestamp: String = row.get("creation_timestamp");

            let mut sc = json!({
                "apiVersion": "storage.k8s.io/v1",
                "kind": "StorageClass",
                "metadata": {
                    "uid": uid,
                    "name": name,
                    "resourceVersion": resource_version.to_string(),
                    "generation": generation,
                    "creationTimestamp": creation_timestamp,
                    "labels": serde_json::from_str(&labels)?,
                    "annotations": serde_json::from_str(&annotations)?
                },
                "provisioner": provisioner,
                "reclaimPolicy": reclaim_policy,
                "allowVolumeExpansion": allow_volume_expansion,
                "volumeBindingMode": volume_binding_mode
            });

            if let Some(params) = parameters {
                sc["parameters"] = serde_json::from_str(&params)?;
            }
            if let Some(mount_opts) = mount_options {
                sc["mountOptions"] = serde_json::from_str(&mount_opts)?;
            }
            if let Some(topologies) = allowed_topologies {
                sc["allowedTopologies"] = serde_json::from_str(&topologies)?;
            }

            items.push(sc);
        }

        Ok(json!({
            "apiVersion": "storage.k8s.io/v1",
            "kind": "StorageClassList",
            "items": items
        }))
    }

    async fn record_event(&self, uid: &str, resource_type: &str, name: &str, reason: &str, message: &str) -> Result<()> {
        let event_uid = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO events (uid, namespace, involved_object_uid, involved_object_kind, 
             involved_object_name, reason, message, event_time, first_timestamp, last_timestamp, count, type)
             VALUES (?, '', ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1, 'Normal')"
        )
        .bind(&event_uid)
        .bind(uid)
        .bind(resource_type)
        .bind(name)
        .bind(reason)
        .bind(message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}