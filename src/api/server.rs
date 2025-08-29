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
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub storage: Storage,
    pub container_runtime: Arc<crate::runtime::container::ContainerRuntime>,
}

pub async fn start_server(storage: Storage) -> anyhow::Result<()> {
    let container_runtime = Arc::new(crate::runtime::container::ContainerRuntime::new());
    let state = AppState { 
        storage,
        container_runtime,
    };

    let app = Router::new()
        .route("/livez", get(liveness))
        .route("/readyz", get(readiness))
        .route("/healthz", get(health))
        .route("/version", get(version))
        .route("/api", get(api_versions))
        .route("/api/v1", get(api_v1_resources))
        .route("/apis", get(api_groups))
        .route("/apis/apps/v1", get(apps_v1_resources))
        .route("/apis/batch/v1", get(batch_v1_resources))
        .route("/apis/networking.k8s.io/v1", get(networking_v1_resources))
        .route("/apis/autoscaling/v2", get(autoscaling_v2_resources))
        .route("/apis/rbac.authorization.k8s.io/v1", get(rbac_v1_resources))
        .route("/apis/policy/v1", get(policy_v1_resources))
        .route("/apis/scheduling.k8s.io/v1", get(scheduling_v1_resources))
        .route("/apis/storage.k8s.io/v1", get(storage_v1_resources))
        .route("/apis/admissionregistration.k8s.io/v1", get(admissionregistration_v1_resources))
        .route("/openapi/v2", get(openapi_v2))
        .route("/swagger.json", get(openapi_v2))  // kubectl looks here too
        .route("/openapi/v3", get(openapi_v3_discovery))
        .route("/openapi/v3.0", get(openapi_v3_discovery))
        .nest("/api/v1", super::routes::v1_routes())
        .nest("/apis/apps/v1", super::routes::apps_v1_routes())
        .nest("/apis/batch/v1", super::routes::batch_v1_routes())
        .nest("/apis/networking.k8s.io/v1", super::routes::networking_v1_routes())
        .nest("/apis/autoscaling/v2", super::routes::autoscaling_v2_routes())
        .nest("/apis/rbac.authorization.k8s.io/v1", super::routes::rbac_v1_routes())
        .nest("/apis/policy/v1", super::routes::policy_v1_routes())
        .nest("/apis/scheduling.k8s.io/v1", super::routes::scheduling_v1_routes())
        .nest("/apis/storage.k8s.io/v1", super::routes::storage_v1_routes())
        .nest("/apis/admissionregistration.k8s.io/v1", super::routes::admissionregistration_v1_routes())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Bind to both IPv4 and IPv6
    let addr = "0.0.0.0:6443";
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
            },
            {
                "name": "resourcequotas",
                "singularName": "resourcequota",
                "namespaced": true,
                "kind": "ResourceQuota",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["quota"]
            },
            {
                "name": "resourcequotas/status",
                "singularName": "",
                "namespaced": true,
                "kind": "ResourceQuota",
                "verbs": ["get", "patch", "update"]
            },
            {
                "name": "limitranges",
                "singularName": "limitrange",
                "namespaced": true,
                "kind": "LimitRange",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["limits"]
            },
            {
                "name": "serviceaccounts",
                "singularName": "serviceaccount",
                "namespaced": true,
                "kind": "ServiceAccount",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["sa"]
            },
            {
                "name": "serviceaccounts/token",
                "singularName": "",
                "namespaced": true,
                "kind": "TokenRequest",
                "verbs": ["create"]
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
            },
            {
                "name": "batch",
                "versions": [
                    {
                        "groupVersion": "batch/v1",
                        "version": "v1"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "batch/v1",
                    "version": "v1"
                }
            },
            {
                "name": "networking.k8s.io",
                "versions": [
                    {
                        "groupVersion": "networking.k8s.io/v1",
                        "version": "v1"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "networking.k8s.io/v1",
                    "version": "v1"
                }
            },
            {
                "name": "autoscaling",
                "versions": [
                    {
                        "groupVersion": "autoscaling/v2",
                        "version": "v2"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "autoscaling/v2",
                    "version": "v2"
                }
            },
            {
                "name": "rbac.authorization.k8s.io",
                "versions": [
                    {
                        "groupVersion": "rbac.authorization.k8s.io/v1",
                        "version": "v1"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "rbac.authorization.k8s.io/v1",
                    "version": "v1"
                }
            },
            {
                "name": "policy",
                "versions": [
                    {
                        "groupVersion": "policy/v1",
                        "version": "v1"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "policy/v1",
                    "version": "v1"
                }
            },
            {
                "name": "scheduling.k8s.io",
                "versions": [
                    {
                        "groupVersion": "scheduling.k8s.io/v1",
                        "version": "v1"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "scheduling.k8s.io/v1",
                    "version": "v1"
                }
            },
            {
                "name": "storage.k8s.io",
                "versions": [
                    {
                        "groupVersion": "storage.k8s.io/v1",
                        "version": "v1"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "storage.k8s.io/v1",
                    "version": "v1"
                }
            },
            {
                "name": "admissionregistration.k8s.io",
                "versions": [
                    {
                        "groupVersion": "admissionregistration.k8s.io/v1",
                        "version": "v1"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "admissionregistration.k8s.io/v1",
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

async fn batch_v1_resources() -> Json<Value> {
    Json(json!({
        "kind": "APIResourceList",
        "apiVersion": "batch/v1",
        "groupVersion": "batch/v1",
        "resources": [
            {
                "name": "jobs",
                "singularName": "job",
                "namespaced": true,
                "kind": "Job",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "jobs/status",
                "singularName": "",
                "namespaced": true,
                "kind": "Job",
                "verbs": ["get", "patch", "update"]
            },
            {
                "name": "cronjobs",
                "singularName": "cronjob",
                "namespaced": true,
                "kind": "CronJob",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["cj"]
            },
            {
                "name": "cronjobs/status",
                "singularName": "",
                "namespaced": true,
                "kind": "CronJob",
                "verbs": ["get", "patch", "update"]
            }
        ]
    }))
}

async fn networking_v1_resources() -> Json<Value> {
    Json(json!({
        "kind": "APIResourceList",
        "apiVersion": "networking.k8s.io/v1",
        "groupVersion": "networking.k8s.io/v1",
        "resources": [
            {
                "name": "networkpolicies",
                "singularName": "networkpolicy",
                "namespaced": true,
                "kind": "NetworkPolicy",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["netpol"]
            },
            {
                "name": "ingresses",
                "singularName": "ingress",
                "namespaced": true,
                "kind": "Ingress",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["ing"]
            },
            {
                "name": "ingresses/status",
                "singularName": "",
                "namespaced": true,
                "kind": "Ingress",
                "verbs": ["get", "patch", "update"]
            }
        ]
    }))
}

async fn autoscaling_v2_resources() -> Json<Value> {
    Json(json!({
        "kind": "APIResourceList",
        "apiVersion": "autoscaling/v2",
        "groupVersion": "autoscaling/v2",
        "resources": [
            {
                "name": "horizontalpodautoscalers",
                "singularName": "horizontalpodautoscaler",
                "namespaced": true,
                "kind": "HorizontalPodAutoscaler",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["hpa"]
            },
            {
                "name": "horizontalpodautoscalers/status",
                "singularName": "",
                "namespaced": true,
                "kind": "HorizontalPodAutoscaler",
                "verbs": ["get", "patch", "update"]
            }
        ]
    }))
}

async fn policy_v1_resources() -> Json<Value> {
    Json(json!({
        "kind": "APIResourceList",
        "apiVersion": "policy/v1",
        "groupVersion": "policy/v1",
        "resources": [
            {
                "name": "poddisruptionbudgets",
                "singularName": "poddisruptionbudget",
                "namespaced": true,
                "kind": "PodDisruptionBudget",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["pdb"]
            },
            {
                "name": "poddisruptionbudgets/status",
                "singularName": "",
                "namespaced": true,
                "kind": "PodDisruptionBudget",
                "verbs": ["get", "patch", "update"]
            }
        ]
    }))
}

async fn rbac_v1_resources() -> Json<Value> {
    Json(json!({
        "kind": "APIResourceList",
        "apiVersion": "rbac.authorization.k8s.io/v1",
        "groupVersion": "rbac.authorization.k8s.io/v1",
        "resources": [
            {
                "name": "roles",
                "singularName": "role",
                "namespaced": true,
                "kind": "Role",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "rolebindings",
                "singularName": "rolebinding",
                "namespaced": true,
                "kind": "RoleBinding",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "clusterroles",
                "singularName": "clusterrole",
                "namespaced": false,
                "kind": "ClusterRole",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "clusterrolebindings",
                "singularName": "clusterrolebinding",
                "namespaced": false,
                "kind": "ClusterRoleBinding",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            }
        ]
    }))
}

async fn scheduling_v1_resources() -> Json<Value> {
    Json(json!({
        "kind": "APIResourceList",
        "apiVersion": "scheduling.k8s.io/v1",
        "groupVersion": "scheduling.k8s.io/v1",
        "resources": [
            {
                "name": "priorityclasses",
                "singularName": "priorityclass",
                "namespaced": false,
                "kind": "PriorityClass",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["pc"]
            }
        ]
    }))
}

async fn storage_v1_resources() -> Json<Value> {
    Json(json!({
        "kind": "APIResourceList",
        "apiVersion": "storage.k8s.io/v1",
        "groupVersion": "storage.k8s.io/v1",
        "resources": [
            {
                "name": "storageclasses",
                "singularName": "storageclass",
                "namespaced": false,
                "kind": "StorageClass",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"],
                "shortNames": ["sc"]
            }
        ]
    }))
}

async fn admissionregistration_v1_resources() -> Json<Value> {
    Json(json!({
        "kind": "APIResourceList",
        "apiVersion": "admissionregistration.k8s.io/v1",
        "groupVersion": "admissionregistration.k8s.io/v1",
        "resources": [
            {
                "name": "validatingwebhookconfigurations",
                "singularName": "validatingwebhookconfiguration",
                "namespaced": false,
                "kind": "ValidatingWebhookConfiguration",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "mutatingwebhookconfigurations",
                "singularName": "mutatingwebhookconfiguration",
                "namespaced": false,
                "kind": "MutatingWebhookConfiguration",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
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