use axum::{
    http::StatusCode,
    response::Json,
    routing::{delete, get, patch, post, put},
    Router,
};
use serde_json::{json, Value};

use super::handlers;
use super::pod_proxy;
use super::portforward;
use super::server::AppState;

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
        // Pod port-forward endpoint
        .route(
            "/namespaces/:namespace/pods/:name/portforward",
            get(portforward::pod_portforward_get),
        )
        .route(
            "/namespaces/:namespace/pods/:name/portforward",
            post(portforward::pod_portforward_post),
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
        // Node routes
        .route("/nodes", get(handlers::list_nodes))
        .route("/nodes/:name", get(handlers::get_node))
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
        // ReplicaSets
        .route("/replicasets", get(handlers::list_all_replicasets))
        .route("/namespaces/:namespace/replicasets", get(handlers::list_replicasets))
        .route("/namespaces/:namespace/replicasets/:name", get(handlers::get_replicaset))
}