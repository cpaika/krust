use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_replicaset_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/apps/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Test 1: Create a ReplicaSet
    let replicaset = json!({
        "apiVersion": "apps/v1",
        "kind": "ReplicaSet",
        "metadata": {
            "name": "test-replicaset",
            "namespace": "default",
            "labels": {
                "app": "test"
            }
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
                        "image": "nginx:alpine"
                    }]
                }
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/replicasets", base_url))
        .json(&replicaset)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 201, "ReplicaSet creation should return 201");
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-replicaset");
    assert_eq!(created["spec"]["replicas"], 3);
    
    // Test 2: Get the ReplicaSet
    let response = client
        .get(&format!("{}/namespaces/default/replicasets/test-replicaset", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let fetched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(fetched["metadata"]["name"], "test-replicaset");
    
    // Test 3: List ReplicaSets
    let response = client
        .get(&format!("{}/namespaces/default/replicasets", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["kind"], "ReplicaSetList");
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Test 4: Update the ReplicaSet
    let mut updated_replicaset = created.clone();
    updated_replicaset["spec"]["replicas"] = json!(5);
    updated_replicaset["metadata"]["labels"]["environment"] = json!("test");
    
    let response = client
        .put(&format!("{}/namespaces/default/replicasets/test-replicaset", base_url))
        .json(&updated_replicaset)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["spec"]["replicas"], 5);
    assert_eq!(updated["metadata"]["labels"]["environment"], "test");
    
    // Test 5: Patch the ReplicaSet
    let patch = json!({
        "spec": {
            "replicas": 2
        }
    });
    
    let response = client
        .patch(&format!("{}/namespaces/default/replicasets/test-replicaset", base_url))
        .header("Content-Type", "application/merge-patch+json")
        .json(&patch)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let patched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(patched["spec"]["replicas"], 2);
    
    // Test 6: Scale subresource
    let scale = json!({
        "apiVersion": "autoscaling/v1",
        "kind": "Scale",
        "metadata": {
            "name": "test-replicaset",
            "namespace": "default"
        },
        "spec": {
            "replicas": 4
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/replicasets/test-replicaset/scale", base_url))
        .json(&scale)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let scale_result: serde_json::Value = response.json().await.unwrap();
    assert_eq!(scale_result["spec"]["replicas"], 4);
    
    // Test 7: Get scale subresource
    let response = client
        .get(&format!("{}/namespaces/default/replicasets/test-replicaset/scale", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let scale_get: serde_json::Value = response.json().await.unwrap();
    assert_eq!(scale_get["spec"]["replicas"], 4);
    assert_eq!(scale_get["kind"], "Scale");
    
    // Test 8: Status subresource
    let status_update = json!({
        "status": {
            "replicas": 4,
            "readyReplicas": 3,
            "availableReplicas": 3,
            "observedGeneration": 2,
            "conditions": [{
                "type": "ReplicaFailure",
                "status": "False",
                "lastTransitionTime": chrono::Utc::now().to_rfc3339(),
                "reason": "ReplicasAvailable",
                "message": "ReplicaSet has successfully progressed."
            }]
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/replicasets/test-replicaset/status", base_url))
        .json(&status_update)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let status_result: serde_json::Value = response.json().await.unwrap();
    assert_eq!(status_result["status"]["readyReplicas"], 3);
    
    // Test 9: Delete the ReplicaSet
    let response = client
        .delete(&format!("{}/namespaces/default/replicasets/test-replicaset", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Verify it's deleted
    let response = client
        .get(&format!("{}/namespaces/default/replicasets/test-replicaset", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_replicaset_owner_references() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/apps/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a ReplicaSet with owner references
    let replicaset = json!({
        "apiVersion": "apps/v1",
        "kind": "ReplicaSet",
        "metadata": {
            "name": "test-rs-owned",
            "namespace": "default",
            "ownerReferences": [{
                "apiVersion": "apps/v1",
                "kind": "Deployment",
                "name": "test-deployment",
                "uid": "12345678-1234-1234-1234-123456789012",
                "controller": true,
                "blockOwnerDeletion": true
            }]
        },
        "spec": {
            "replicas": 1,
            "selector": {
                "matchLabels": {
                    "app": "owned"
                }
            },
            "template": {
                "metadata": {
                    "labels": {
                        "app": "owned"
                    }
                },
                "spec": {
                    "containers": [{
                        "name": "nginx",
                        "image": "nginx:alpine"
                    }]
                }
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/replicasets", base_url))
        .json(&replicaset)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["ownerReferences"][0]["kind"], "Deployment");
    
    // Clean up
    client
        .delete(&format!("{}/namespaces/default/replicasets/test-rs-owned", base_url))
        .send()
        .await
        .unwrap();
}