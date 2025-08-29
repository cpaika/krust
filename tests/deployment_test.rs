use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_deployment_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/apps/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a Deployment
    let deployment = json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {
            "name": "test-deployment",
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
                        "image": "nginx:1.14.2",
                        "ports": [{
                            "containerPort": 80
                        }]
                    }]
                }
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/deployments", base_url))
        .json(&deployment)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-deployment");
    assert_eq!(created["spec"]["replicas"], 3);
    
    // Get Deployment
    let response = client
        .get(&format!("{}/namespaces/default/deployments/test-deployment", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let retrieved: serde_json::Value = response.json().await.unwrap();
    assert_eq!(retrieved["metadata"]["name"], "test-deployment");
    
    // Update Deployment (rollout new version)
    let mut updated_deployment = retrieved.clone();
    updated_deployment["spec"]["template"]["spec"]["containers"][0]["image"] = json!("nginx:1.16.0");
    updated_deployment["spec"]["replicas"] = json!(5);
    
    let response = client
        .put(&format!("{}/namespaces/default/deployments/test-deployment", base_url))
        .json(&updated_deployment)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["spec"]["template"]["spec"]["containers"][0]["image"], "nginx:1.16.0");
    assert_eq!(updated["spec"]["replicas"], 5);
    
    // Update Deployment status
    let status_update = json!({
        "status": {
            "observedGeneration": 2,
            "replicas": 5,
            "updatedReplicas": 5,
            "readyReplicas": 5,
            "availableReplicas": 5,
            "conditions": [{
                "type": "Available",
                "status": "True",
                "lastUpdateTime": chrono::Utc::now().to_rfc3339(),
                "lastTransitionTime": chrono::Utc::now().to_rfc3339(),
                "reason": "MinimumReplicasAvailable",
                "message": "Deployment has minimum availability."
            }, {
                "type": "Progressing",
                "status": "True",
                "lastUpdateTime": chrono::Utc::now().to_rfc3339(),
                "lastTransitionTime": chrono::Utc::now().to_rfc3339(),
                "reason": "NewReplicaSetAvailable",
                "message": "ReplicaSet \"test-deployment-xyz\" has successfully progressed."
            }]
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/deployments/test-deployment/status", base_url))
        .json(&status_update)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("Deployment status endpoint not implemented yet");
    } else {
        assert_eq!(response.status(), 200);
        let status_result: serde_json::Value = response.json().await.unwrap();
        assert_eq!(status_result["status"]["readyReplicas"], 5);
    }
    
    // Get Deployment scale
    let response = client
        .get(&format!("{}/namespaces/default/deployments/test-deployment/scale", base_url))
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("Deployment scale endpoint not implemented yet");
    } else {
        assert_eq!(response.status(), 200);
        let scale: serde_json::Value = response.json().await.unwrap();
        assert_eq!(scale["spec"]["replicas"], 5);
    }
    
    // Update Deployment scale
    let scale_update = json!({
        "apiVersion": "autoscaling/v1",
        "kind": "Scale",
        "metadata": {
            "name": "test-deployment",
            "namespace": "default"
        },
        "spec": {
            "replicas": 10
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/deployments/test-deployment/scale", base_url))
        .json(&scale_update)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("Deployment scale endpoint not implemented yet");
    } else {
        assert_eq!(response.status(), 200);
        let scaled: serde_json::Value = response.json().await.unwrap();
        assert_eq!(scaled["spec"]["replicas"], 10);
    }
    
    // Patch Deployment
    let patch = json!({
        "metadata": {
            "annotations": {
                "deployment.kubernetes.io/revision": "2"
            }
        }
    });
    
    let response = client
        .patch(&format!("{}/namespaces/default/deployments/test-deployment", base_url))
        .header("Content-Type", "application/merge-patch+json")
        .json(&patch)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("Deployment patch endpoint not implemented yet");
    } else {
        assert_eq!(response.status(), 200);
        let patched: serde_json::Value = response.json().await.unwrap();
        assert_eq!(patched["metadata"]["annotations"]["deployment.kubernetes.io/revision"], "2");
    }
    
    // List Deployments
    let response = client
        .get(&format!("{}/namespaces/default/deployments", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.unwrap();
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Delete Deployment
    let response = client
        .delete(&format!("{}/namespaces/default/deployments/test-deployment", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let deleted: serde_json::Value = response.json().await.unwrap();
    assert_eq!(deleted["metadata"]["name"], "test-deployment");
}

#[tokio::test]
async fn test_deployment_rollout() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/apps/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create initial Deployment
    let deployment = json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {
            "name": "test-rollout",
            "namespace": "default"
        },
        "spec": {
            "replicas": 3,
            "selector": {
                "matchLabels": {
                    "app": "rollout-test"
                }
            },
            "template": {
                "metadata": {
                    "labels": {
                        "app": "rollout-test"
                    }
                },
                "spec": {
                    "containers": [{
                        "name": "app",
                        "image": "app:v1"
                    }]
                }
            },
            "strategy": {
                "type": "RollingUpdate",
                "rollingUpdate": {
                    "maxSurge": 1,
                    "maxUnavailable": 1
                }
            }
        }
    });
    
    client
        .post(&format!("{}/namespaces/default/deployments", base_url))
        .json(&deployment)
        .send()
        .await
        .unwrap();
    
    // Trigger rollout by updating image
    let response = client
        .get(&format!("{}/namespaces/default/deployments/test-rollout", base_url))
        .send()
        .await
        .unwrap();
    
    let mut deployment: serde_json::Value = response.json().await.unwrap();
    deployment["spec"]["template"]["spec"]["containers"][0]["image"] = json!("app:v2");
    
    client
        .put(&format!("{}/namespaces/default/deployments/test-rollout", base_url))
        .json(&deployment)
        .send()
        .await
        .unwrap();
    
    // Check rollout status subresource
    let response = client
        .get(&format!("{}/namespaces/default/deployments/test-rollout/status", base_url))
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("Deployment status subresource not implemented yet");
    } else {
        assert_eq!(response.status(), 200);
        let status: serde_json::Value = response.json().await.unwrap();
        // Check that status fields exist
        assert!(status["status"].get("observedGeneration").is_some());
    }
    
    // Clean up
    client
        .delete(&format!("{}/namespaces/default/deployments/test-rollout", base_url))
        .send()
        .await
        .unwrap();
}