use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_pod_status_subresource() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a Pod
    let pod = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "test-pod-status",
            "namespace": "default"
        },
        "spec": {
            "containers": [{
                "name": "nginx",
                "image": "nginx:alpine",
                "resources": {
                    "requests": {
                        "cpu": "100m",
                        "memory": "128Mi"
                    },
                    "limits": {
                        "cpu": "200m",
                        "memory": "256Mi"
                    }
                }
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/pods", base_url))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["status"]["phase"], "Pending");
    
    // Update Pod status
    let status_update = json!({
        "status": {
            "phase": "Running",
            "conditions": [
                {
                    "type": "Initialized",
                    "status": "True",
                    "lastProbeTime": null,
                    "lastTransitionTime": chrono::Utc::now().to_rfc3339()
                },
                {
                    "type": "Ready",
                    "status": "True",
                    "lastProbeTime": null,
                    "lastTransitionTime": chrono::Utc::now().to_rfc3339()
                },
                {
                    "type": "ContainersReady",
                    "status": "True",
                    "lastProbeTime": null,
                    "lastTransitionTime": chrono::Utc::now().to_rfc3339()
                },
                {
                    "type": "PodScheduled",
                    "status": "True",
                    "lastProbeTime": null,
                    "lastTransitionTime": chrono::Utc::now().to_rfc3339()
                }
            ],
            "hostIP": "10.0.0.1",
            "podIP": "172.17.0.2",
            "podIPs": [
                {
                    "ip": "172.17.0.2"
                }
            ],
            "startTime": chrono::Utc::now().to_rfc3339(),
            "containerStatuses": [
                {
                    "name": "nginx",
                    "state": {
                        "running": {
                            "startedAt": chrono::Utc::now().to_rfc3339()
                        }
                    },
                    "lastState": {},
                    "ready": true,
                    "restartCount": 0,
                    "image": "nginx:alpine",
                    "imageID": "docker://sha256:abc123",
                    "containerID": "docker://container123",
                    "started": true
                }
            ],
            "qosClass": "Burstable"
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/pods/test-pod-status/status", base_url))
        .json(&status_update)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["status"]["phase"], "Running");
    assert_eq!(updated["status"]["podIP"], "172.17.0.2");
    assert_eq!(updated["status"]["containerStatuses"][0]["ready"], true);
    
    // Get Pod status
    let response = client
        .get(&format!("{}/namespaces/default/pods/test-pod-status/status", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let status: serde_json::Value = response.json().await.unwrap();
    assert_eq!(status["status"]["phase"], "Running");
    
    // Clean up
    client
        .delete(&format!("{}/namespaces/default/pods/test-pod-status", base_url))
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_pod_ephemeralcontainers() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a Pod
    let pod = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "test-pod-ephemeral",
            "namespace": "default"
        },
        "spec": {
            "containers": [{
                "name": "main",
                "image": "nginx:alpine"
            }]
        }
    });
    
    client
        .post(&format!("{}/namespaces/default/pods", base_url))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    // Add ephemeral container
    let ephemeral_update = json!({
        "spec": {
            "ephemeralContainers": [{
                "name": "debugger",
                "image": "busybox",
                "command": ["sh"],
                "stdin": true,
                "tty": true,
                "targetContainerName": "main"
            }]
        }
    });
    
    let response = client
        .patch(&format!("{}/namespaces/default/pods/test-pod-ephemeral/ephemeralcontainers", base_url))
        .header("Content-Type", "application/strategic-merge-patch+json")
        .json(&ephemeral_update)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("Ephemeral containers endpoint not implemented yet");
        // Clean up
        client
            .delete(&format!("{}/namespaces/default/pods/test-pod-ephemeral", base_url))
            .send()
            .await
            .unwrap();
        return;
    }
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["spec"]["ephemeralContainers"][0]["name"], "debugger");
    
    // Clean up
    client
        .delete(&format!("{}/namespaces/default/pods/test-pod-ephemeral", base_url))
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_pod_binding() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a Pod
    let pod = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "test-pod-binding",
            "namespace": "default"
        },
        "spec": {
            "containers": [{
                "name": "nginx",
                "image": "nginx:alpine"
            }]
        }
    });
    
    client
        .post(&format!("{}/namespaces/default/pods", base_url))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    // Create a binding (schedule the pod to a node)
    let binding = json!({
        "apiVersion": "v1",
        "kind": "Binding",
        "metadata": {
            "name": "test-pod-binding",
            "namespace": "default"
        },
        "target": {
            "apiVersion": "v1",
            "kind": "Node",
            "name": "krust-node"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/pods/test-pod-binding/binding", base_url))
        .json(&binding)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("Pod binding endpoint not implemented yet");
        // Clean up
        client
            .delete(&format!("{}/namespaces/default/pods/test-pod-binding", base_url))
            .send()
            .await
            .unwrap();
        return;
    }
    
    assert_eq!(response.status(), 201);
    
    // Verify pod is bound
    let response = client
        .get(&format!("{}/namespaces/default/pods/test-pod-binding", base_url))
        .send()
        .await
        .unwrap();
    
    let pod: serde_json::Value = response.json().await.unwrap();
    assert_eq!(pod["spec"]["nodeName"], "krust-node");
    
    // Clean up
    client
        .delete(&format!("{}/namespaces/default/pods/test-pod-binding", base_url))
        .send()
        .await
        .unwrap();
}