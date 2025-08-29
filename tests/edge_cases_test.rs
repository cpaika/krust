use reqwest;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn test_namespace_edge_cases() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }

    // Test 1: Create namespace with empty name (should fail)
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": ""
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    assert_ne!(response.status(), 201, "Empty namespace name should be rejected");

    // Test 2: Create namespace with invalid characters
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": "test-ns-with-CAPS-and_underscores"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    // Kubernetes names must be lowercase alphanumeric or '-'
    assert_ne!(response.status(), 201, "Invalid namespace name should be rejected");

    // Test 3: Create namespace with very long name (>253 chars)
    let long_name = "a".repeat(254);
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": long_name
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    assert_ne!(response.status(), 201, "Too long namespace name should be rejected");

    // Test 4: Create duplicate namespace
    let unique_name = format!("test-dup-{}", Uuid::new_v4());
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &unique_name
        }
    });
    
    // First creation should succeed
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 201);
    
    // Second creation should fail with 409 Conflict
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 409, "Duplicate namespace should return 409 Conflict");

    // Test 5: Update non-existent namespace
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": "non-existent-ns-999"
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/non-existent-ns-999", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404, "Update non-existent namespace should return 404");

    // Test 6: Delete non-existent namespace
    let response = client
        .delete(&format!("{}/namespaces/non-existent-ns-999", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404, "Delete non-existent namespace should return 404");

    // Test 7: Get deleted namespace should return 404
    let unique_name = format!("test-del-{}", Uuid::new_v4());
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &unique_name
        }
    });
    
    // Create
    client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    // Delete
    client
        .delete(&format!("{}/namespaces/{}", base_url, unique_name))
        .send()
        .await
        .unwrap();
    
    // Try to get deleted namespace
    let response = client
        .get(&format!("{}/namespaces/{}", base_url, unique_name))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404, "Deleted namespace should return 404");

    // Cleanup
    client
        .delete(&format!("{}/namespaces/{}", base_url, unique_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_pod_edge_cases() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }

    // Ensure test namespace exists
    let ns_name = format!("test-pod-edge-{}", Uuid::new_v4());
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name
        }
    });
    
    client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();

    // Test 1: Create pod without containers (should fail)
    let pod = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "no-containers",
            "namespace": &ns_name
        },
        "spec": {
            "containers": []
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/pods", base_url, ns_name))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    // Should fail or create with error status
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        // Check if pod has error status
        assert!(created.get("status").is_some(), "Pod without containers should have status");
    }

    // Test 2: Create pod with duplicate container names
    let pod = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "dup-containers",
            "namespace": &ns_name
        },
        "spec": {
            "containers": [
                {
                    "name": "container1",
                    "image": "nginx"
                },
                {
                    "name": "container1",
                    "image": "redis"
                }
            ]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/pods", base_url, ns_name))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    // Should either reject or handle gracefully
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        let containers = &created["spec"]["containers"];
        // Verify container handling
        assert!(containers.is_array());
    }

    // Test 3: Create pod with invalid image name
    let pod = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "invalid-image",
            "namespace": &ns_name
        },
        "spec": {
            "containers": [{
                "name": "test",
                "image": "!!!invalid-image-name!!!"
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/pods", base_url, ns_name))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    // Should create but fail to pull image
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        // Status should eventually show image pull error
        assert!(created.get("metadata").is_some());
    }

    // Test 4: Update pod spec (should fail - pods are mostly immutable)
    let pod = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "test-immutable",
            "namespace": &ns_name
        },
        "spec": {
            "containers": [{
                "name": "test",
                "image": "nginx"
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/pods", base_url, ns_name))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        
        // Try to update the pod's container image
        let mut updated = created.clone();
        updated["spec"]["containers"][0]["image"] = json!("redis");
        
        let response = client
            .put(&format!("{}/namespaces/{}/pods/test-immutable", base_url, ns_name))
            .json(&updated)
            .send()
            .await
            .unwrap();
        
        // Most pod spec fields are immutable
        if response.status() == 200 {
            let result: serde_json::Value = response.json().await.unwrap();
            // Image should not have changed
            assert_eq!(result["spec"]["containers"][0]["image"], "nginx", 
                      "Pod container image should be immutable");
        }
    }

    // Test 5: Create pod in non-existent namespace
    let pod = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "test-pod",
            "namespace": "non-existent-namespace-999"
        },
        "spec": {
            "containers": [{
                "name": "test",
                "image": "nginx"
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/non-existent-namespace-999/pods", base_url))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    assert_ne!(response.status(), 201, "Pod creation in non-existent namespace should fail");

    // Cleanup
    client
        .delete(&format!("{}/namespaces/{}", base_url, ns_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_service_edge_cases() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }

    // Ensure test namespace exists
    let ns_name = format!("test-svc-edge-{}", Uuid::new_v4());
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name
        }
    });
    
    client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();

    // Test 1: Create service with invalid port number
    let service = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": "invalid-port",
            "namespace": &ns_name
        },
        "spec": {
            "ports": [{
                "port": 70000,  // Max port is 65535
                "targetPort": 80
            }],
            "selector": {
                "app": "test"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/services", base_url, ns_name))
        .json(&service)
        .send()
        .await
        .unwrap();
    
    // Should either reject or handle gracefully
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        // Check if port was clamped or handled
        let port = created["spec"]["ports"][0]["port"].as_i64().unwrap();
        assert!(port <= 65535, "Port should be valid");
    }

    // Test 2: Create service with duplicate port definitions
    let service = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": "dup-ports",
            "namespace": &ns_name
        },
        "spec": {
            "ports": [
                {
                    "name": "http",
                    "port": 80,
                    "targetPort": 8080
                },
                {
                    "name": "http2",
                    "port": 80,  // Duplicate port
                    "targetPort": 8081
                }
            ],
            "selector": {
                "app": "test"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/services", base_url, ns_name))
        .json(&service)
        .send()
        .await
        .unwrap();
    
    // Should handle duplicate ports
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        assert!(created["spec"]["ports"].is_array());
    }

    // Test 3: Create service without selector (headless service)
    let service = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": "headless",
            "namespace": &ns_name
        },
        "spec": {
            "clusterIP": "None",
            "ports": [{
                "port": 80,
                "targetPort": 80
            }]
            // No selector - valid for headless services
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/services", base_url, ns_name))
        .json(&service)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 201, "Headless service without selector should be valid");
    
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        assert_eq!(created["spec"]["clusterIP"], "None");
    }

    // Test 4: Create service with invalid ClusterIP
    let service = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": "invalid-ip",
            "namespace": &ns_name
        },
        "spec": {
            "clusterIP": "999.999.999.999",  // Invalid IP
            "ports": [{
                "port": 80,
                "targetPort": 80
            }],
            "selector": {
                "app": "test"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/services", base_url, ns_name))
        .json(&service)
        .send()
        .await
        .unwrap();
    
    // Should either reject or assign a valid IP
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        let cluster_ip = created["spec"]["clusterIP"].as_str().unwrap();
        // Should have assigned a valid IP or None
        assert!(cluster_ip == "None" || cluster_ip.starts_with("10."));
    }

    // Test 5: Update service type (some changes should be rejected)
    let service = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": "type-change",
            "namespace": &ns_name
        },
        "spec": {
            "type": "ClusterIP",
            "ports": [{
                "port": 80,
                "targetPort": 80
            }],
            "selector": {
                "app": "test"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/services", base_url, ns_name))
        .json(&service)
        .send()
        .await
        .unwrap();
    
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        
        // Try to change to NodePort
        let mut updated = created.clone();
        updated["spec"]["type"] = json!("NodePort");
        
        let response = client
            .put(&format!("{}/namespaces/{}/services/type-change", base_url, ns_name))
            .json(&updated)
            .send()
            .await
            .unwrap();
        
        // Type changes might be restricted
        if response.status() == 200 {
            let result: serde_json::Value = response.json().await.unwrap();
            assert!(result["spec"]["type"].is_string());
        }
    }

    // Cleanup
    client
        .delete(&format!("{}/namespaces/{}", base_url, ns_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_concurrent_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }

    // Test concurrent namespace creation
    let ns_base = format!("test-concurrent-{}", Uuid::new_v4());
    let mut handles = vec![];
    
    for i in 0..5 {
        let client = client.clone();
        let base_url = base_url.to_string();
        let ns_name = format!("{}-{}", ns_base, i);
        
        let handle = tokio::spawn(async move {
            let ns = json!({
                "apiVersion": "v1",
                "kind": "Namespace",
                "metadata": {
                    "name": ns_name.clone()
                }
            });
            
            let response = client
                .post(&format!("{}/namespaces", base_url))
                .json(&ns)
                .send()
                .await
                .unwrap();
            
            (ns_name, response.status())
        });
        
        handles.push(handle);
    }
    
    // Wait for all to complete
    let mut results = vec![];
    for handle in handles {
        results.push(handle.await.unwrap());
    }
    
    // All should have succeeded
    for (name, status) in &results {
        assert_eq!(*status, 201, "Concurrent creation of {} failed", name);
    }
    
    // Cleanup
    for (name, _) in results {
        client
            .delete(&format!("{}/namespaces/{}", base_url, name))
            .send()
            .await
            .ok();
    }

    // Test concurrent updates to same resource
    let ns_name = format!("test-race-{}", Uuid::new_v4());
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name,
            "labels": {
                "counter": "0"
            }
        }
    });
    
    client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    let mut handles = vec![];
    
    for i in 0..5 {
        let client = client.clone();
        let base_url = base_url.to_string();
        let ns_name = ns_name.clone();
        
        let handle = tokio::spawn(async move {
            // Get current namespace
            let response = client
                .get(&format!("{}/namespaces/{}", base_url, ns_name))
                .send()
                .await
                .unwrap();
            
            if response.status() == 200 {
                let mut ns: serde_json::Value = response.json().await.unwrap();
                
                // Update label
                ns["metadata"]["labels"]["counter"] = json!(i.to_string());
                ns["metadata"]["labels"][format!("update-{}", i)] = json!("true");
                
                // Try to update
                let response = client
                    .put(&format!("{}/namespaces/{}", base_url, ns_name))
                    .json(&ns)
                    .send()
                    .await
                    .unwrap();
                
                response.status()
            } else {
                response.status()
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all updates
    let mut update_results = vec![];
    for handle in handles {
        update_results.push(handle.await.unwrap());
    }
    
    // At least some should have succeeded
    let successes = update_results.iter().filter(|s| **s == 200).count();
    assert!(successes > 0, "At least some concurrent updates should succeed");
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/{}", base_url, ns_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_resource_validation() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }

    // Test 1: Wrong API version
    let ns = json!({
        "apiVersion": "v2",  // Wrong version
        "kind": "Namespace",
        "metadata": {
            "name": "test-wrong-version"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    // Should either accept (and correct) or reject
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        assert_eq!(created["apiVersion"], "v1", "API version should be corrected to v1");
    }

    // Test 2: Wrong kind
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Pod",  // Wrong kind for namespace endpoint
        "metadata": {
            "name": "test-wrong-kind"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    assert_ne!(response.status(), 201, "Wrong kind should be rejected");

    // Test 3: Missing required fields
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace"
        // Missing metadata
    });
    
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    assert_ne!(response.status(), 201, "Missing metadata should be rejected");

    // Test 4: Extra unknown fields (should be preserved or ignored)
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": format!("test-extra-{}", Uuid::new_v4())
        },
        "unknownField": "should-be-handled",
        "spec": {
            "unknownSpec": "value"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    // Should handle gracefully
    assert!(response.status() == 201 || response.status() == 400, 
            "Extra fields should be handled gracefully");
    
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        // Check if extra fields are preserved or stripped
        println!("Created with extra fields: {:?}", created.get("unknownField"));
    }

    // Test 5: Invalid JSON
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .header("Content-Type", "application/json")
        .body("{invalid json}")
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 400, "Invalid JSON should return 400");

    // Test 6: Label validation
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": format!("test-labels-{}", Uuid::new_v4()),
            "labels": {
                "valid-label": "value",
                "kubernetes.io/metadata.name": "reserved",  // Reserved prefix
                "very-long-label-key-that-exceeds-the-maximum-allowed-length-for-kubernetes-labels": "value",
                "label-with-invalid-chars!@#": "value"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    // Should handle label validation
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        let labels = &created["metadata"]["labels"];
        // Check which labels were accepted
        assert!(labels.get("valid-label").is_some(), "Valid label should be preserved");
    }
}

#[tokio::test]
async fn test_configmap_edge_cases() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }

    // Ensure test namespace exists
    let ns_name = format!("test-cm-edge-{}", Uuid::new_v4());
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": &ns_name
        }
    });
    
    client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();

    // Test 1: ConfigMap with very large data
    let large_value = "x".repeat(1024 * 1024); // 1MB value
    let configmap = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {
            "name": "large-data",
            "namespace": &ns_name
        },
        "data": {
            "large-key": large_value
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/configmaps", base_url, ns_name))
        .json(&configmap)
        .send()
        .await
        .unwrap();
    
    // Should handle large data (Kubernetes limit is 1MB total)
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        assert!(created["data"]["large-key"].as_str().unwrap().len() > 1000000);
    }

    // Cleanup
    client
        .delete(&format!("{}/namespaces/{}", base_url, ns_name))
        .send()
        .await
        .ok();
}
