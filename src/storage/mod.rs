pub mod deployment_store;
pub mod endpoints_store;
pub mod pod_store;
pub mod replicaset_store;
pub mod service_store;
pub mod watch_store;

use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::Arc;

use self::deployment_store::DeploymentStore;
use self::endpoints_store::EndpointsStore;
use self::pod_store::PodStore;
use self::replicaset_store::ReplicaSetStore;
use self::service_store::ServiceStore;
use self::watch_store::WatchStore;

#[derive(Clone)]
pub struct Storage {
    pub pool: Arc<SqlitePool>,
}

impl Storage {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        
        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations").run(&*self.pool).await?;
        Ok(())
    }

    pub fn pods(&self) -> PodStore {
        PodStore::new((*self.pool).clone())
    }

    pub fn services(&self) -> ServiceStore {
        ServiceStore::new((*self.pool).clone())
    }

    pub fn endpoints(&self) -> EndpointsStore {
        EndpointsStore::new((*self.pool).clone())
    }

    pub fn deployments(&self) -> DeploymentStore {
        DeploymentStore::new((*self.pool).clone())
    }

    pub fn replicasets(&self) -> ReplicaSetStore {
        ReplicaSetStore::new((*self.pool).clone())
    }

    pub fn watch(&self) -> WatchStore {
        WatchStore::new((*self.pool).clone())
    }
}