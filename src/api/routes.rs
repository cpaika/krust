use axum::{
    http::StatusCode,
    response::Json,
    routing::{any, delete, get, patch, post, put},
    Router,
};
use serde_json::{json, Value};

use super::configmap_handlers;
use super::cronjob_handlers;
use super::daemonset_handlers;
use super::handlers;
use super::ingress_handlers;
use super::job_handlers;
use super::networkpolicy_handlers;
use super::pv_handlers;
use super::pvc_handlers;
use super::quota_handlers;
use super::secret_handlers;
use super::serviceaccount_handlers;
use super::statefulset_handlers;
use super::pod_proxy;
use super::portforward;
use super::portforward_spdy;
use super::portforward_v2;
use super::server::AppState;
use super::service_portforward;

pub fn v1_routes() -> Router<AppState> {
    Router::new()
        // Namespace routes
        .route("/namespaces", get(handlers::list_namespaces))
        .route("/namespaces", post(handlers::create_namespace))
        .route("/namespaces/:name", get(handlers::get_namespace))
        .route("/namespaces/:name", put(handlers::update_namespace))
        .route("/namespaces/:name", patch(handlers::patch_namespace))
        .route("/namespaces/:name", delete(handlers::delete_namespace))
        // Pod routes
        .route("/pods", get(handlers::list_all_pods))
        .route("/namespaces/:namespace/pods", get(handlers::list_pods))
        .route("/namespaces/:namespace/pods", post(handlers::create_pod))
        .route(
            "/namespaces/:namespace/pods/:name",
            get(handlers::get_pod),
        )
        .route(
            "/namespaces/:namespace/pods/:name",
            put(handlers::update_pod),
        )
        .route(
            "/namespaces/:namespace/pods/:name",
            patch(handlers::patch_pod),
        )
        .route(
            "/namespaces/:namespace/pods/:name",
            delete(handlers::delete_pod),
        )
        // Pod logs endpoint
        .route(
            "/namespaces/:namespace/pods/:name/log",
            get(handlers::get_pod_logs),
        )
        // Pod subresources
        .route(
            "/namespaces/:namespace/pods/:name/status",
            get(handlers::get_pod_status)
                .put(handlers::update_pod_status)
                .patch(handlers::patch_pod_status),
        )
        .route(
            "/namespaces/:namespace/pods/:name/ephemeralcontainers",
            patch(handlers::update_pod_ephemeralcontainers),
        )
        .route(
            "/namespaces/:namespace/pods/:name/binding",
            post(handlers::create_pod_binding),
        )
        .route(
            "/namespaces/:namespace/pods/:name/exec",
            get(handlers::pod_exec),
        )
        .route(
            "/namespaces/:namespace/pods/:name/attach",
            get(handlers::pod_attach),
        )
        // Pod port-forward endpoints (WebSocket handler with SPDY protocol)
        .route(
            "/namespaces/:namespace/pods/:name/portforward",
            any(super::portforward_champion::handle_portforward_champion),
        )
        // Pod proxy endpoints - directly access pod services
        .route(
            "/proxy/pods/:namespace/:name/:port",
            get(pod_proxy::proxy_to_pod_root),
        )
        .route(
            "/proxy/pods/:namespace/:name/:port/*path",
            get(pod_proxy::proxy_to_pod),
        )
        // Service routes
        .route("/services", get(handlers::list_all_services))
        .route(
            "/namespaces/:namespace/services",
            get(handlers::list_services),
        )
        .route(
            "/namespaces/:namespace/services",
            post(handlers::create_service),
        )
        .route(
            "/namespaces/:namespace/services/:name",
            get(handlers::get_service),
        )
        .route(
            "/namespaces/:namespace/services/:name",
            put(handlers::update_service),
        )
        .route(
            "/namespaces/:namespace/services/:name",
            patch(handlers::patch_service),
        )
        .route(
            "/namespaces/:namespace/services/:name",
            delete(handlers::delete_service),
        )
        // Service port-forward endpoint
        .route(
            "/namespaces/:namespace/services/:name/portforward",
            get(service_portforward::service_portforward_handler),
        )
        // Endpoints routes
        .route("/endpoints", get(handlers::list_all_endpoints))
        .route(
            "/namespaces/:namespace/endpoints",
            get(handlers::list_endpoints),
        )
        .route(
            "/namespaces/:namespace/endpoints/:name",
            get(handlers::get_endpoints),
        )
        .route(
            "/namespaces/:namespace/endpoints",
            post(handlers::create_endpoints),
        )
        .route(
            "/namespaces/:namespace/endpoints/:name",
            put(handlers::update_endpoints),
        )
        .route(
            "/namespaces/:namespace/endpoints/:name",
            delete(handlers::delete_endpoints),
        )
        // Node routes
        .route("/nodes", get(handlers::list_nodes))
        .route("/nodes/:name", get(handlers::get_node))
        // ConfigMap routes
        .route("/configmaps", get(configmap_handlers::list_all_configmaps))
        .route(
            "/namespaces/:namespace/configmaps",
            get(configmap_handlers::list_configmaps),
        )
        .route(
            "/namespaces/:namespace/configmaps",
            post(configmap_handlers::create_configmap),
        )
        .route(
            "/namespaces/:namespace/configmaps/:name",
            get(configmap_handlers::get_configmap),
        )
        .route(
            "/namespaces/:namespace/configmaps/:name",
            put(configmap_handlers::update_configmap),
        )
        .route(
            "/namespaces/:namespace/configmaps/:name",
            patch(configmap_handlers::patch_configmap),
        )
        .route(
            "/namespaces/:namespace/configmaps/:name",
            delete(configmap_handlers::delete_configmap),
        )
        // Secret routes
        .route("/secrets", get(secret_handlers::list_all_secrets))
        .route(
            "/namespaces/:namespace/secrets",
            get(secret_handlers::list_secrets),
        )
        .route(
            "/namespaces/:namespace/secrets",
            post(secret_handlers::create_secret),
        )
        .route(
            "/namespaces/:namespace/secrets/:name",
            get(secret_handlers::get_secret),
        )
        .route(
            "/namespaces/:namespace/secrets/:name",
            put(secret_handlers::update_secret),
        )
        .route(
            "/namespaces/:namespace/secrets/:name",
            patch(secret_handlers::patch_secret),
        )
        .route(
            "/namespaces/:namespace/secrets/:name",
            delete(secret_handlers::delete_secret),
        )
        // PersistentVolume routes
        .route("/persistentvolumes", get(pv_handlers::list_pvs))
        .route("/persistentvolumes", post(pv_handlers::create_pv))
        .route("/persistentvolumes/:name", get(pv_handlers::get_pv))
        .route("/persistentvolumes/:name", put(pv_handlers::update_pv))
        .route("/persistentvolumes/:name", delete(pv_handlers::delete_pv))
        // PersistentVolumeClaim routes
        .route("/persistentvolumeclaims", get(pvc_handlers::list_all_pvcs))
        .route(
            "/namespaces/:namespace/persistentvolumeclaims",
            get(pvc_handlers::list_pvcs),
        )
        .route(
            "/namespaces/:namespace/persistentvolumeclaims",
            post(pvc_handlers::create_pvc),
        )
        .route(
            "/namespaces/:namespace/persistentvolumeclaims/:name",
            get(pvc_handlers::get_pvc),
        )
        .route(
            "/namespaces/:namespace/persistentvolumeclaims/:name",
            put(pvc_handlers::update_pvc),
        )
        .route(
            "/namespaces/:namespace/persistentvolumeclaims/:name",
            delete(pvc_handlers::delete_pvc),
        )
        // ResourceQuota routes
        .route("/resourcequotas", get(quota_handlers::list_all_resourcequotas))
        .route(
            "/namespaces/:namespace/resourcequotas",
            get(quota_handlers::list_resourcequotas),
        )
        .route(
            "/namespaces/:namespace/resourcequotas",
            post(quota_handlers::create_resourcequota),
        )
        .route(
            "/namespaces/:namespace/resourcequotas/:name",
            get(quota_handlers::get_resourcequota),
        )
        .route(
            "/namespaces/:namespace/resourcequotas/:name",
            put(quota_handlers::update_resourcequota),
        )
        .route(
            "/namespaces/:namespace/resourcequotas/:name",
            delete(quota_handlers::delete_resourcequota),
        )
        .route(
            "/namespaces/:namespace/resourcequotas/:name/status",
            get(quota_handlers::get_resourcequota_status)
                .put(quota_handlers::update_resourcequota_status),
        )
        // LimitRange routes
        .route("/limitranges", get(quota_handlers::list_all_limitranges))
        .route(
            "/namespaces/:namespace/limitranges",
            get(quota_handlers::list_limitranges),
        )
        .route(
            "/namespaces/:namespace/limitranges",
            post(quota_handlers::create_limitrange),
        )
        .route(
            "/namespaces/:namespace/limitranges/:name",
            get(quota_handlers::get_limitrange),
        )
        .route(
            "/namespaces/:namespace/limitranges/:name",
            put(quota_handlers::update_limitrange),
        )
        .route(
            "/namespaces/:namespace/limitranges/:name",
            delete(quota_handlers::delete_limitrange),
        )
        // ServiceAccount routes
        .route("/serviceaccounts", get(serviceaccount_handlers::list_all_serviceaccounts))
        .route(
            "/namespaces/:namespace/serviceaccounts",
            get(serviceaccount_handlers::list_serviceaccounts),
        )
        .route(
            "/namespaces/:namespace/serviceaccounts",
            post(serviceaccount_handlers::create_serviceaccount),
        )
        .route(
            "/namespaces/:namespace/serviceaccounts/:name",
            get(serviceaccount_handlers::get_serviceaccount),
        )
        .route(
            "/namespaces/:namespace/serviceaccounts/:name",
            put(serviceaccount_handlers::update_serviceaccount),
        )
        .route(
            "/namespaces/:namespace/serviceaccounts/:name",
            patch(serviceaccount_handlers::patch_serviceaccount),
        )
        .route(
            "/namespaces/:namespace/serviceaccounts/:name",
            delete(serviceaccount_handlers::delete_serviceaccount),
        )
        .route(
            "/namespaces/:namespace/serviceaccounts/:name/token",
            post(serviceaccount_handlers::create_serviceaccount_token),
        )
        // Watch endpoints
        .route("/watch/pods", get(handlers::watch_pods))
        .route(
            "/watch/namespaces/:namespace/pods",
            get(handlers::watch_namespace_pods),
        )
}

pub fn apps_v1_routes() -> Router<AppState> {
    Router::new()
        // Deployments
        .route("/deployments", get(handlers::list_all_deployments))
        .route("/namespaces/:namespace/deployments", get(handlers::list_deployments))
        .route("/namespaces/:namespace/deployments", post(handlers::create_deployment))
        .route("/namespaces/:namespace/deployments/:name", get(handlers::get_deployment))
        .route("/namespaces/:namespace/deployments/:name", put(handlers::update_deployment))
        .route("/namespaces/:namespace/deployments/:name", patch(handlers::patch_deployment))
        .route("/namespaces/:namespace/deployments/:name", delete(handlers::delete_deployment))
        // Scale subresource
        .route("/namespaces/:namespace/deployments/:name/scale", get(handlers::get_deployment_scale))
        .route("/namespaces/:namespace/deployments/:name/scale", put(handlers::update_deployment_scale))
        .route("/namespaces/:namespace/deployments/:name/scale", patch(handlers::patch_deployment_scale))
        // Status subresource
        .route("/namespaces/:namespace/deployments/:name/status", get(handlers::get_deployment_status))
        .route("/namespaces/:namespace/deployments/:name/status", put(handlers::update_deployment_status))
        // ReplicaSets
        .route("/replicasets", get(handlers::list_all_replicasets))
        .route("/namespaces/:namespace/replicasets", get(handlers::list_replicasets))
        .route("/namespaces/:namespace/replicasets", post(handlers::create_replicaset))
        .route("/namespaces/:namespace/replicasets/:name", get(handlers::get_replicaset))
        .route("/namespaces/:namespace/replicasets/:name", put(handlers::update_replicaset))
        .route("/namespaces/:namespace/replicasets/:name", patch(handlers::patch_replicaset))
        .route("/namespaces/:namespace/replicasets/:name", delete(handlers::delete_replicaset))
        // ReplicaSet scale subresource
        .route("/namespaces/:namespace/replicasets/:name/scale", get(handlers::get_replicaset_scale))
        .route("/namespaces/:namespace/replicasets/:name/scale", put(handlers::update_replicaset_scale))
        // ReplicaSet status subresource
        .route("/namespaces/:namespace/replicasets/:name/status", put(handlers::update_replicaset_status))
        // StatefulSets
        .route("/statefulsets", get(statefulset_handlers::list_all_statefulsets))
        .route("/namespaces/:namespace/statefulsets", get(statefulset_handlers::list_statefulsets))
        .route("/namespaces/:namespace/statefulsets", post(statefulset_handlers::create_statefulset))
        .route("/namespaces/:namespace/statefulsets/:name", get(statefulset_handlers::get_statefulset))
        .route("/namespaces/:namespace/statefulsets/:name", put(statefulset_handlers::update_statefulset))
        .route("/namespaces/:namespace/statefulsets/:name", delete(statefulset_handlers::delete_statefulset))
        // StatefulSet scale subresource
        .route("/namespaces/:namespace/statefulsets/:name/scale", get(statefulset_handlers::get_statefulset_scale))
        .route("/namespaces/:namespace/statefulsets/:name/scale", put(statefulset_handlers::update_statefulset_scale))
        // StatefulSet status subresource
        .route("/namespaces/:namespace/statefulsets/:name/status", get(statefulset_handlers::get_statefulset_status))
        // DaemonSets
        .route("/daemonsets", get(daemonset_handlers::list_all_daemonsets))
        .route("/namespaces/:namespace/daemonsets", get(daemonset_handlers::list_daemonsets))
        .route("/namespaces/:namespace/daemonsets", post(daemonset_handlers::create_daemonset))
        .route("/namespaces/:namespace/daemonsets/:name", get(daemonset_handlers::get_daemonset))
        .route("/namespaces/:namespace/daemonsets/:name", put(daemonset_handlers::update_daemonset))
        .route("/namespaces/:namespace/daemonsets/:name", delete(daemonset_handlers::delete_daemonset))
        // DaemonSet status subresource
        .route("/namespaces/:namespace/daemonsets/:name/status", get(daemonset_handlers::get_daemonset_status))
}

pub fn batch_v1_routes() -> Router<AppState> {
    Router::new()
        // Jobs
        .route("/jobs", get(job_handlers::list_jobs_all_namespaces))
        .route("/namespaces/:namespace/jobs", get(job_handlers::list_jobs_namespaced))
        .route("/namespaces/:namespace/jobs", post(job_handlers::create_job))
        .route("/namespaces/:namespace/jobs/:name", get(job_handlers::get_job))
        .route("/namespaces/:namespace/jobs/:name", delete(job_handlers::delete_job))
        // Job status subresource
        .route("/namespaces/:namespace/jobs/:name/status", put(job_handlers::update_job_status))
        // CronJobs
        .route("/cronjobs", get(cronjob_handlers::list_cronjobs_all_namespaces))
        .route("/namespaces/:namespace/cronjobs", get(cronjob_handlers::list_cronjobs_namespaced))
        .route("/namespaces/:namespace/cronjobs", post(cronjob_handlers::create_cronjob))
        .route("/namespaces/:namespace/cronjobs/:name", get(cronjob_handlers::get_cronjob))
        .route("/namespaces/:namespace/cronjobs/:name", delete(cronjob_handlers::delete_cronjob))
        // CronJob status subresource
        .route("/namespaces/:namespace/cronjobs/:name/status", put(cronjob_handlers::update_cronjob_status))
}

pub fn networking_v1_routes() -> Router<AppState> {
    Router::new()
        // NetworkPolicy routes
        .route("/networkpolicies", get(networkpolicy_handlers::list_networkpolicies_all_namespaces))
        .route("/namespaces/:namespace/networkpolicies", get(networkpolicy_handlers::list_networkpolicies_namespaced))
        .route("/namespaces/:namespace/networkpolicies", post(networkpolicy_handlers::create_networkpolicy))
        .route("/namespaces/:namespace/networkpolicies/:name", get(networkpolicy_handlers::get_networkpolicy))
        .route("/namespaces/:namespace/networkpolicies/:name", put(networkpolicy_handlers::update_networkpolicy))
        .route("/namespaces/:namespace/networkpolicies/:name", patch(networkpolicy_handlers::patch_networkpolicy))
        .route("/namespaces/:namespace/networkpolicies/:name", delete(networkpolicy_handlers::delete_networkpolicy))
        // Ingress routes
        .route("/ingresses", get(ingress_handlers::list_ingresses_all_namespaces))
        .route("/namespaces/:namespace/ingresses", get(ingress_handlers::list_ingresses_namespaced))
        .route("/namespaces/:namespace/ingresses", post(ingress_handlers::create_ingress))
        .route("/namespaces/:namespace/ingresses/:name", get(ingress_handlers::get_ingress))
        .route("/namespaces/:namespace/ingresses/:name", put(ingress_handlers::update_ingress))
        .route("/namespaces/:namespace/ingresses/:name", patch(ingress_handlers::patch_ingress))
        .route("/namespaces/:namespace/ingresses/:name", delete(ingress_handlers::delete_ingress))
        // Ingress status subresource
        .route("/namespaces/:namespace/ingresses/:name/status", put(ingress_handlers::update_ingress_status))
}

pub fn autoscaling_v2_routes() -> Router<AppState> {
    Router::new()
        // HorizontalPodAutoscalers
        .route("/horizontalpodautoscalers", get(handlers::list_all_hpas))
        .route("/namespaces/:namespace/horizontalpodautoscalers", get(handlers::list_hpas))
        .route("/namespaces/:namespace/horizontalpodautoscalers", post(handlers::create_hpa))
        .route("/namespaces/:namespace/horizontalpodautoscalers/:name", get(handlers::get_hpa))
        .route("/namespaces/:namespace/horizontalpodautoscalers/:name", put(handlers::update_hpa))
        .route("/namespaces/:namespace/horizontalpodautoscalers/:name", delete(handlers::delete_hpa))
        // HPA status subresource
        .route("/namespaces/:namespace/horizontalpodautoscalers/:name/status", get(handlers::get_hpa_status))
        .route("/namespaces/:namespace/horizontalpodautoscalers/:name/status", put(handlers::update_hpa_status))
}

pub fn policy_v1_routes() -> Router<AppState> {
    use super::pdb_handlers;
    
    Router::new()
        // PodDisruptionBudgets
        .route("/poddisruptionbudgets", get(pdb_handlers::list_all_pdbs))
        .route("/namespaces/:namespace/poddisruptionbudgets", get(pdb_handlers::list_pdbs))
        .route("/namespaces/:namespace/poddisruptionbudgets", post(pdb_handlers::create_pdb))
        .route("/namespaces/:namespace/poddisruptionbudgets/:name", get(pdb_handlers::get_pdb))
        .route("/namespaces/:namespace/poddisruptionbudgets/:name", put(pdb_handlers::update_pdb))
        .route("/namespaces/:namespace/poddisruptionbudgets/:name", patch(pdb_handlers::patch_pdb))
        .route("/namespaces/:namespace/poddisruptionbudgets/:name", delete(pdb_handlers::delete_pdb))
        .route("/namespaces/:namespace/poddisruptionbudgets/:name/status", 
            get(pdb_handlers::get_pdb_status).put(pdb_handlers::update_pdb_status))
}

pub fn scheduling_v1_routes() -> Router<AppState> {
    use super::scheduling_handlers;
    
    Router::new()
        // PriorityClasses
        .route("/priorityclasses", get(scheduling_handlers::list_priorityclasses))
        .route("/priorityclasses", post(scheduling_handlers::create_priorityclass))
        .route("/priorityclasses/:name", get(scheduling_handlers::get_priorityclass))
        .route("/priorityclasses/:name", put(scheduling_handlers::update_priorityclass))
        .route("/priorityclasses/:name", delete(scheduling_handlers::delete_priorityclass))
}

pub fn storage_v1_routes() -> Router<AppState> {
    use super::scheduling_handlers;
    
    Router::new()
        // StorageClasses
        .route("/storageclasses", get(scheduling_handlers::list_storageclasses))
        .route("/storageclasses", post(scheduling_handlers::create_storageclass))
        .route("/storageclasses/:name", get(scheduling_handlers::get_storageclass))
        .route("/storageclasses/:name", put(scheduling_handlers::update_storageclass))
        .route("/storageclasses/:name", delete(scheduling_handlers::delete_storageclass))
}

pub fn admissionregistration_v1_routes() -> Router<AppState> {
    use super::webhook_handlers;
    
    Router::new()
        // ValidatingWebhookConfigurations
        .route("/validatingwebhookconfigurations", get(webhook_handlers::list_validating_webhooks))
        .route("/validatingwebhookconfigurations", post(webhook_handlers::create_validating_webhook))
        .route("/validatingwebhookconfigurations/:name", get(webhook_handlers::get_validating_webhook))
        .route("/validatingwebhookconfigurations/:name", put(webhook_handlers::update_validating_webhook))
        .route("/validatingwebhookconfigurations/:name", delete(webhook_handlers::delete_validating_webhook))
        // MutatingWebhookConfigurations
        .route("/mutatingwebhookconfigurations", get(webhook_handlers::list_mutating_webhooks))
        .route("/mutatingwebhookconfigurations", post(webhook_handlers::create_mutating_webhook))
        .route("/mutatingwebhookconfigurations/:name", get(webhook_handlers::get_mutating_webhook))
        .route("/mutatingwebhookconfigurations/:name", put(webhook_handlers::update_mutating_webhook))
        .route("/mutatingwebhookconfigurations/:name", delete(webhook_handlers::delete_mutating_webhook))
}

pub fn rbac_v1_routes() -> Router<AppState> {
    use super::rbac_handlers;
    
    Router::new()
        // Roles
        .route("/namespaces/:namespace/roles", get(rbac_handlers::list_roles))
        .route("/namespaces/:namespace/roles", post(rbac_handlers::create_role))
        .route("/namespaces/:namespace/roles/:name", get(rbac_handlers::get_role))
        .route("/namespaces/:namespace/roles/:name", put(rbac_handlers::update_role))
        .route("/namespaces/:namespace/roles/:name", delete(rbac_handlers::delete_role))
        // RoleBindings
        .route("/namespaces/:namespace/rolebindings", get(rbac_handlers::list_rolebindings))
        .route("/namespaces/:namespace/rolebindings", post(rbac_handlers::create_rolebinding))
        .route("/namespaces/:namespace/rolebindings/:name", get(rbac_handlers::get_rolebinding))
        .route("/namespaces/:namespace/rolebindings/:name", delete(rbac_handlers::delete_rolebinding))
        // ClusterRoles
        .route("/clusterroles", get(rbac_handlers::list_clusterroles))
        .route("/clusterroles", post(rbac_handlers::create_clusterrole))
        .route("/clusterroles/:name", get(rbac_handlers::get_clusterrole))
        .route("/clusterroles/:name", put(rbac_handlers::update_clusterrole))
        .route("/clusterroles/:name", delete(rbac_handlers::delete_clusterrole))
        // ClusterRoleBindings
        .route("/clusterrolebindings", get(rbac_handlers::list_clusterrolebindings))
        .route("/clusterrolebindings", post(rbac_handlers::create_clusterrolebinding))
        .route("/clusterrolebindings/:name", get(rbac_handlers::get_clusterrolebinding))
        .route("/clusterrolebindings/:name", delete(rbac_handlers::delete_clusterrolebinding))
}