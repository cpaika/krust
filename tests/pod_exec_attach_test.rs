use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_pod_exec_websocket() {
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
            "name": "test-pod-exec",
            "namespace": "default"
        },
        "spec": {
            "containers": [{
                "name": "main",
                "image": "busybox",
                "command": ["sh", "-c", "sleep 3600"]
            }]
        }
    });
    
    client
        .post(&format!("{}/namespaces/default/pods", base_url))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    // Test exec endpoint exists
    // Note: The exec endpoint expects a WebSocket upgrade in real Kubernetes
    // For now, we just test that the endpoint is registered
    let response = client
        .get(&format!("{}/namespaces/default/pods/test-pod-exec/exec", base_url))
        .send()
        .await
        .unwrap();
    
    // For now, we expect 400 (bad request - missing params) or 501 (not implemented)
    // since WebSocket support needs additional implementation
    assert!(response.status() == 400 || response.status() == 501, 
            "Expected 400 or 501, got {}", response.status());
    
    // Clean up
    client
        .delete(&format!("{}/namespaces/default/pods/test-pod-exec", base_url))
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_pod_attach_websocket() {
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
            "name": "test-pod-attach",
            "namespace": "default"
        },
        "spec": {
            "containers": [{
                "name": "main",
                "image": "busybox",
                "command": ["sh", "-c", "sleep 3600"]
            }]
        }
    });
    
    client
        .post(&format!("{}/namespaces/default/pods", base_url))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    // Test attach endpoint exists
    // Note: The attach endpoint expects a WebSocket upgrade in real Kubernetes
    // For now, we just test that the endpoint is registered
    let response = client
        .get(&format!("{}/namespaces/default/pods/test-pod-attach/attach", base_url))
        .send()
        .await
        .unwrap();
    
    // For now, we expect 400 (bad request - missing params) or 501 (not implemented)
    // since WebSocket support needs additional implementation
    assert!(response.status() == 400 || response.status() == 501,
            "Expected 400 or 501, got {}", response.status());
    
    // Clean up
    client
        .delete(&format!("{}/namespaces/default/pods/test-pod-attach", base_url))
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_pod_portforward() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a Pod with a port
    let pod = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "test-pod-portforward",
            "namespace": "default"
        },
        "spec": {
            "containers": [{
                "name": "nginx",
                "image": "nginx:alpine",
                "ports": [{
                    "containerPort": 80,
                    "name": "http"
                }]
            }]
        }
    });
    
    client
        .post(&format!("{}/namespaces/default/pods", base_url))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    // Test portforward endpoint exists (implemented with WebSocket support)
    let response = client
        .get(&format!("{}/namespaces/default/pods/test-pod-portforward/portforward", base_url))
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .send()
        .await
        .unwrap();
    
    // The portforward endpoint is already implemented but requires WebSocket upgrade
    // Expect 426 Upgrade Required or 400 Bad Request
    assert!(response.status() == 426 || response.status() == 400,
            "Expected 400 or 426, got {}", response.status());
    
    // Clean up
    client
        .delete(&format!("{}/namespaces/default/pods/test-pod-portforward", base_url))
        .send()
        .await
        .unwrap();
}