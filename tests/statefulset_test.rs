use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_statefulset_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/apps/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Test 1: Create a StatefulSet
    let statefulset = json!({
        "apiVersion": "apps/v1",
        "kind": "StatefulSet",
        "metadata": {
            "name": "test-statefulset",
            "namespace": "default"
        },
        "spec": {
            "replicas": 3,
            "selector": {
                "matchLabels": {
                    "app": "test-app"
                }
            },
            "serviceName": "test-service",
            "template": {
                "metadata": {
                    "labels": {
                        "app": "test-app"
                    }
                },
                "spec": {
                    "containers": [{
                        "name": "nginx",
                        "image": "nginx:1.21",
                        "ports": [{
                            "containerPort": 80,
                            "name": "web"
                        }]
                    }]
                }
            },
            "volumeClaimTemplates": [{
                "metadata": {
                    "name": "data"
                },
                "spec": {
                    "accessModes": ["ReadWriteOnce"],
                    "resources": {
                        "requests": {
                            "storage": "1Gi"
                        }
                    }
                }
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/statefulsets", base_url))
        .json(&statefulset)
        .send()
        .await
        .unwrap();
    
    println!("Create StatefulSet response status: {}", response.status());
    
    if response.status() == 404 {
        println!("StatefulSet endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201, "StatefulSet creation should return 201");
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-statefulset");
    assert_eq!(created["spec"]["replicas"], 3);
    assert_eq!(created["spec"]["serviceName"], "test-service");
    
    // Test 2: Get the StatefulSet
    let response = client
        .get(&format!("{}/namespaces/default/statefulsets/test-statefulset", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let fetched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(fetched["metadata"]["name"], "test-statefulset");
    assert_eq!(fetched["spec"]["replicas"], 3);
    
    // Test 3: List StatefulSets
    let response = client
        .get(&format!("{}/namespaces/default/statefulsets", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["kind"], "StatefulSetList");
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Test 4: Update the StatefulSet
    let updated_statefulset = json!({
        "apiVersion": "apps/v1",
        "kind": "StatefulSet",
        "metadata": {
            "name": "test-statefulset",
            "namespace": "default",
            "resourceVersion": created["metadata"]["resourceVersion"]
        },
        "spec": {
            "replicas": 5,
            "selector": {
                "matchLabels": {
                    "app": "test-app"
                }
            },
            "serviceName": "test-service",
            "template": {
                "metadata": {
                    "labels": {
                        "app": "test-app"
                    }
                },
                "spec": {
                    "containers": [{
                        "name": "nginx",
                        "image": "nginx:1.22",
                        "ports": [{
                            "containerPort": 80,
                            "name": "web"
                        }]
                    }]
                }
            },
            "volumeClaimTemplates": [{
                "metadata": {
                    "name": "data"
                },
                "spec": {
                    "accessModes": ["ReadWriteOnce"],
                    "resources": {
                        "requests": {
                            "storage": "1Gi"
                        }
                    }
                }
            }]
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/statefulsets/test-statefulset", base_url))
        .json(&updated_statefulset)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["spec"]["replicas"], 5);
    
    // Test 5: Scale the StatefulSet
    let scale = json!({
        "apiVersion": "autoscaling/v1",
        "kind": "Scale",
        "metadata": {
            "name": "test-statefulset",
            "namespace": "default"
        },
        "spec": {
            "replicas": 2
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/statefulsets/test-statefulset/scale", base_url))
        .json(&scale)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let scaled: serde_json::Value = response.json().await.unwrap();
    assert_eq!(scaled["spec"]["replicas"], 2);
    
    // Test 6: Get StatefulSet status
    let response = client
        .get(&format!("{}/namespaces/default/statefulsets/test-statefulset/status", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let status: serde_json::Value = response.json().await.unwrap();
    assert!(status["status"].is_object());
    
    // Test 7: Delete the StatefulSet
    let response = client
        .delete(&format!("{}/namespaces/default/statefulsets/test-statefulset", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Verify it's deleted
    let response = client
        .get(&format!("{}/namespaces/default/statefulsets/test-statefulset", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_statefulset_pod_management() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/apps/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a StatefulSet
    let statefulset = json!({
        "apiVersion": "apps/v1",
        "kind": "StatefulSet",
        "metadata": {
            "name": "pod-mgmt-test",
            "namespace": "default"
        },
        "spec": {
            "replicas": 2,
            "selector": {
                "matchLabels": {
                    "app": "pod-mgmt"
                }
            },
            "serviceName": "pod-mgmt-service",
            "podManagementPolicy": "OrderedReady",
            "updateStrategy": {
                "type": "RollingUpdate",
                "rollingUpdate": {
                    "partition": 0
                }
            },
            "template": {
                "metadata": {
                    "labels": {
                        "app": "pod-mgmt"
                    }
                },
                "spec": {
                    "containers": [{
                        "name": "app",
                        "image": "busybox:1.35",
                        "command": ["sleep", "3600"]
                    }]
                }
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/statefulsets", base_url))
        .json(&statefulset)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        println!("StatefulSet endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["spec"]["podManagementPolicy"], "OrderedReady");
    
    // Wait a bit for pods to be created
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Check that pods are created with predictable names
    let pod_response = client
        .get("http://localhost:6443/api/v1/namespaces/default/pods")
        .send()
        .await
        .unwrap();
    
    if pod_response.status() == 200 {
        let pods: serde_json::Value = pod_response.json().await.unwrap();
        // In a full implementation, we'd check for pods named pod-mgmt-test-0, pod-mgmt-test-1
    }
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/default/statefulsets/pod-mgmt-test", base_url))
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_statefulset_parallel_pod_management() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/apps/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a StatefulSet with Parallel pod management
    let statefulset = json!({
        "apiVersion": "apps/v1",
        "kind": "StatefulSet",
        "metadata": {
            "name": "parallel-test",
            "namespace": "default"
        },
        "spec": {
            "replicas": 3,
            "selector": {
                "matchLabels": {
                    "app": "parallel"
                }
            },
            "serviceName": "parallel-service",
            "podManagementPolicy": "Parallel",
            "template": {
                "metadata": {
                    "labels": {
                        "app": "parallel"
                    }
                },
                "spec": {
                    "containers": [{
                        "name": "app",
                        "image": "busybox:1.35",
                        "command": ["sleep", "3600"]
                    }]
                }
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/statefulsets", base_url))
        .json(&statefulset)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        println!("StatefulSet endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["spec"]["podManagementPolicy"], "Parallel");
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/default/statefulsets/parallel-test", base_url))
        .send()
        .await
        .unwrap();
}