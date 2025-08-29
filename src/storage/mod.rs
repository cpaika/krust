pub mod configmap_store;
pub mod cronjob_store;
pub mod daemonset_store;
pub mod deployment_store;
pub mod endpoints_store;
pub mod hpa_store;
pub mod ingress_store;
pub mod job_store;
pub mod limitrange_store;
pub mod networkpolicy_store;
pub mod pdb_store;
pub mod pod_store;
pub mod pv_store;
pub mod pvc_store;
pub mod rbac_store;
pub mod replicaset_store;
pub mod resourcequota_store;
pub mod scheduling_store;
pub mod secret_store;
pub mod serviceaccount_store;
pub mod service_store;
pub mod statefulset_store;
pub mod watch_store;
pub mod webhook_store;

use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::Arc;

use self::configmap_store::ConfigMapStore;
use self::cronjob_store::CronJobStore;
use self::daemonset_store::DaemonSetStore;
use self::deployment_store::DeploymentStore;
use self::endpoints_store::EndpointsStore;
use self::hpa_store::HpaStore;
use self::ingress_store::IngressStore;
use self::job_store::JobStore;
use self::limitrange_store::LimitRangeStore;
use self::networkpolicy_store::NetworkPolicyStore;
use self::pdb_store::PdbStore;
use self::pod_store::PodStore;
use self::pv_store::PersistentVolumeStore;
use self::pvc_store::PersistentVolumeClaimStore;
use self::rbac_store::{RoleStore, RoleBindingStore, ClusterRoleStore, ClusterRoleBindingStore};
use self::replicaset_store::ReplicaSetStore;
use self::resourcequota_store::ResourceQuotaStore;
use self::scheduling_store::{PriorityClassStore, StorageClassStore};
use self::secret_store::SecretStore;
use self::serviceaccount_store::ServiceAccountStore;
use self::service_store::ServiceStore;
use self::statefulset_store::StatefulSetStore;
use self::watch_store::WatchStore;
use self::webhook_store::{ValidatingWebhookStore, MutatingWebhookStore};

#[derive(Clone)]
pub struct Storage {
    pub pool: Arc<SqlitePool>,
}

impl Storage {
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
    
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
    
    pub fn hpas(&self) -> HpaStore {
        HpaStore::new((*self.pool).clone())
    }

    pub fn replicasets(&self) -> ReplicaSetStore {
        ReplicaSetStore::new((*self.pool).clone())
    }

    pub fn configmaps(&self) -> ConfigMapStore {
        ConfigMapStore::new((*self.pool).clone())
    }

    pub fn secrets(&self) -> SecretStore {
        SecretStore::new((*self.pool).clone())
    }

    pub fn persistent_volumes(&self) -> PersistentVolumeStore {
        PersistentVolumeStore::new((*self.pool).clone())
    }

    pub fn persistent_volume_claims(&self) -> PersistentVolumeClaimStore {
        PersistentVolumeClaimStore::new((*self.pool).clone())
    }

    pub fn statefulsets(&self) -> StatefulSetStore {
        StatefulSetStore::new((*self.pool).clone())
    }

    pub fn daemonsets(&self) -> DaemonSetStore {
        DaemonSetStore::new((*self.pool).clone())
    }

    pub fn jobs(&self) -> JobStore {
        JobStore::new((*self.pool).clone())
    }

    pub fn cronjobs(&self) -> CronJobStore {
        CronJobStore::new((*self.pool).clone())
    }

    pub fn networkpolicies(&self) -> NetworkPolicyStore {
        NetworkPolicyStore::new((*self.pool).clone())
    }

    pub fn ingresses(&self) -> IngressStore {
        IngressStore::new((*self.pool).clone())
    }

    pub fn watch(&self) -> WatchStore {
        WatchStore::new((*self.pool).clone())
    }
    
    pub fn roles(&self) -> RoleStore {
        RoleStore::new((*self.pool).clone())
    }
    
    pub fn rolebindings(&self) -> RoleBindingStore {
        RoleBindingStore::new((*self.pool).clone())
    }
    
    pub fn clusterroles(&self) -> ClusterRoleStore {
        ClusterRoleStore::new((*self.pool).clone())
    }
    
    pub fn clusterrolebindings(&self) -> ClusterRoleBindingStore {
        ClusterRoleBindingStore::new((*self.pool).clone())
    }
    
    pub fn resourcequotas(&self) -> ResourceQuotaStore {
        ResourceQuotaStore::new((*self.pool).clone())
    }
    
    pub fn limitranges(&self) -> LimitRangeStore {
        LimitRangeStore::new((*self.pool).clone())
    }
    
    pub fn serviceaccounts(&self) -> ServiceAccountStore {
        ServiceAccountStore::new((*self.pool).clone())
    }
    
    pub fn pdbs(&self) -> PdbStore {
        PdbStore::new((*self.pool).clone())
    }
    
    pub fn priorityclasses(&self) -> PriorityClassStore {
        PriorityClassStore::new((*self.pool).clone())
    }
    
    pub fn storageclasses(&self) -> StorageClassStore {
        StorageClassStore::new((*self.pool).clone())
    }
    
    pub fn validating_webhooks(&self) -> ValidatingWebhookStore {
        ValidatingWebhookStore::new((*self.pool).clone())
    }
    
    pub fn mutating_webhooks(&self) -> MutatingWebhookStore {
        MutatingWebhookStore::new((*self.pool).clone())
    }
}