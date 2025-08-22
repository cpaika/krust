use axum::http::HeaderMap;
use serde_json::{json, Value};

pub fn generate_openapi_schema() -> Value {
    let mut schema = json!({
        "swagger": "2.0",
        "info": {
            "title": "Kubernetes",
            "version": "v1.29.0"
        },
        "paths": {},
        "definitions": {}
    });
    
    // Add paths
    if let Some(paths) = schema.get_mut("paths").and_then(|v| v.as_object_mut()) {
        add_pod_paths(paths);
        add_service_paths(paths);
        add_deployment_paths(paths);
        add_replicaset_paths(paths);
    }
    
    // Add definitions
    if let Some(defs) = schema.get_mut("definitions").and_then(|v| v.as_object_mut()) {
        add_core_definitions(defs);
        add_apps_definitions(defs);
        add_meta_definitions(defs);
    }
    
    schema
}

fn add_pod_paths(paths: &mut serde_json::Map<String, Value>) {
    paths.insert("/api/v1/pods".to_string(), json!({
        "get": {
            "description": "list all pods",
            "produces": ["application/json"],
            "operationId": "listCoreV1PodForAllNamespaces",
            "responses": {
                "200": {
                    "description": "OK",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.core.v1.PodList"
                    }
                }
            }
        }
    }));
    
    paths.insert("/api/v1/namespaces/{namespace}/pods".to_string(), json!({
        "get": {
            "description": "list pods in namespace",
            "produces": ["application/json"],
            "operationId": "listCoreV1NamespacedPod",
            "parameters": [
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                }
            ],
            "responses": {
                "200": {
                    "description": "OK",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.core.v1.PodList"
                    }
                }
            }
        },
        "post": {
            "description": "create a Pod",
            "consumes": ["application/json"],
            "produces": ["application/json"],
            "operationId": "createCoreV1NamespacedPod",
            "parameters": [
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                },
                {
                    "name": "body",
                    "in": "body",
                    "required": true,
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.core.v1.Pod"
                    }
                }
            ],
            "responses": {
                "201": {
                    "description": "Created",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.core.v1.Pod"
                    }
                }
            }
        }
    }));
    
    paths.insert("/api/v1/namespaces/{namespace}/pods/{name}".to_string(), json!({
        "get": {
            "description": "read the specified Pod",
            "produces": ["application/json"],
            "operationId": "readCoreV1NamespacedPod",
            "parameters": [
                {
                    "name": "name",
                    "in": "path",
                    "required": true,
                    "type": "string"
                },
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                }
            ],
            "responses": {
                "200": {
                    "description": "OK",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.core.v1.Pod"
                    }
                }
            }
        },
        "delete": {
            "description": "delete a Pod",
            "produces": ["application/json"],
            "operationId": "deleteCoreV1NamespacedPod",
            "parameters": [
                {
                    "name": "name",
                    "in": "path",
                    "required": true,
                    "type": "string"
                },
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                }
            ],
            "responses": {
                "200": {
                    "description": "OK",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.Status"
                    }
                }
            }
        }
    }));
}

fn add_service_paths(paths: &mut serde_json::Map<String, Value>) {
    paths.insert("/api/v1/namespaces/{namespace}/services".to_string(), json!({
        "get": {
            "description": "list services in namespace",
            "produces": ["application/json"],
            "operationId": "listCoreV1NamespacedService",
            "parameters": [
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                }
            ],
            "responses": {
                "200": {
                    "description": "OK",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.core.v1.ServiceList"
                    }
                }
            }
        },
        "post": {
            "description": "create a Service",
            "consumes": ["application/json"],
            "produces": ["application/json"],
            "operationId": "createCoreV1NamespacedService",
            "parameters": [
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                },
                {
                    "name": "body",
                    "in": "body",
                    "required": true,
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.core.v1.Service"
                    }
                }
            ],
            "responses": {
                "201": {
                    "description": "Created",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.core.v1.Service"
                    }
                }
            }
        }
    }));
    
    paths.insert("/api/v1/namespaces/{namespace}/services/{name}".to_string(), json!({
        "get": {
            "description": "read the specified Service",
            "produces": ["application/json"],
            "operationId": "readCoreV1NamespacedService",
            "parameters": [
                {
                    "name": "name",
                    "in": "path",
                    "required": true,
                    "type": "string"
                },
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                }
            ],
            "responses": {
                "200": {
                    "description": "OK",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.core.v1.Service"
                    }
                }
            }
        },
        "delete": {
            "description": "delete a Service",
            "produces": ["application/json"],
            "operationId": "deleteCoreV1NamespacedService",
            "parameters": [
                {
                    "name": "name",
                    "in": "path",
                    "required": true,
                    "type": "string"
                },
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                }
            ],
            "responses": {
                "200": {
                    "description": "OK",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.Status"
                    }
                }
            }
        }
    }));
}

fn add_deployment_paths(paths: &mut serde_json::Map<String, Value>) {
    paths.insert("/apis/apps/v1/namespaces/{namespace}/deployments".to_string(), json!({
        "get": {
            "description": "list deployments in namespace",
            "produces": ["application/json"],
            "operationId": "listAppsV1NamespacedDeployment",
            "parameters": [
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                }
            ],
            "responses": {
                "200": {
                    "description": "OK",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.apps.v1.DeploymentList"
                    }
                }
            }
        },
        "post": {
            "description": "create a Deployment",
            "consumes": ["application/json"],
            "produces": ["application/json"],
            "operationId": "createAppsV1NamespacedDeployment",
            "parameters": [
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                },
                {
                    "name": "body",
                    "in": "body",
                    "required": true,
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.apps.v1.Deployment"
                    }
                }
            ],
            "responses": {
                "201": {
                    "description": "Created",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.apps.v1.Deployment"
                    }
                }
            }
        }
    }));
    
    paths.insert("/apis/apps/v1/namespaces/{namespace}/deployments/{name}".to_string(), json!({
        "get": {
            "description": "read the specified Deployment",
            "produces": ["application/json"],
            "operationId": "readAppsV1NamespacedDeployment",
            "parameters": [
                {
                    "name": "name",
                    "in": "path",
                    "required": true,
                    "type": "string"
                },
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                }
            ],
            "responses": {
                "200": {
                    "description": "OK",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.apps.v1.Deployment"
                    }
                }
            }
        },
        "delete": {
            "description": "delete a Deployment",
            "produces": ["application/json"],
            "operationId": "deleteAppsV1NamespacedDeployment",
            "parameters": [
                {
                    "name": "name",
                    "in": "path",
                    "required": true,
                    "type": "string"
                },
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                }
            ],
            "responses": {
                "200": {
                    "description": "OK",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.Status"
                    }
                }
            }
        }
    }));
}

fn add_replicaset_paths(paths: &mut serde_json::Map<String, Value>) {
    paths.insert("/apis/apps/v1/namespaces/{namespace}/replicasets".to_string(), json!({
        "get": {
            "description": "list replicasets in namespace",
            "produces": ["application/json"],
            "operationId": "listAppsV1NamespacedReplicaSet",
            "parameters": [
                {
                    "name": "namespace",
                    "in": "path",
                    "required": true,
                    "type": "string"
                }
            ],
            "responses": {
                "200": {
                    "description": "OK",
                    "schema": {
                        "$ref": "#/definitions/io.k8s.api.apps.v1.ReplicaSetList"
                    }
                }
            }
        }
    }));
}

fn add_core_definitions(defs: &mut serde_json::Map<String, Value>) {
    // Pod definitions
    defs.insert("io.k8s.api.core.v1.Pod".to_string(), json!({
        "type": "object",
        "properties": {
            "apiVersion": { "type": "string" },
            "kind": { "type": "string" },
            "metadata": { "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta" },
            "spec": { "$ref": "#/definitions/io.k8s.api.core.v1.PodSpec" },
            "status": { "$ref": "#/definitions/io.k8s.api.core.v1.PodStatus" }
        }
    }));
    
    defs.insert("io.k8s.api.core.v1.PodList".to_string(), json!({
        "type": "object",
        "required": ["items"],
        "properties": {
            "apiVersion": { "type": "string" },
            "kind": { "type": "string" },
            "metadata": { "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ListMeta" },
            "items": {
                "type": "array",
                "items": { "$ref": "#/definitions/io.k8s.api.core.v1.Pod" }
            }
        }
    }));
    
    defs.insert("io.k8s.api.core.v1.PodSpec".to_string(), json!({
        "type": "object",
        "properties": {
            "containers": {
                "type": "array",
                "items": { "$ref": "#/definitions/io.k8s.api.core.v1.Container" }
            },
            "nodeName": { "type": "string" },
            "restartPolicy": { "type": "string" },
            "serviceAccountName": { "type": "string" }
        }
    }));
    
    defs.insert("io.k8s.api.core.v1.PodStatus".to_string(), json!({
        "type": "object",
        "properties": {
            "phase": { "type": "string" },
            "hostIP": { "type": "string" },
            "podIP": { "type": "string" },
            "startTime": { "type": "string" }
        }
    }));
    
    defs.insert("io.k8s.api.core.v1.Container".to_string(), json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": { "type": "string" },
            "image": { "type": "string" },
            "ports": {
                "type": "array",
                "items": { "$ref": "#/definitions/io.k8s.api.core.v1.ContainerPort" }
            }
        }
    }));
    
    defs.insert("io.k8s.api.core.v1.ContainerPort".to_string(), json!({
        "type": "object",
        "required": ["containerPort"],
        "properties": {
            "containerPort": { "type": "integer" },
            "name": { "type": "string" },
            "protocol": { "type": "string" }
        }
    }));
    
    // Service definitions
    defs.insert("io.k8s.api.core.v1.Service".to_string(), json!({
        "type": "object",
        "properties": {
            "apiVersion": { "type": "string" },
            "kind": { "type": "string" },
            "metadata": { "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta" },
            "spec": { "$ref": "#/definitions/io.k8s.api.core.v1.ServiceSpec" },
            "status": { "$ref": "#/definitions/io.k8s.api.core.v1.ServiceStatus" }
        }
    }));
    
    defs.insert("io.k8s.api.core.v1.ServiceList".to_string(), json!({
        "type": "object",
        "required": ["items"],
        "properties": {
            "apiVersion": { "type": "string" },
            "kind": { "type": "string" },
            "metadata": { "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ListMeta" },
            "items": {
                "type": "array",
                "items": { "$ref": "#/definitions/io.k8s.api.core.v1.Service" }
            }
        }
    }));
    
    defs.insert("io.k8s.api.core.v1.ServiceSpec".to_string(), json!({
        "type": "object",
        "properties": {
            "type": { "type": "string" },
            "selector": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            },
            "ports": {
                "type": "array",
                "items": { "$ref": "#/definitions/io.k8s.api.core.v1.ServicePort" }
            },
            "clusterIP": { "type": "string" }
        }
    }));
    
    defs.insert("io.k8s.api.core.v1.ServicePort".to_string(), json!({
        "type": "object",
        "required": ["port"],
        "properties": {
            "name": { "type": "string" },
            "protocol": { "type": "string" },
            "port": { "type": "integer" },
            "targetPort": { "x-kubernetes-int-or-string": true }
        }
    }));
    
    defs.insert("io.k8s.api.core.v1.ServiceStatus".to_string(), json!({
        "type": "object",
        "properties": {}
    }));
    
    defs.insert("io.k8s.api.core.v1.PodTemplateSpec".to_string(), json!({
        "type": "object",
        "properties": {
            "metadata": { "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta" },
            "spec": { "$ref": "#/definitions/io.k8s.api.core.v1.PodSpec" }
        }
    }));
}

fn add_apps_definitions(defs: &mut serde_json::Map<String, Value>) {
    // Deployment definitions
    defs.insert("io.k8s.api.apps.v1.Deployment".to_string(), json!({
        "type": "object",
        "properties": {
            "apiVersion": { "type": "string" },
            "kind": { "type": "string" },
            "metadata": { "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta" },
            "spec": { "$ref": "#/definitions/io.k8s.api.apps.v1.DeploymentSpec" },
            "status": { "$ref": "#/definitions/io.k8s.api.apps.v1.DeploymentStatus" }
        }
    }));
    
    defs.insert("io.k8s.api.apps.v1.DeploymentList".to_string(), json!({
        "type": "object",
        "required": ["items"],
        "properties": {
            "apiVersion": { "type": "string" },
            "kind": { "type": "string" },
            "metadata": { "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ListMeta" },
            "items": {
                "type": "array",
                "items": { "$ref": "#/definitions/io.k8s.api.apps.v1.Deployment" }
            }
        }
    }));
    
    defs.insert("io.k8s.api.apps.v1.DeploymentSpec".to_string(), json!({
        "type": "object",
        "required": ["selector", "template"],
        "properties": {
            "replicas": { "type": "integer" },
            "selector": { "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.LabelSelector" },
            "template": { "$ref": "#/definitions/io.k8s.api.core.v1.PodTemplateSpec" }
        }
    }));
    
    defs.insert("io.k8s.api.apps.v1.DeploymentStatus".to_string(), json!({
        "type": "object",
        "properties": {
            "observedGeneration": { "type": "integer" },
            "replicas": { "type": "integer" },
            "updatedReplicas": { "type": "integer" },
            "readyReplicas": { "type": "integer" },
            "availableReplicas": { "type": "integer" }
        }
    }));
    
    // ReplicaSet definitions
    defs.insert("io.k8s.api.apps.v1.ReplicaSet".to_string(), json!({
        "type": "object",
        "properties": {
            "apiVersion": { "type": "string" },
            "kind": { "type": "string" },
            "metadata": { "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta" },
            "spec": { "$ref": "#/definitions/io.k8s.api.apps.v1.ReplicaSetSpec" },
            "status": { "$ref": "#/definitions/io.k8s.api.apps.v1.ReplicaSetStatus" }
        }
    }));
    
    defs.insert("io.k8s.api.apps.v1.ReplicaSetList".to_string(), json!({
        "type": "object",
        "required": ["items"],
        "properties": {
            "apiVersion": { "type": "string" },
            "kind": { "type": "string" },
            "metadata": { "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ListMeta" },
            "items": {
                "type": "array",
                "items": { "$ref": "#/definitions/io.k8s.api.apps.v1.ReplicaSet" }
            }
        }
    }));
    
    defs.insert("io.k8s.api.apps.v1.ReplicaSetSpec".to_string(), json!({
        "type": "object",
        "required": ["selector"],
        "properties": {
            "replicas": { "type": "integer" },
            "selector": { "$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.LabelSelector" },
            "template": { "$ref": "#/definitions/io.k8s.api.core.v1.PodTemplateSpec" }
        }
    }));
    
    defs.insert("io.k8s.api.apps.v1.ReplicaSetStatus".to_string(), json!({
        "type": "object",
        "properties": {
            "replicas": { "type": "integer" },
            "readyReplicas": { "type": "integer" },
            "availableReplicas": { "type": "integer" }
        }
    }));
}

fn add_meta_definitions(defs: &mut serde_json::Map<String, Value>) {
    defs.insert("io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta".to_string(), json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "namespace": { "type": "string" },
            "uid": { "type": "string" },
            "resourceVersion": { "type": "string" },
            "generation": { "type": "integer" },
            "creationTimestamp": { "type": "string" },
            "labels": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            },
            "annotations": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            }
        }
    }));
    
    defs.insert("io.k8s.apimachinery.pkg.apis.meta.v1.ListMeta".to_string(), json!({
        "type": "object",
        "properties": {
            "resourceVersion": { "type": "string" },
            "continue": { "type": "string" }
        }
    }));
    
    defs.insert("io.k8s.apimachinery.pkg.apis.meta.v1.Status".to_string(), json!({
        "type": "object",
        "properties": {
            "kind": { "type": "string" },
            "apiVersion": { "type": "string" },
            "metadata": { "type": "object" },
            "status": { "type": "string" },
            "message": { "type": "string" },
            "reason": { "type": "string" },
            "code": { "type": "integer" }
        }
    }));
    
    defs.insert("io.k8s.apimachinery.pkg.apis.meta.v1.LabelSelector".to_string(), json!({
        "type": "object",
        "properties": {
            "matchLabels": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            }
        }
    }));
}

// Check if client is requesting protobuf format
pub fn wants_protobuf(headers: &HeaderMap) -> bool {
    if let Some(accept) = headers.get("accept") {
        if let Ok(accept_str) = accept.to_str() {
            return accept_str.contains("protobuf");
        }
    }
    false
}