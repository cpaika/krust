use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};

use super::handlers;
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