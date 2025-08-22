use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get},
    Router,
};
use serde_json::{json, Value};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::Storage;

#[derive(Clone)]
pub struct AppState {
    pub storage: Storage,
}

pub async fn start_server(storage: Storage) -> anyhow::Result<()> {
    let state = AppState { storage };

    let app = Router::new()
        .route("/livez", get(liveness))
        .route("/readyz", get(readiness))
        .route("/healthz", get(health))
        .route("/version", get(version))
        .route("/api", get(api_versions))
        .route("/api/v1", get(api_v1_resources))
        .route("/apis", get(api_groups))
        .route("/apis/apps/v1", get(apps_v1_resources))
        .route("/openapi/v2", get(openapi_v2))
        .route("/swagger.json", get(openapi_v2))  // kubectl looks here too
        .route("/openapi/v3", get(openapi_v3_discovery))
        .route("/openapi/v3.0", get(openapi_v3_discovery))
        .nest("/api/v1", super::routes::v1_routes())
        .nest("/apis/apps/v1", super::routes::apps_v1_routes())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 6443));
    tracing::info!("Krust API server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn liveness() -> StatusCode {
    StatusCode::OK
}

async fn readiness(State(state): State<AppState>) -> StatusCode {
    match sqlx::query("SELECT 1").fetch_one(&*state.storage.pool).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn health() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}

async fn version() -> Json<Value> {
    Json(json!({
        "major": "1",
        "minor": "29",
        "gitVersion": "v1.29.0-krust",
        "gitCommit": "000000",
        "gitTreeState": "clean",
        "buildDate": chrono::Utc::now().to_rfc3339(),
        "goVersion": "rust1.75",
        "compiler": "rustc",
        "platform": "linux/amd64"
    }))
}

async fn api_versions() -> Json<Value> {
    Json(json!({
        "kind": "APIVersions",
        "versions": ["v1"],
        "serverAddressByClientCIDRs": [{
            "clientCIDR": "0.0.0.0/0",
            "serverAddress": "127.0.0.1:6443"
        }]
    }))
}

async fn api_v1_resources() -> Json<Value> {
    Json(json!({
        "kind": "APIResourceList",
        "groupVersion": "v1",
        "resources": [
            {
                "name": "namespaces",
                "singularName": "namespace",
                "namespaced": false,
                "kind": "Namespace",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["ns"]
            },
            {
                "name": "pods",
                "singularName": "pod",
                "namespaced": true,
                "kind": "Pod",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["po"]
            },
            {
                "name": "services",
                "singularName": "service",
                "namespaced": true,
                "kind": "Service",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["svc"]
            },
            {
                "name": "endpoints",
                "singularName": "endpoints",
                "namespaced": true,
                "kind": "Endpoints",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["ep"]
            },
            {
                "name": "nodes",
                "singularName": "node",
                "namespaced": false,
                "kind": "Node",
                "verbs": ["get", "list", "patch", "update", "watch"],
                "shortNames": ["no"]
            }
        ]
    }))
}

async fn api_groups() -> Json<Value> {
    Json(json!({
        "kind": "APIGroupList",
        "apiVersion": "v1",
        "groups": [
            {
                "name": "apps",
                "versions": [
                    {
                        "groupVersion": "apps/v1",
                        "version": "v1"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "apps/v1",
                    "version": "v1"
                }
            }
        ]
    }))
}

async fn apps_v1_resources() -> Json<Value> {
    Json(json!({
        "kind": "APIResourceList",
        "apiVersion": "apps/v1",
        "groupVersion": "apps/v1",
        "resources": [
            {
                "name": "deployments",
                "singularName": "deployment",
                "namespaced": true,
                "kind": "Deployment",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["deploy"]
            },
            {
                "name": "replicasets",
                "singularName": "replicaset",
                "namespaced": true,
                "kind": "ReplicaSet",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["rs"]
            },
            {
                "name": "statefulsets",
                "singularName": "statefulset",
                "namespaced": true,
                "kind": "StatefulSet",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["sts"]
            },
            {
                "name": "daemonsets",
                "singularName": "daemonset",
                "namespaced": true,
                "kind": "DaemonSet",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["ds"]
            }
        ]
    }))
}

async fn openapi_v2(headers: HeaderMap) -> Response<Body> {
    let json = super::openapi::generate_openapi_schema();
    
    // Check if client wants protobuf
    if super::openapi::wants_protobuf(&headers) {
        // Return proper protobuf format
        match super::openapi_proto_v2::json_to_protobuf_v2(&json) {
            Ok(protobuf_data) => {
                return Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/octet-stream")
                    .body(Body::from(protobuf_data))
                    .unwrap();
            }
            Err(e) => {
                tracing::warn!("Failed to encode OpenAPI as protobuf: {}", e);
                // Fall back to JSON
            }
        }
    }
    
    // Return JSON as fallback
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&json).unwrap()))
        .unwrap()
}

async fn openapi_v3_discovery() -> Json<Value> {
    Json(json!({
        "paths": {
            "v3": "/openapi/v3",
            "v3.0": "/openapi/v3.0"
        }
    }))
}