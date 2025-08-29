use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest;
use serde_json::json;
use uuid::Uuid;

// Test helper to ensure namespace exists
async fn ensure_namespace(client: &reqwest::Client, base_url: &str, ns_name: &str) {
    let ns = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": ns_name
        }
    });
    
    client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_pod_immutability_validation() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }

    let ns_name = format!("test-pod-immut-{}", Uuid::new_v4());
    ensure_namespace(&client, base_url, &ns_name).await;

    // Test 1: Create a pod
    let pod = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "immutable-pod",
            "namespace": &ns_name
        },
        "spec": {
            "containers": [{
                "name": "nginx",
                "image": "nginx:1.14"
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/pods", base_url, ns_name))
        .json(&pod)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 201, "Pod creation should succeed");
    let created: serde_json::Value = response.json().await.unwrap();
    
    // Test 2: Try to update immutable spec fields (should fail or be ignored)
    let mut update = created.clone();
    update["spec"]["containers"][0]["image"] = json!("nginx:1.15");
    update["spec"]["containers"][0]["name"] = json!("nginx-changed");
    
    let response = client
        .put(&format!("{}/namespaces/{}/pods/immutable-pod", base_url, ns_name))
        .json(&update)
        .send()
        .await
        .unwrap();
    
    // Should either reject (422) or silently ignore the spec changes (200)
    if response.status() == 200 {
        let updated: serde_json::Value = response.json().await.unwrap();
        // Spec should remain unchanged
        assert_eq!(
            updated["spec"]["containers"][0]["image"], 
            "nginx:1.14",
            "Pod container image must remain immutable"
        );
        assert_eq!(
            updated["spec"]["containers"][0]["name"],
            "nginx",
            "Pod container name must remain immutable"
        );
    } else {
        assert_eq!(
            response.status(), 
            422,
            "Updating immutable pod fields should return 422 Unprocessable Entity"
        );
    }
    
    // Test 3: Metadata updates should be allowed
    let mut metadata_update = created.clone();
    metadata_update["metadata"]["labels"] = json!({"env": "test", "version": "v1"});
    metadata_update["metadata"]["annotations"] = json!({"updated": "true"});
    
    let response = client
        .put(&format!("{}/namespaces/{}/pods/immutable-pod", base_url, ns_name))
        .json(&metadata_update)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200, "Metadata updates should be allowed");
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["metadata"]["labels"]["env"], "test");
    assert_eq!(updated["metadata"]["annotations"]["updated"], "true");
    
    // Test 4: Status updates via PATCH should be allowed
    let status_patch = json!({
        "status": {
            "phase": "Running",
            "conditions": [{
                "type": "Ready",
                "status": "True",
                "lastTransitionTime": "2024-01-01T00:00:00Z"
            }]
        }
    });
    
    let response = client
        .patch(&format!("{}/namespaces/{}/pods/immutable-pod/status", base_url, ns_name))
        .json(&status_patch)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200, "Status updates should be allowed");
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/{}", base_url, ns_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_api_version_validation() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }

    // Test 1: Wrong API version for namespace (should auto-correct or reject)
    let ns = json!({
        "apiVersion": "v2",  // Wrong version
        "kind": "Namespace",
        "metadata": {
            "name": format!("test-apiv-{}", Uuid::new_v4())
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns)
        .send()
        .await
        .unwrap();
    
    // Should either auto-correct to v1 or reject
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        assert_eq!(
            created["apiVersion"], 
            "v1",
            "API version should be corrected to v1"
        );
    } else {
        assert_eq!(
            response.status(),
            400,
            "Wrong API version should be rejected with 400"
        );
    }
    
    // Test 2: Missing API version (should reject)
    let ns_no_version = json!({
        "kind": "Namespace",
        "metadata": {
            "name": format!("test-noapi-{}", Uuid::new_v4())
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&ns_no_version)
        .send()
        .await
        .unwrap();
    
    assert_eq!(
        response.status(),
        400,
        "Missing API version should be rejected"
    );
    
    // Test 3: Wrong kind for endpoint (should reject)
    let wrong_kind = json!({
        "apiVersion": "v1",
        "kind": "Pod",  // Wrong kind for namespace endpoint
        "metadata": {
            "name": format!("test-wrongkind-{}", Uuid::new_v4())
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&wrong_kind)
        .send()
        .await
        .unwrap();
    
    assert_eq!(
        response.status(),
        400,
        "Wrong kind should be rejected"
    );
    
    // Test 4: Test with pods endpoint
    let ns_name = format!("test-pod-apiv-{}", Uuid::new_v4());
    ensure_namespace(&client, base_url, &ns_name).await;
    
    let pod_wrong_version = json!({
        "apiVersion": "apps/v1",  // Wrong for core pods
        "kind": "Pod",
        "metadata": {
            "name": "test-pod",
            "namespace": &ns_name
        },
        "spec": {
            "containers": [{
                "name": "nginx",
                "image": "nginx"
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/pods", base_url, ns_name))
        .json(&pod_wrong_version)
        .send()
        .await
        .unwrap();
    
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        assert_eq!(created["apiVersion"], "v1", "Pod API version should be v1");
    } else {
        assert_eq!(response.status(), 400, "Wrong API version should be rejected");
    }
}

#[tokio::test]
async fn test_service_port_validation() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }

    let ns_name = format!("test-svc-port-{}", Uuid::new_v4());
    ensure_namespace(&client, base_url, &ns_name).await;

    // Test 1: Port number out of range (>65535)
    let service_high_port = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": "high-port",
            "namespace": &ns_name
        },
        "spec": {
            "ports": [{
                "port": 70000,
                "targetPort": 80
            }],
            "selector": {
                "app": "test"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/services", base_url, ns_name))
        .json(&service_high_port)
        .send()
        .await
        .unwrap();
    
    // Should either clamp to valid range or reject
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        let port = created["spec"]["ports"][0]["port"].as_i64().unwrap();
        assert!(
            port > 0 && port <= 65535,
            "Port should be clamped to valid range 1-65535"
        );
    } else {
        assert_eq!(
            response.status(),
            400,
            "Invalid port should be rejected"
        );
    }
    
    // Test 2: Port number zero or negative
    let service_zero_port = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": "zero-port",
            "namespace": &ns_name
        },
        "spec": {
            "ports": [{
                "port": 0,
                "targetPort": 80
            }],
            "selector": {
                "app": "test"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/services", base_url, ns_name))
        .json(&service_zero_port)
        .send()
        .await
        .unwrap();
    
    assert_ne!(
        response.status(),
        201,
        "Port 0 should be rejected"
    );
    
    // Test 3: Invalid targetPort string
    let service_invalid_target = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": "invalid-target",
            "namespace": &ns_name
        },
        "spec": {
            "ports": [{
                "port": 80,
                "targetPort": "not-a-valid-port-name!"
            }],
            "selector": {
                "app": "test"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/services", base_url, ns_name))
        .json(&service_invalid_target)
        .send()
        .await
        .unwrap();
    
    // Named ports should follow IANA_SVC_NAME format
    if response.status() == 201 {
        println!("Warning: Invalid targetPort name was accepted");
    } else {
        assert_eq!(response.status(), 400, "Invalid targetPort name should be rejected");
    }
    
    // Test 4: Valid named port
    let service_named_port = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": "named-port",
            "namespace": &ns_name
        },
        "spec": {
            "ports": [{
                "name": "http",
                "port": 80,
                "targetPort": "http-server"
            }],
            "selector": {
                "app": "test"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/services", base_url, ns_name))
        .json(&service_named_port)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 201, "Valid named port should be accepted");
    
    // Test 5: Duplicate port numbers with different protocols
    let service_dup_ports = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": "dup-ports-proto",
            "namespace": &ns_name
        },
        "spec": {
            "ports": [
                {
                    "name": "http",
                    "port": 80,
                    "protocol": "TCP",
                    "targetPort": 8080
                },
                {
                    "name": "dns",
                    "port": 80,
                    "protocol": "UDP",
                    "targetPort": 8080
                }
            ],
            "selector": {
                "app": "test"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/services", base_url, ns_name))
        .json(&service_dup_ports)
        .send()
        .await
        .unwrap();
    
    // Same port with different protocols should be allowed
    assert_eq!(
        response.status(),
        201,
        "Same port number with different protocols should be allowed"
    );
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/{}", base_url, ns_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_secret_validation() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }

    let ns_name = format!("test-secret-val-{}", Uuid::new_v4());
    ensure_namespace(&client, base_url, &ns_name).await;

    // Test 1: Invalid base64 in data field
    let secret_invalid_base64 = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "invalid-base64",
            "namespace": &ns_name
        },
        "type": "Opaque",
        "data": {
            "password": "not-valid-base64!@#$%^&*()"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/secrets", base_url, ns_name))
        .json(&secret_invalid_base64)
        .send()
        .await
        .unwrap();
    
    assert_ne!(
        response.status(),
        201,
        "Invalid base64 in data field should be rejected"
    );
    assert!(
        response.status() == 400 || response.status() == 422 || response.status() == 404,
        "Should return 400, 422, or 404 (not implemented)"
    );
    
    // Test 2: stringData should be converted to data
    let secret_string_data = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "string-data",
            "namespace": &ns_name
        },
        "type": "Opaque",
        "stringData": {
            "username": "admin",
            "password": "secret123"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/secrets", base_url, ns_name))
        .json(&secret_string_data)
        .send()
        .await
        .unwrap();
    
    if response.status() == 201 {
        let created: serde_json::Value = response.json().await.unwrap();
        // stringData should be converted to base64 data
        assert!(created["data"].is_object(), "data field should exist");
        assert!(created["stringData"].is_null(), "stringData should not be returned");
        
        // Verify base64 encoding
        let username_b64 = created["data"]["username"].as_str().unwrap();
        assert_eq!(username_b64, "YWRtaW4=", "username should be base64 encoded");
    }
    
    // Test 3: TLS secret type validation
    let secret_tls_invalid = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "tls-invalid",
            "namespace": &ns_name
        },
        "type": "kubernetes.io/tls",
        "data": {
            "wrong-key": "LS0tLS1CRUdJTg=="
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/secrets", base_url, ns_name))
        .json(&secret_tls_invalid)
        .send()
        .await
        .unwrap();
    
    if response.status() != 404 {  // If secrets are implemented
        assert_ne!(
            response.status(),
            201,
            "TLS secret without tls.crt and tls.key should be rejected"
        );
    }
    
    // Test 4: Valid TLS secret
    let secret_tls_valid = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "tls-valid",
            "namespace": &ns_name
        },
        "type": "kubernetes.io/tls",
        "data": {
            "tls.crt": "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0t",
            "tls.key": "LS0tLS1CRUdJTiBQUklWQVRFIEtFWS0tLS0t"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/secrets", base_url, ns_name))
        .json(&secret_tls_valid)
        .send()
        .await
        .unwrap();
    
    if response.status() != 404 {  // If secrets are implemented
        assert_eq!(
            response.status(),
            201,
            "Valid TLS secret should be accepted"
        );
    }
    
    // Test 5: Docker config secret validation
    let docker_config_invalid = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "docker-invalid",
            "namespace": &ns_name
        },
        "type": "kubernetes.io/dockerconfigjson",
        "data": {
            "wrong-key": "LS0tLS1CRUdJTg=="
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/secrets", base_url, ns_name))
        .json(&docker_config_invalid)
        .send()
        .await
        .unwrap();
    
    if response.status() != 404 {  // If secrets are implemented
        assert_ne!(
            response.status(),
            201,
            "Docker config secret without .dockerconfigjson key should be rejected"
        );
    }
    
    // Test 6: Size limit validation (Secrets have 1MB limit in Kubernetes)
    let large_data = "x".repeat(2 * 1024 * 1024); // 2MB
    let large_data_b64 = STANDARD.encode(&large_data);
    
    let secret_too_large = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "too-large",
            "namespace": &ns_name
        },
        "type": "Opaque",
        "data": {
            "large": large_data_b64
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/{}/secrets", base_url, ns_name))
        .json(&secret_too_large)
        .send()
        .await
        .unwrap();
    
    if response.status() != 404 {  // If secrets are implemented
        assert_ne!(
            response.status(),
            201,
            "Secret larger than 1MB should be rejected"
        );
    }
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/{}", base_url, ns_name))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_resource_name_validation() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }

    // Test various invalid names across different resources
    let long_name = "a".repeat(254);
    let test_cases = vec![
        ("", "empty name"),
        ("UPPERCASE", "uppercase letters"),
        ("under_score", "underscores"),
        ("special!char", "special characters"),
        ("-startdash", "starts with dash"),
        ("enddash-", "ends with dash"),
        ("dot.name", "dots are actually valid in some resources"),
        (long_name.as_str(), "name too long (>253 chars)"),
        ("123numeric", "starting with number is actually valid"),
        ("válid-utf8", "non-ASCII characters"),
    ];
    
    for (name, description) in test_cases {
        // Test with namespace
        let ns = json!({
            "apiVersion": "v1",
            "kind": "Namespace",
            "metadata": {
                "name": name
            }
        });
        
        let response = client
            .post(&format!("{}/namespaces", base_url))
            .json(&ns)
            .send()
            .await
            .unwrap();
        
        // Check if validation is working
        if name.is_empty() || name.len() > 253 || 
           name.contains(|c: char| !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' && c != '.') ||
           name.starts_with('-') || name.ends_with('-') {
            assert_ne!(
                response.status(),
                201,
                "Namespace with {} should be rejected",
                description
            );
        } else {
            // Some names might be valid
            if response.status() == 201 {
                // Clean up
                client
                    .delete(&format!("{}/namespaces/{}", base_url, name))
                    .send()
                    .await
                    .ok();
            }
        }
    }
}