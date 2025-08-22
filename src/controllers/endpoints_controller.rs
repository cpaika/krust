use anyhow::Result;
use serde_json::Value;
use sqlx::Row;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};

use crate::Storage;

pub struct EndpointsController {
    storage: Storage,
}

impl EndpointsController {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting endpoints controller");
        
        loop {
            if let Err(e) = self.reconcile_endpoints().await {
                error!("Endpoints controller error: {}", e);
            }
            
            sleep(Duration::from_secs(2)).await;
        }
    }

    async fn reconcile_endpoints(&self) -> Result<()> {
        // Get all services with selectors
        let services = sqlx::query(
            "SELECT name, namespace, spec FROM services WHERE deletion_timestamp IS NULL"
        )
        .fetch_all(&*self.storage.pool)
        .await?;
        
        for service_row in services {
            let service_name: String = service_row.get("name");
            let service_namespace: String = service_row.get("namespace");
            let spec_str: String = service_row.get("spec");
            
            if let Ok(spec) = serde_json::from_str::<Value>(&spec_str) {
                if let Some(selector) = spec.get("selector") {
                    if !selector.is_null() {
                        // Update endpoints for this service
                        if let Err(e) = self.storage.endpoints()
                            .update_for_service(&service_namespace, &service_name, selector)
                            .await 
                        {
                            error!(
                                "Failed to update endpoints for service {}/{}: {}",
                                service_namespace, service_name, e
                            );
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
}