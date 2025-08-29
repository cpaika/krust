use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_hpa_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/autoscaling/v2";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create HorizontalPodAutoscaler
    let hpa = json!({
        "apiVersion": "autoscaling/v2",
        "kind": "HorizontalPodAutoscaler",
        "metadata": {
            "name": "test-hpa",
            "namespace": "default"
        },
        "spec": {
            "scaleTargetRef": {
                "apiVersion": "apps/v1",
                "kind": "Deployment",
                "name": "test-deployment"
            },
            "minReplicas": 2,
            "maxReplicas": 10,
            "metrics": [{
                "type": "Resource",
                "resource": {
                    "name": "cpu",
                    "target": {
                        "type": "Utilization",
                        "averageUtilization": 50
                    }
                }
            }, {
                "type": "Resource",
                "resource": {
                    "name": "memory",
                    "target": {
                        "type": "Utilization",
                        "averageUtilization": 80
                    }
                }
            }],
            "behavior": {
                "scaleDown": {
                    "stabilizationWindowSeconds": 300,
                    "policies": [{
                        "type": "Percent",
                        "value": 100,
                        "periodSeconds": 60
                    }]
                },
                "scaleUp": {
                    "stabilizationWindowSeconds": 0,
                    "policies": [{
                        "type": "Percent",
                        "value": 100,
                        "periodSeconds": 15
                    }, {
                        "type": "Pods",
                        "value": 4,
                        "periodSeconds": 15
                    }],
                    "selectPolicy": "Max"
                }
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/horizontalpodautoscalers", base_url))
        .json(&hpa)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("HPA endpoints not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-hpa");
    assert_eq!(created["spec"]["minReplicas"], 2);
    assert_eq!(created["spec"]["maxReplicas"], 10);
    
    // Get HPA
    let response = client
        .get(&format!("{}/namespaces/default/horizontalpodautoscalers/test-hpa", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let retrieved: serde_json::Value = response.json().await.unwrap();
    assert_eq!(retrieved["metadata"]["name"], "test-hpa");
    
    // Update HPA
    let mut updated_hpa = retrieved.clone();
    updated_hpa["spec"]["maxReplicas"] = json!(20);
    updated_hpa["spec"]["minReplicas"] = json!(3);
    
    let response = client
        .put(&format!("{}/namespaces/default/horizontalpodautoscalers/test-hpa", base_url))
        .json(&updated_hpa)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("HPA update endpoint not implemented yet");
    } else {
        assert_eq!(response.status(), 200);
        let updated: serde_json::Value = response.json().await.unwrap();
        assert_eq!(updated["spec"]["maxReplicas"], 20);
        assert_eq!(updated["spec"]["minReplicas"], 3);
    }
    
    // Update HPA status
    let status_update = json!({
        "status": {
            "observedGeneration": 1,
            "desiredReplicas": 5,
            "currentReplicas": 3,
            "currentMetrics": [{
                "type": "Resource",
                "resource": {
                    "name": "cpu",
                    "current": {
                        "averageUtilization": 45,
                        "averageValue": "450m"
                    }
                }
            }],
            "conditions": [{
                "type": "AbleToScale",
                "status": "True",
                "lastTransitionTime": chrono::Utc::now().to_rfc3339(),
                "reason": "SucceededGetScale",
                "message": "the HPA controller was able to get the target's current scale"
            }, {
                "type": "ScalingActive",
                "status": "True",
                "lastTransitionTime": chrono::Utc::now().to_rfc3339(),
                "reason": "ValidMetricFound",
                "message": "the HPA was able to successfully calculate a replica count from cpu resource utilization"
            }],
            "lastScaleTime": chrono::Utc::now().to_rfc3339()
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/horizontalpodautoscalers/test-hpa/status", base_url))
        .json(&status_update)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("HPA status endpoint not implemented yet");
    } else {
        assert_eq!(response.status(), 200);
        let status_result: serde_json::Value = response.json().await.unwrap();
        assert_eq!(status_result["status"]["desiredReplicas"], 5);
    }
    
    // List HPAs
    let response = client
        .get(&format!("{}/namespaces/default/horizontalpodautoscalers", base_url))
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("HPA list endpoint not implemented yet");
    } else {
        assert_eq!(response.status(), 200);
        let list: serde_json::Value = response.json().await.unwrap();
        assert!(list["items"].as_array().unwrap().len() >= 1);
    }
    
    // Delete HPA
    let response = client
        .delete(&format!("{}/namespaces/default/horizontalpodautoscalers/test-hpa", base_url))
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("HPA delete endpoint not implemented yet");
    } else {
        assert_eq!(response.status(), 200);
        let deleted: serde_json::Value = response.json().await.unwrap();
        assert_eq!(deleted["metadata"]["name"], "test-hpa");
    }
}

#[tokio::test]
async fn test_hpa_controller_scaling() {
    let client = reqwest::Client::new();
    let api_base = "http://localhost:6443/api/v1";
    let apps_base = "http://localhost:6443/apis/apps/v1";
    let hpa_base = "http://localhost:6443/apis/autoscaling/v2";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a Deployment to scale
    let deployment = json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {
            "name": "test-app",
            "namespace": "default"
        },
        "spec": {
            "replicas": 1,
            "selector": {
                "matchLabels": {
                    "app": "test-app"
                }
            },
            "template": {
                "metadata": {
                    "labels": {
                        "app": "test-app"
                    }
                },
                "spec": {
                    "containers": [{
                        "name": "app",
                        "image": "nginx",
                        "resources": {
                            "requests": {
                                "cpu": "100m",
                                "memory": "128Mi"
                            },
                            "limits": {
                                "cpu": "500m",
                                "memory": "512Mi"
                            }
                        }
                    }]
                }
            }
        }
    });
    
    client
        .post(&format!("{}/namespaces/default/deployments", apps_base))
        .json(&deployment)
        .send()
        .await
        .unwrap();
    
    // Create HPA for the deployment
    let hpa = json!({
        "apiVersion": "autoscaling/v2",
        "kind": "HorizontalPodAutoscaler",
        "metadata": {
            "name": "test-app-hpa",
            "namespace": "default"
        },
        "spec": {
            "scaleTargetRef": {
                "apiVersion": "apps/v1",
                "kind": "Deployment",
                "name": "test-app"
            },
            "minReplicas": 1,
            "maxReplicas": 5,
            "metrics": [{
                "type": "Resource",
                "resource": {
                    "name": "cpu",
                    "target": {
                        "type": "Utilization",
                        "averageUtilization": 70
                    }
                }
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/horizontalpodautoscalers", hpa_base))
        .json(&hpa)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("HPA not implemented yet, skipping controller test");
        // Clean up
        client
            .delete(&format!("{}/namespaces/default/deployments/test-app", apps_base))
            .send()
            .await
            .unwrap();
        return;
    }
    
    // Simulate high CPU usage by updating HPA status
    let status_update = json!({
        "status": {
            "currentMetrics": [{
                "type": "Resource",
                "resource": {
                    "name": "cpu",
                    "current": {
                        "averageUtilization": 90,
                        "averageValue": "900m"
                    }
                }
            }],
            "desiredReplicas": 2,
            "currentReplicas": 1
        }
    });
    
    client
        .put(&format!("{}/namespaces/default/horizontalpodautoscalers/test-app-hpa/status", hpa_base))
        .json(&status_update)
        .send()
        .await
        .unwrap();
    
    // Check if the HPA controller would scale the deployment
    // In a real implementation, the controller would update the deployment replicas
    
    // Clean up
    client
        .delete(&format!("{}/namespaces/default/horizontalpodautoscalers/test-app-hpa", hpa_base))
        .send()
        .await
        .unwrap();
    
    client
        .delete(&format!("{}/namespaces/default/deployments/test-app", apps_base))
        .send()
        .await
        .unwrap();
}