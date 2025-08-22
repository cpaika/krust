use reqwest;
use serde_json::{json, Value};
use serial_test::serial;

const BASE_URL: &str = "http://localhost:6443";

async fn ensure_server_running() -> bool {
    let client = reqwest::Client::new();
    match client.get(&format!("{}/healthz", BASE_URL)).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false
    }
}

#[tokio::test]
#[serial]
async fn test_pod_lifecycle() {
    if !ensure_server_running().await {
        eprintln!("Server not running, skipping integration test");
        return;
    }
    
    let client = reqwest::Client::new();
    
    // Create a pod
    let pod = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "test-pod",
            "labels": {
                "app": "test"
            }
        },
        "spec": {
            "containers": [
                {
                    "name": "nginx",
                    "image": "nginx:latest",
                    "ports": [
                        {
                            "containerPort": 80
                        }
                    ]
                }
            ]
        }
    });
    
    let resp = client
        .post(&format!("{}/api/v1/namespaces/default/pods", BASE_URL))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    assert_eq!(resp.status(), 201);
    let created_pod: Value = resp.json().await.unwrap();
    assert_eq!(created_pod["metadata"]["name"], "test-pod");
    assert_eq!(created_pod["metadata"]["namespace"], "default");
    assert!(created_pod["metadata"]["uid"].is_string());
    assert!(created_pod["metadata"]["creationTimestamp"].is_string());
    assert_eq!(created_pod["status"]["phase"], "Pending");
    
    // Get the pod
    let resp = client
        .get(&format!("{}/api/v1/namespaces/default/pods/test-pod", BASE_URL))
        .send()
        .await
        .unwrap();
    
    assert_eq!(resp.status(), 200);
    let fetched_pod: Value = resp.json().await.unwrap();
    assert_eq!(fetched_pod["metadata"]["name"], "test-pod");
    assert_eq!(fetched_pod["metadata"]["uid"], created_pod["metadata"]["uid"]);
    
    // List pods
    let resp = client
        .get(&format!("{}/api/v1/namespaces/default/pods", BASE_URL))
        .send()
        .await
        .unwrap();
    
    assert_eq!(resp.status(), 200);
    let pod_list: Value = resp.json().await.unwrap();
    assert_eq!(pod_list["kind"], "PodList");
    assert!(pod_list["items"].as_array().unwrap().len() >= 1);
    
    // Update the pod
    let mut updated_pod = created_pod.clone();
    updated_pod["metadata"]["labels"]["environment"] = json!("test");
    
    let resp = client
        .put(&format!("{}/api/v1/namespaces/default/pods/test-pod", BASE_URL))
        .json(&updated_pod)
        .send()
        .await
        .unwrap();
    
    assert_eq!(resp.status(), 200);
    let result: Value = resp.json().await.unwrap();
    assert_eq!(result["metadata"]["labels"]["environment"], "test");
    
    // Patch the pod
    let patch = json!({
        "metadata": {
            "annotations": {
                "test": "annotation"
            }
        }
    });
    
    let resp = client
        .patch(&format!("{}/api/v1/namespaces/default/pods/test-pod", BASE_URL))
        .json(&patch)
        .send()
        .await
        .unwrap();
    
    assert_eq!(resp.status(), 200);
    let patched_pod: Value = resp.json().await.unwrap();
    assert_eq!(patched_pod["metadata"]["annotations"]["test"], "annotation");
    
    // Delete the pod
    let resp = client
        .delete(&format!("{}/api/v1/namespaces/default/pods/test-pod", BASE_URL))
        .send()
        .await
        .unwrap();
    
    assert_eq!(resp.status(), 200);
    
    // Verify pod is deleted
    let resp = client
        .get(&format!("{}/api/v1/namespaces/default/pods/test-pod", BASE_URL))
        .send()
        .await
        .unwrap();
    
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
#[serial]
async fn test_pod_not_found() {
    if !ensure_server_running().await {
        eprintln!("Server not running, skipping integration test");
        return;
    }
    
    let client = reqwest::Client::new();
    
    let resp = client
        .get(&format!("{}/api/v1/namespaces/default/pods/nonexistent", BASE_URL))
        .send()
        .await
        .unwrap();
    
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
#[serial]
async fn test_list_all_pods() {
    if !ensure_server_running().await {
        eprintln!("Server not running, skipping integration test");
        return;
    }
    
    let client = reqwest::Client::new();
    
    // Create pods in different namespaces
    let pod1 = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "pod-default"
        },
        "spec": {
            "containers": [
                {
                    "name": "nginx",
                    "image": "nginx"
                }
            ]
        }
    });
    
    client
        .post(&format!("{}/api/v1/namespaces/default/pods", BASE_URL))
        .json(&pod1)
        .send()
        .await
        .unwrap();
    
    // List all pods
    let resp = client
        .get(&format!("{}/api/v1/pods", BASE_URL))
        .send()
        .await
        .unwrap();
    
    assert_eq!(resp.status(), 200);
    let pod_list: Value = resp.json().await.unwrap();
    assert_eq!(pod_list["kind"], "PodList");
    assert!(pod_list["items"].as_array().unwrap().len() >= 1);
    
    // Clean up
    client
        .delete(&format!("{}/api/v1/namespaces/default/pods/pod-default", BASE_URL))
        .send()
        .await
        .unwrap();
}