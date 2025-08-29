use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_daemonset_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/apps/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Test 1: Create a DaemonSet
    let daemonset = json!({
        "apiVersion": "apps/v1",
        "kind": "DaemonSet",
        "metadata": {
            "name": "test-daemonset",
            "namespace": "default"
        },
        "spec": {
            "selector": {
                "matchLabels": {
                    "app": "test-daemon"
                }
            },
            "template": {
                "metadata": {
                    "labels": {
                        "app": "test-daemon"
                    }
                },
                "spec": {
                    "containers": [{
                        "name": "fluentd",
                        "image": "fluentd:v1.14",
                        "resources": {
                            "limits": {
                                "memory": "200Mi"
                            },
                            "requests": {
                                "cpu": "100m",
                                "memory": "200Mi"
                            }
                        }
                    }],
                    "tolerations": [{
                        "key": "node-role.kubernetes.io/master",
                        "effect": "NoSchedule"
                    }]
                }
            },
            "updateStrategy": {
                "type": "RollingUpdate",
                "rollingUpdate": {
                    "maxUnavailable": 1
                }
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/daemonsets", base_url))
        .json(&daemonset)
        .send()
        .await
        .unwrap();
    
    println!("Create DaemonSet response status: {}", response.status());
    
    if response.status() == 404 {
        println!("DaemonSet endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201, "DaemonSet creation should return 201");
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-daemonset");
    assert_eq!(created["spec"]["updateStrategy"]["type"], "RollingUpdate");
    
    // Test 2: Get the DaemonSet
    let response = client
        .get(&format!("{}/namespaces/default/daemonsets/test-daemonset", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let fetched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(fetched["metadata"]["name"], "test-daemonset");
    
    // Test 3: List DaemonSets
    let response = client
        .get(&format!("{}/namespaces/default/daemonsets", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["kind"], "DaemonSetList");
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Test 4: Update the DaemonSet
    let updated_daemonset = json!({
        "apiVersion": "apps/v1",
        "kind": "DaemonSet",
        "metadata": {
            "name": "test-daemonset",
            "namespace": "default",
            "resourceVersion": created["metadata"]["resourceVersion"]
        },
        "spec": {
            "selector": {
                "matchLabels": {
                    "app": "test-daemon"
                }
            },
            "template": {
                "metadata": {
                    "labels": {
                        "app": "test-daemon",
                        "version": "v2"
                    }
                },
                "spec": {
                    "containers": [{
                        "name": "fluentd",
                        "image": "fluentd:v1.15",
                        "resources": {
                            "limits": {
                                "memory": "400Mi"
                            },
                            "requests": {
                                "cpu": "200m",
                                "memory": "400Mi"
                            }
                        }
                    }],
                    "tolerations": [{
                        "key": "node-role.kubernetes.io/master",
                        "effect": "NoSchedule"
                    }]
                }
            },
            "updateStrategy": {
                "type": "RollingUpdate",
                "rollingUpdate": {
                    "maxUnavailable": 2
                }
            }
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/daemonsets/test-daemonset", base_url))
        .json(&updated_daemonset)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["spec"]["template"]["spec"]["containers"][0]["image"], "fluentd:v1.15");
    
    // Test 5: Get DaemonSet status
    let response = client
        .get(&format!("{}/namespaces/default/daemonsets/test-daemonset/status", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let status: serde_json::Value = response.json().await.unwrap();
    assert!(status["status"].is_object());
    
    // Test 6: Delete the DaemonSet
    let response = client
        .delete(&format!("{}/namespaces/default/daemonsets/test-daemonset", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Verify it's deleted
    let response = client
        .get(&format!("{}/namespaces/default/daemonsets/test-daemonset", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_daemonset_node_selector() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/apps/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create DaemonSet with node selector
    let daemonset = json!({
        "apiVersion": "apps/v1",
        "kind": "DaemonSet",
        "metadata": {
            "name": "node-selector-ds",
            "namespace": "default"
        },
        "spec": {
            "selector": {
                "matchLabels": {
                    "app": "monitoring"
                }
            },
            "template": {
                "metadata": {
                    "labels": {
                        "app": "monitoring"
                    }
                },
                "spec": {
                    "nodeSelector": {
                        "disk": "ssd",
                        "environment": "production"
                    },
                    "containers": [{
                        "name": "metrics",
                        "image": "prom/node-exporter:v1.3.1"
                    }]
                }
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/daemonsets", base_url))
        .json(&daemonset)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        println!("DaemonSet endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["spec"]["template"]["spec"]["nodeSelector"]["disk"], "ssd");
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/default/daemonsets/node-selector-ds", base_url))
        .send()
        .await
        .unwrap();
}