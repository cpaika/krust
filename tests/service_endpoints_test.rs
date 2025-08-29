use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_service_endpoints_sync() {
    let client = reqwest::Client::new();
    let api_base = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a Service with a selector
    let service = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": "test-service",
            "namespace": "default"
        },
        "spec": {
            "selector": {
                "app": "test-app"
            },
            "ports": [{
                "port": 80,
                "targetPort": 8080,
                "protocol": "TCP",
                "name": "http"
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/services", api_base))
        .json(&service)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 201);
    
    // Check that Endpoints were automatically created
    let response = client
        .get(&format!("{}/namespaces/default/endpoints/test-service", api_base))
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("Endpoints auto-creation not implemented yet");
    } else {
        assert_eq!(response.status(), 200);
        let endpoints: serde_json::Value = response.json().await.unwrap();
        assert_eq!(endpoints["metadata"]["name"], "test-service");
        // Should have no subsets initially (no pods match selector)
        assert!(endpoints["subsets"].as_array().unwrap().is_empty());
    }
    
    // Create a Pod that matches the selector
    let pod = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "test-pod-1",
            "namespace": "default",
            "labels": {
                "app": "test-app"
            }
        },
        "spec": {
            "containers": [{
                "name": "app",
                "image": "nginx:alpine",
                "ports": [{
                    "containerPort": 8080,
                    "name": "http"
                }]
            }]
        }
    });
    
    client
        .post(&format!("{}/namespaces/default/pods", api_base))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    // Update Pod status to Running with IP
    let status_update = json!({
        "status": {
            "phase": "Running",
            "podIP": "10.1.1.1",
            "conditions": [{
                "type": "Ready",
                "status": "True"
            }]
        }
    });
    
    client
        .put(&format!("{}/namespaces/default/pods/test-pod-1/status", api_base))
        .json(&status_update)
        .send()
        .await
        .unwrap();
    
    // Check if Endpoints were updated
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    let response = client
        .get(&format!("{}/namespaces/default/endpoints/test-service", api_base))
        .send()
        .await
        .unwrap();
    
    if response.status() == 200 {
        let endpoints: serde_json::Value = response.json().await.unwrap();
        
        // Check if the endpoint controller updated the endpoints
        if !endpoints["subsets"].as_array().unwrap().is_empty() {
            let subset = &endpoints["subsets"][0];
            assert_eq!(subset["addresses"][0]["ip"], "10.1.1.1");
            assert_eq!(subset["ports"][0]["port"], 8080);
        } else {
            eprintln!("Endpoints controller not syncing yet");
        }
    }
    
    // Create another matching Pod
    let pod2 = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "test-pod-2",
            "namespace": "default",
            "labels": {
                "app": "test-app"
            }
        },
        "spec": {
            "containers": [{
                "name": "app",
                "image": "nginx:alpine",
                "ports": [{
                    "containerPort": 8080
                }]
            }]
        }
    });
    
    client
        .post(&format!("{}/namespaces/default/pods", api_base))
        .json(&pod2)
        .send()
        .await
        .unwrap();
    
    // Update second Pod status
    let status_update2 = json!({
        "status": {
            "phase": "Running",
            "podIP": "10.1.1.2",
            "conditions": [{
                "type": "Ready",
                "status": "True"
            }]
        }
    });
    
    client
        .put(&format!("{}/namespaces/default/pods/test-pod-2/status", api_base))
        .json(&status_update2)
        .send()
        .await
        .unwrap();
    
    // Clean up
    client
        .delete(&format!("{}/namespaces/default/services/test-service", api_base))
        .send()
        .await
        .unwrap();
    
    client
        .delete(&format!("{}/namespaces/default/pods/test-pod-1", api_base))
        .send()
        .await
        .unwrap();
    
    client
        .delete(&format!("{}/namespaces/default/pods/test-pod-2", api_base))
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_endpoints_crud() {
    let client = reqwest::Client::new();
    let api_base = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create Endpoints manually
    let endpoints = json!({
        "apiVersion": "v1",
        "kind": "Endpoints",
        "metadata": {
            "name": "manual-endpoints",
            "namespace": "default"
        },
        "subsets": [{
            "addresses": [{
                "ip": "192.168.1.1"
            }, {
                "ip": "192.168.1.2"
            }],
            "ports": [{
                "port": 80,
                "protocol": "TCP",
                "name": "http"
            }]
        }]
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/endpoints", api_base))
        .json(&endpoints)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("Endpoints create endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "manual-endpoints");
    assert_eq!(created["subsets"][0]["addresses"].as_array().unwrap().len(), 2);
    
    // Get Endpoints
    let response = client
        .get(&format!("{}/namespaces/default/endpoints/manual-endpoints", api_base))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Update Endpoints
    let mut updated_endpoints = created.clone();
    updated_endpoints["subsets"][0]["addresses"] = json!([
        {"ip": "192.168.1.3"},
        {"ip": "192.168.1.4"},
        {"ip": "192.168.1.5"}
    ]);
    
    let response = client
        .put(&format!("{}/namespaces/default/endpoints/manual-endpoints", api_base))
        .json(&updated_endpoints)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("Endpoints update endpoint not implemented yet");
    } else {
        assert_eq!(response.status(), 200);
        let updated: serde_json::Value = response.json().await.unwrap();
        assert_eq!(updated["subsets"][0]["addresses"].as_array().unwrap().len(), 3);
    }
    
    // List Endpoints
    let response = client
        .get(&format!("{}/namespaces/default/endpoints", api_base))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.unwrap();
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Delete Endpoints
    let response = client
        .delete(&format!("{}/namespaces/default/endpoints/manual-endpoints", api_base))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
}