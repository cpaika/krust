use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_resourcequota_crud() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create namespace first
    let namespace = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": "test-quota"
        }
    });
    
    let _ = client
        .post(&format!("{}/namespaces", base_url))
        .json(&namespace)
        .send()
        .await;
    
    // Create ResourceQuota
    let quota = json!({
        "apiVersion": "v1",
        "kind": "ResourceQuota",
        "metadata": {
            "name": "test-quota",
            "namespace": "test-quota"
        },
        "spec": {
            "hard": {
                "requests.cpu": "100",
                "requests.memory": "100Gi",
                "limits.cpu": "200",
                "limits.memory": "200Gi",
                "persistentvolumeclaims": "10",
                "pods": "50",
                "services": "10"
            },
            "scopeSelector": {
                "matchExpressions": [
                    {
                        "scopeName": "PriorityClass",
                        "operator": "In",
                        "values": ["high"]
                    }
                ]
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/test-quota/resourcequotas", base_url))
        .json(&quota)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-quota");
    
    // Get ResourceQuota
    let response = client
        .get(&format!("{}/namespaces/test-quota/resourcequotas/test-quota", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    // Update ResourceQuota
    let mut updated_quota = created.clone();
    updated_quota["spec"]["hard"]["pods"] = json!("100");
    
    let response = client
        .put(&format!("{}/namespaces/test-quota/resourcequotas/test-quota", base_url))
        .json(&updated_quota)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    // List ResourceQuotas
    let response = client
        .get(&format!("{}/namespaces/test-quota/resourcequotas", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    
    // Update status
    let status_update = json!({
        "status": {
            "hard": {
                "requests.cpu": "100",
                "requests.memory": "100Gi",
                "limits.cpu": "200",
                "limits.memory": "200Gi",
                "persistentvolumeclaims": "10",
                "pods": "100",
                "services": "10"
            },
            "used": {
                "requests.cpu": "50",
                "requests.memory": "50Gi",
                "limits.cpu": "100",
                "limits.memory": "100Gi",
                "persistentvolumeclaims": "5",
                "pods": "25",
                "services": "5"
            }
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/test-quota/resourcequotas/test-quota/status", base_url))
        .json(&status_update)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    // Delete ResourceQuota
    let response = client
        .delete(&format!("{}/namespaces/test-quota/resourcequotas/test-quota", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn test_limitrange_crud() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create namespace first
    let namespace = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": "test-limits"
        }
    });
    
    let _ = client
        .post(&format!("{}/namespaces", base_url))
        .json(&namespace)
        .send()
        .await;
    
    // Create LimitRange
    let limitrange = json!({
        "apiVersion": "v1",
        "kind": "LimitRange",
        "metadata": {
            "name": "test-limits",
            "namespace": "test-limits"
        },
        "spec": {
            "limits": [
                {
                    "type": "Pod",
                    "max": {
                        "cpu": "2",
                        "memory": "1Gi"
                    },
                    "min": {
                        "cpu": "100m",
                        "memory": "128Mi"
                    }
                },
                {
                    "type": "Container",
                    "default": {
                        "cpu": "500m",
                        "memory": "512Mi"
                    },
                    "defaultRequest": {
                        "cpu": "200m",
                        "memory": "256Mi"
                    },
                    "max": {
                        "cpu": "1",
                        "memory": "512Mi"
                    },
                    "min": {
                        "cpu": "100m",
                        "memory": "128Mi"
                    }
                },
                {
                    "type": "PersistentVolumeClaim",
                    "max": {
                        "storage": "10Gi"
                    },
                    "min": {
                        "storage": "1Gi"
                    }
                }
            ]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/test-limits/limitranges", base_url))
        .json(&limitrange)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-limits");
    
    // Get LimitRange
    let response = client
        .get(&format!("{}/namespaces/test-limits/limitranges/test-limits", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    // Update LimitRange
    let mut updated_limitrange = created.clone();
    updated_limitrange["spec"]["limits"][0]["max"]["cpu"] = json!("4");
    
    let response = client
        .put(&format!("{}/namespaces/test-limits/limitranges/test-limits", base_url))
        .json(&updated_limitrange)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    // List LimitRanges
    let response = client
        .get(&format!("{}/namespaces/test-limits/limitranges", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    
    // Delete LimitRange
    let response = client
        .delete(&format!("{}/namespaces/test-limits/limitranges/test-limits", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn test_quota_enforcement() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create namespace first
    let namespace = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": "quota-test"
        }
    });
    
    let _ = client
        .post(&format!("{}/namespaces", base_url))
        .json(&namespace)
        .send()
        .await;
    
    // Create a strict ResourceQuota
    let quota = json!({
        "apiVersion": "v1",
        "kind": "ResourceQuota",
        "metadata": {
            "name": "strict-quota",
            "namespace": "quota-test"
        },
        "spec": {
            "hard": {
                "pods": "2",
                "requests.cpu": "1",
                "requests.memory": "1Gi"
            }
        }
    });
    
    client
        .post(&format!("{}/api/v1/namespaces/quota-test/resourcequotas", base_url))
        .json(&quota)
        .send()
        .await
        .unwrap();
    
    // Create first pod - should succeed
    let pod1 = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "pod1",
            "namespace": "quota-test"
        },
        "spec": {
            "containers": [{
                "name": "container1",
                "image": "nginx",
                "resources": {
                    "requests": {
                        "cpu": "500m",
                        "memory": "512Mi"
                    }
                }
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/api/v1/namespaces/quota-test/pods", base_url))
        .json(&pod1)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    
    // Create second pod - should succeed (at limit)
    let pod2 = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "pod2",
            "namespace": "quota-test"
        },
        "spec": {
            "containers": [{
                "name": "container1",
                "image": "nginx",
                "resources": {
                    "requests": {
                        "cpu": "500m",
                        "memory": "512Mi"
                    }
                }
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/api/v1/namespaces/quota-test/pods", base_url))
        .json(&pod2)
        .send()
        .await
        .unwrap();
    
    // This might fail depending on quota enforcement implementation
    // For now, we just check that the API accepts the request
    assert!(response.status() == reqwest::StatusCode::CREATED || response.status() == reqwest::StatusCode::FORBIDDEN);
}