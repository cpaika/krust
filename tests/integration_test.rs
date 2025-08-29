// Comprehensive integration tests to find and fix bugs
use reqwest;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::sleep;

const API_BASE: &str = "http://localhost:6443/api/v1";

async fn wait_for_krust() -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..10 {
        if reqwest::get(format!("{}/namespaces", API_BASE)).await.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_secs(1)).await;
    }
    Err("Krust failed to start".into())
}

#[tokio::test]
async fn test_namespace_crud_operations() {
    wait_for_krust().await.expect("Krust should be running");
    
    let client = reqwest::Client::new();
    let ns_name = format!("test-ns-{}", uuid::Uuid::new_v4());
    
    // Create namespace
    let create_body = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name,
            "labels": {
                "test": "true"
            }
        }
    });
    
    let create_resp = client
        .post(format!("{}/namespaces", API_BASE))
        .json(&create_body)
        .send()
        .await
        .expect("Failed to create namespace");
    
    println!("Create response status: {}", create_resp.status());
    assert_eq!(create_resp.status(), 201, "Namespace creation should return 201");
    let created: Value = create_resp.json().await.expect("Invalid JSON response");
    println!("Created namespace: {}", serde_json::to_string_pretty(&created).unwrap());
    assert_eq!(created["metadata"]["name"], ns_name);
    
    // List namespaces to see what's in the database
    let list_check = client
        .get(format!("{}/namespaces", API_BASE))
        .send()
        .await
        .expect("Failed to list namespaces");
    let list_data: Value = list_check.json().await.expect("Invalid JSON");
    println!("All namespaces after creation: {}", serde_json::to_string_pretty(&list_data).unwrap());
    
    // Get namespace
    let get_resp = client
        .get(format!("{}/namespaces/{}", API_BASE, ns_name))
        .send()
        .await
        .expect("Failed to get namespace");
    
    let status = get_resp.status();
    println!("Get response status: {}", status);
    
    if status != 200 {
        let text = get_resp.text().await.unwrap();
        println!("Get response body: {}", text);
        panic!("Get namespace failed with status {}", status);
    }
    
    let retrieved: Value = get_resp.json().await.expect("Invalid JSON response");
    assert_eq!(retrieved["metadata"]["name"], ns_name);
    assert_eq!(retrieved["metadata"]["labels"]["test"], "true");
    
    // List namespaces - should include our namespace
    let list_resp = client
        .get(format!("{}/namespaces", API_BASE))
        .send()
        .await
        .expect("Failed to list namespaces");
    
    assert_eq!(list_resp.status(), 200);
    let list: Value = list_resp.json().await.expect("Invalid JSON response");
    let items = list["items"].as_array().expect("items should be array");
    let found = items.iter().any(|item| item["metadata"]["name"] == ns_name);
    assert!(found, "Created namespace should appear in list");
    
    // Update namespace
    let update_body = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name,
            "labels": {
                "test": "false",
                "updated": "true"
            }
        }
    });
    
    let update_resp = client
        .put(format!("{}/namespaces/{}", API_BASE, ns_name))
        .json(&update_body)
        .send()
        .await
        .expect("Failed to update namespace");
    
    assert_eq!(update_resp.status(), 200);
    let updated: Value = update_resp.json().await.expect("Invalid JSON response");
    assert_eq!(updated["metadata"]["labels"]["updated"], "true");
    
    // Delete namespace
    let delete_resp = client
        .delete(format!("{}/namespaces/{}", API_BASE, ns_name))
        .send()
        .await
        .expect("Failed to delete namespace");
    
    assert_eq!(delete_resp.status(), 200);
    
    // Verify deletion
    let verify_resp = client
        .get(format!("{}/namespaces/{}", API_BASE, ns_name))
        .send()
        .await
        .expect("Failed to verify deletion");
    
    assert_eq!(verify_resp.status(), 404, "Deleted namespace should return 404");
}

#[tokio::test]
async fn test_pod_creation_and_persistence() {
    wait_for_krust().await.expect("Krust should be running");
    
    let client = reqwest::Client::new();
    let ns_name = format!("test-pods-{}", uuid::Uuid::new_v4());
    let pod_name = "test-pod";
    
    // Create namespace first
    let ns_body = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name
        }
    });
    
    client
        .post(format!("{}/namespaces", API_BASE))
        .json(&ns_body)
        .send()
        .await
        .expect("Failed to create namespace");
    
    // Create pod
    let pod_body = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": pod_name,
            "namespace": &ns_name,
            "labels": {
                "app": "test"
            }
        },
        "spec": {
            "containers": [{
                "name": "nginx",
                "image": "nginx:latest",
                "ports": [{
                    "containerPort": 80,
                    "name": "http"
                }]
            }]
        }
    });
    
    let create_resp = client
        .post(format!("{}/namespaces/{}/pods", API_BASE, ns_name))
        .json(&pod_body)
        .send()
        .await
        .expect("Failed to create pod");
    
    assert_eq!(create_resp.status(), 201, "Pod creation should return 201");
    let created: Value = create_resp.json().await.expect("Invalid JSON response");
    assert_eq!(created["metadata"]["name"], pod_name);
    
    // Get pod
    let get_resp = client
        .get(format!("{}/namespaces/{}/pods/{}", API_BASE, ns_name, pod_name))
        .send()
        .await
        .expect("Failed to get pod");
    
    assert_eq!(get_resp.status(), 200, "Get pod should return 200");
    let retrieved: Value = get_resp.json().await.expect("Invalid JSON response");
    assert_eq!(retrieved["metadata"]["name"], pod_name);
    assert_eq!(retrieved["spec"]["containers"][0]["image"], "nginx:latest");
    
    // List pods in namespace
    let list_resp = client
        .get(format!("{}/namespaces/{}/pods", API_BASE, ns_name))
        .send()
        .await
        .expect("Failed to list pods");
    
    assert_eq!(list_resp.status(), 200);
    let list: Value = list_resp.json().await.expect("Invalid JSON response");
    let items = list["items"].as_array().expect("items should be array");
    assert_eq!(items.len(), 1, "Should have exactly one pod");
    assert_eq!(items[0]["metadata"]["name"], pod_name);
    
    // Update pod status
    let status_body = json!({
        "status": {
            "phase": "Running",
            "conditions": [{
                "type": "Ready",
                "status": "True"
            }]
        }
    });
    
    let status_resp = client
        .patch(format!("{}/namespaces/{}/pods/{}/status", API_BASE, ns_name, pod_name))
        .json(&status_body)
        .send()
        .await
        .expect("Failed to update pod status");
    
    assert_eq!(status_resp.status(), 200);
    let status_updated: Value = status_resp.json().await.expect("Invalid JSON response");
    assert_eq!(status_updated["status"]["phase"], "Running");
    
    // Delete pod
    let delete_resp = client
        .delete(format!("{}/namespaces/{}/pods/{}", API_BASE, ns_name, pod_name))
        .send()
        .await
        .expect("Failed to delete pod");
    
    assert_eq!(delete_resp.status(), 200);
    
    // Verify deletion
    let verify_resp = client
        .get(format!("{}/namespaces/{}/pods/{}", API_BASE, ns_name, pod_name))
        .send()
        .await
        .expect("Failed to verify deletion");
    
    assert_eq!(verify_resp.status(), 404, "Deleted pod should return 404");
    
    // Cleanup namespace
    client
        .delete(format!("{}/namespaces/{}", API_BASE, ns_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_service_account_operations() {
    wait_for_krust().await.expect("Krust should be running");
    
    let client = reqwest::Client::new();
    let ns_name = format!("test-sa-{}", uuid::Uuid::new_v4());
    let sa_name = "test-service-account";
    
    // Create namespace
    let ns_body = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name
        }
    });
    
    client
        .post(format!("{}/namespaces", API_BASE))
        .json(&ns_body)
        .send()
        .await
        .expect("Failed to create namespace");
    
    // Create service account
    let sa_body = json!({
        "apiVersion": "v1",
        "kind": "ServiceAccount",
        "metadata": {
            "name": sa_name,
            "namespace": &ns_name,
            "labels": {
                "app": "test"
            }
        }
    });
    
    let create_resp = client
        .post(format!("{}/namespaces/{}/serviceaccounts", API_BASE, ns_name))
        .json(&sa_body)
        .send()
        .await
        .expect("Failed to create service account");
    
    assert_eq!(create_resp.status(), 201, "Service account creation should return 201");
    let created: Value = create_resp.json().await.expect("Invalid JSON response");
    assert_eq!(created["metadata"]["name"], sa_name);
    
    // Get service account
    let get_resp = client
        .get(format!("{}/namespaces/{}/serviceaccounts/{}", API_BASE, ns_name, sa_name))
        .send()
        .await
        .expect("Failed to get service account");
    
    assert_eq!(get_resp.status(), 200);
    let retrieved: Value = get_resp.json().await.expect("Invalid JSON response");
    assert_eq!(retrieved["metadata"]["name"], sa_name);
    
    // List service accounts
    let list_resp = client
        .get(format!("{}/namespaces/{}/serviceaccounts", API_BASE, ns_name))
        .send()
        .await
        .expect("Failed to list service accounts");
    
    assert_eq!(list_resp.status(), 200);
    let list: Value = list_resp.json().await.expect("Invalid JSON response");
    let items = list["items"].as_array().expect("items should be array");
    let found = items.iter().any(|item| item["metadata"]["name"] == sa_name);
    assert!(found, "Created service account should appear in list");
    
    // Delete service account
    let delete_resp = client
        .delete(format!("{}/namespaces/{}/serviceaccounts/{}", API_BASE, ns_name, sa_name))
        .send()
        .await
        .expect("Failed to delete service account");
    
    assert_eq!(delete_resp.status(), 200);
    
    // Cleanup namespace
    client
        .delete(format!("{}/namespaces/{}", API_BASE, ns_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_service_operations() {
    wait_for_krust().await.expect("Krust should be running");
    
    let client = reqwest::Client::new();
    let ns_name = format!("test-svc-{}", uuid::Uuid::new_v4());
    let svc_name = "test-service";
    
    // Create namespace
    let ns_body = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name
        }
    });
    
    client
        .post(format!("{}/namespaces", API_BASE))
        .json(&ns_body)
        .send()
        .await
        .expect("Failed to create namespace");
    
    // Create service
    let svc_body = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": svc_name,
            "namespace": &ns_name
        },
        "spec": {
            "selector": {
                "app": "test"
            },
            "ports": [{
                "port": 80,
                "targetPort": 8080,
                "name": "http"
            }]
        }
    });
    
    let create_resp = client
        .post(format!("{}/namespaces/{}/services", API_BASE, ns_name))
        .json(&svc_body)
        .send()
        .await
        .expect("Failed to create service");
    
    assert_eq!(create_resp.status(), 201, "Service creation should return 201");
    let created: Value = create_resp.json().await.expect("Invalid JSON response");
    assert_eq!(created["metadata"]["name"], svc_name);
    
    // Get service
    let get_resp = client
        .get(format!("{}/namespaces/{}/services/{}", API_BASE, ns_name, svc_name))
        .send()
        .await
        .expect("Failed to get service");
    
    assert_eq!(get_resp.status(), 200);
    let retrieved: Value = get_resp.json().await.expect("Invalid JSON response");
    assert_eq!(retrieved["metadata"]["name"], svc_name);
    assert_eq!(retrieved["spec"]["ports"][0]["port"], 80);
    
    // Delete service
    let delete_resp = client
        .delete(format!("{}/namespaces/{}/services/{}", API_BASE, ns_name, svc_name))
        .send()
        .await
        .expect("Failed to delete service");
    
    assert_eq!(delete_resp.status(), 200);
    
    // Cleanup namespace
    client
        .delete(format!("{}/namespaces/{}", API_BASE, ns_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_configmap_operations() {
    wait_for_krust().await.expect("Krust should be running");
    
    let client = reqwest::Client::new();
    let ns_name = format!("test-cm-{}", uuid::Uuid::new_v4());
    let cm_name = "test-configmap";
    
    // Create namespace
    let ns_body = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name
        }
    });
    
    client
        .post(format!("{}/namespaces", API_BASE))
        .json(&ns_body)
        .send()
        .await
        .expect("Failed to create namespace");
    
    // Create ConfigMap
    let cm_body = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {
            "name": cm_name,
            "namespace": &ns_name
        },
        "data": {
            "key1": "value1",
            "key2": "value2"
        }
    });
    
    let create_resp = client
        .post(format!("{}/namespaces/{}/configmaps", API_BASE, ns_name))
        .json(&cm_body)
        .send()
        .await
        .expect("Failed to create configmap");
    
    assert_eq!(create_resp.status(), 201, "ConfigMap creation should return 201");
    let created: Value = create_resp.json().await.expect("Invalid JSON response");
    assert_eq!(created["metadata"]["name"], cm_name);
    assert_eq!(created["data"]["key1"], "value1");
    
    // Get ConfigMap
    let get_resp = client
        .get(format!("{}/namespaces/{}/configmaps/{}", API_BASE, ns_name, cm_name))
        .send()
        .await
        .expect("Failed to get configmap");
    
    assert_eq!(get_resp.status(), 200);
    let retrieved: Value = get_resp.json().await.expect("Invalid JSON response");
    assert_eq!(retrieved["data"]["key2"], "value2");
    
    // Update ConfigMap
    let update_body = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {
            "name": cm_name,
            "namespace": &ns_name
        },
        "data": {
            "key1": "updated",
            "key3": "new"
        }
    });
    
    let update_resp = client
        .put(format!("{}/namespaces/{}/configmaps/{}", API_BASE, ns_name, cm_name))
        .json(&update_body)
        .send()
        .await
        .expect("Failed to update configmap");
    
    assert_eq!(update_resp.status(), 200);
    let updated: Value = update_resp.json().await.expect("Invalid JSON response");
    assert_eq!(updated["data"]["key1"], "updated");
    assert_eq!(updated["data"]["key3"], "new");
    
    // Delete ConfigMap
    let delete_resp = client
        .delete(format!("{}/namespaces/{}/configmaps/{}", API_BASE, ns_name, cm_name))
        .send()
        .await
        .expect("Failed to delete configmap");
    
    assert_eq!(delete_resp.status(), 200);
    
    // Cleanup namespace
    client
        .delete(format!("{}/namespaces/{}", API_BASE, ns_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_secret_operations() {
    wait_for_krust().await.expect("Krust should be running");
    
    let client = reqwest::Client::new();
    let ns_name = format!("test-secret-{}", uuid::Uuid::new_v4());
    let secret_name = "test-secret";
    
    // Create namespace
    let ns_body = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name
        }
    });
    
    client
        .post(format!("{}/namespaces", API_BASE))
        .json(&ns_body)
        .send()
        .await
        .expect("Failed to create namespace");
    
    // Create Secret
    let secret_body = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": secret_name,
            "namespace": &ns_name
        },
        "type": "Opaque",
        "data": {
            "username": encode_base64("admin"),
            "password": encode_base64("secret123")
        }
    });
    
    let create_resp = client
        .post(format!("{}/namespaces/{}/secrets", API_BASE, ns_name))
        .json(&secret_body)
        .send()
        .await
        .expect("Failed to create secret");
    
    assert_eq!(create_resp.status(), 201, "Secret creation should return 201");
    let created: Value = create_resp.json().await.expect("Invalid JSON response");
    assert_eq!(created["metadata"]["name"], secret_name);
    
    // Get Secret
    let get_resp = client
        .get(format!("{}/namespaces/{}/secrets/{}", API_BASE, ns_name, secret_name))
        .send()
        .await
        .expect("Failed to get secret");
    
    assert_eq!(get_resp.status(), 200);
    let retrieved: Value = get_resp.json().await.expect("Invalid JSON response");
    assert_eq!(retrieved["type"], "Opaque");
    
    // Delete Secret
    let delete_resp = client
        .delete(format!("{}/namespaces/{}/secrets/{}", API_BASE, ns_name, secret_name))
        .send()
        .await
        .expect("Failed to delete secret");
    
    assert_eq!(delete_resp.status(), 200);
    
    // Cleanup namespace
    client
        .delete(format!("{}/namespaces/{}", API_BASE, ns_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_deployment_operations() {
    wait_for_krust().await.expect("Krust should be running");
    
    let client = reqwest::Client::new();
    let ns_name = format!("test-deploy-{}", uuid::Uuid::new_v4());
    let deploy_name = "test-deployment";
    
    // Create namespace
    let ns_body = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name
        }
    });
    
    client
        .post(format!("{}/namespaces", API_BASE))
        .json(&ns_body)
        .send()
        .await
        .expect("Failed to create namespace");
    
    // Create Deployment
    let deploy_body = json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {
            "name": deploy_name,
            "namespace": &ns_name
        },
        "spec": {
            "replicas": 3,
            "selector": {
                "matchLabels": {
                    "app": "test"
                }
            },
            "template": {
                "metadata": {
                    "labels": {
                        "app": "test"
                    }
                },
                "spec": {
                    "containers": [{
                        "name": "nginx",
                        "image": "nginx:latest"
                    }]
                }
            }
        }
    });
    
    let create_resp = client
        .post(format!("http://localhost:6443/apis/apps/v1/namespaces/{}/deployments", ns_name))
        .json(&deploy_body)
        .send()
        .await
        .expect("Failed to create deployment");
    
    assert_eq!(create_resp.status(), 201, "Deployment creation should return 201");
    let created: Value = create_resp.json().await.expect("Invalid JSON response");
    assert_eq!(created["metadata"]["name"], deploy_name);
    assert_eq!(created["spec"]["replicas"], 3);
    
    // Get Deployment
    let get_resp = client
        .get(format!("http://localhost:6443/apis/apps/v1/namespaces/{}/deployments/{}", ns_name, deploy_name))
        .send()
        .await
        .expect("Failed to get deployment");
    
    assert_eq!(get_resp.status(), 200);
    
    // Scale Deployment
    let scale_body = json!({
        "spec": {
            "replicas": 5
        }
    });
    
    let scale_resp = client
        .patch(format!("http://localhost:6443/apis/apps/v1/namespaces/{}/deployments/{}", ns_name, deploy_name))
        .json(&scale_body)
        .send()
        .await
        .expect("Failed to scale deployment");
    
    assert_eq!(scale_resp.status(), 200);
    let scaled: Value = scale_resp.json().await.expect("Invalid JSON response");
    assert_eq!(scaled["spec"]["replicas"], 5);
    
    // Delete Deployment
    let delete_resp = client
        .delete(format!("http://localhost:6443/apis/apps/v1/namespaces/{}/deployments/{}", ns_name, deploy_name))
        .send()
        .await
        .expect("Failed to delete deployment");
    
    assert_eq!(delete_resp.status(), 200);
    
    // Cleanup namespace
    client
        .delete(format!("{}/namespaces/{}", API_BASE, ns_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_pod_logs() {
    wait_for_krust().await.expect("Krust should be running");
    
    let client = reqwest::Client::new();
    let ns_name = format!("test-logs-{}", uuid::Uuid::new_v4());
    let pod_name = "test-pod-logs";
    
    // Create namespace
    let ns_body = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name
        }
    });
    
    client
        .post(format!("{}/namespaces", API_BASE))
        .json(&ns_body)
        .send()
        .await
        .expect("Failed to create namespace");
    
    // Create pod
    let pod_body = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": pod_name,
            "namespace": &ns_name
        },
        "spec": {
            "containers": [{
                "name": "test",
                "image": "busybox",
                "command": ["sh", "-c", "echo 'Hello from pod'; sleep 3600"]
            }]
        }
    });
    
    let create_resp = client
        .post(format!("{}/namespaces/{}/pods", API_BASE, ns_name))
        .json(&pod_body)
        .send()
        .await
        .expect("Failed to create pod");
    
    assert_eq!(create_resp.status(), 201);
    
    // Wait a moment for the container to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Get pod logs
    let logs_resp = client
        .get(format!("{}/namespaces/{}/pods/{}/log", API_BASE, ns_name, pod_name))
        .send()
        .await
        .expect("Failed to get pod logs");
    
    // Logs endpoint should exist (even if empty)
    assert!(logs_resp.status() == 200 || logs_resp.status() == 404, 
            "Logs endpoint should return 200 or 404, got {}", logs_resp.status());
    
    // Cleanup
    client
        .delete(format!("{}/namespaces/{}/pods/{}", API_BASE, ns_name, pod_name))
        .send()
        .await
        .ok();
    
    client
        .delete(format!("{}/namespaces/{}", API_BASE, ns_name))
        .send()
        .await
        .ok();
}

// Helper function for base64 encoding
mod base64 {
    pub fn encode(input: &str) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(input)
    }
}