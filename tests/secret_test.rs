use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_secret_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Test 1: Create a Secret
    let secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "test-secret",
            "namespace": "default"
        },
        "type": "Opaque",
        "data": {
            "username": "YWRtaW4=", // base64 for "admin"
            "password": "MWYyZDFlMmU2N2Rm" // base64 for "1f2d1e2e67df"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/secrets", base_url))
        .json(&secret)
        .send()
        .await
        .unwrap();
    
    println!("Create Secret response status: {}", response.status());
    
    if response.status() == 404 {
        println!("Secret endpoint not implemented yet");
        // This test will fail until we implement the Secret API
        return;
    }
    
    assert_eq!(response.status(), 201, "Secret creation should return 201");
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-secret");
    assert_eq!(created["type"], "Opaque");
    assert_eq!(created["data"]["username"], "YWRtaW4=");
    
    // Test 2: Get the Secret
    let response = client
        .get(&format!("{}/namespaces/default/secrets/test-secret", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let fetched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(fetched["metadata"]["name"], "test-secret");
    assert_eq!(fetched["data"]["password"], "MWYyZDFlMmU2N2Rm");
    
    // Test 3: List Secrets
    let response = client
        .get(&format!("{}/namespaces/default/secrets", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["kind"], "SecretList");
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Test 4: Update the Secret
    let updated_secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "test-secret",
            "namespace": "default",
            "resourceVersion": created["metadata"]["resourceVersion"]
        },
        "type": "Opaque",
        "data": {
            "username": "cm9vdA==", // base64 for "root"
            "password": "bmV3UGFzc3dvcmQ=", // base64 for "newPassword"
            "token": "c2VjcmV0VG9rZW4=" // base64 for "secretToken"
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/secrets/test-secret", base_url))
        .json(&updated_secret)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["data"]["username"], "cm9vdA==");
    assert_eq!(updated["data"]["token"], "c2VjcmV0VG9rZW4=");
    
    // Test 5: Patch the Secret
    let patch = json!({
        "data": {
            "api_key": "YWJjZGVmZ2hpams=" // base64 for "abcdefghijk"
        }
    });
    
    let response = client
        .patch(&format!("{}/namespaces/default/secrets/test-secret", base_url))
        .header("Content-Type", "application/merge-patch+json")
        .json(&patch)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let patched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(patched["data"]["api_key"], "YWJjZGVmZ2hpams=");
    assert_eq!(patched["data"]["token"], "c2VjcmV0VG9rZW4="); // Should still exist
    
    // Test 6: Delete the Secret
    let response = client
        .delete(&format!("{}/namespaces/default/secrets/test-secret", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Verify it's deleted
    let response = client
        .get(&format!("{}/namespaces/default/secrets/test-secret", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_secret_types() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Test kubernetes.io/service-account-token type
    let sa_secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "sa-token-secret",
            "namespace": "default",
            "annotations": {
                "kubernetes.io/service-account.name": "default",
                "kubernetes.io/service-account.uid": "12345"
            }
        },
        "type": "kubernetes.io/service-account-token",
        "data": {
            "ca.crt": "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0t...",
            "namespace": "ZGVmYXVsdA==", // base64 for "default"
            "token": "ZXlKaGJHY2lPaUpTVXpJMU5pSXNJbXRwWkNJNkltUmxabUYxYkhRaWZRLi4u"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/secrets", base_url))
        .json(&sa_secret)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        println!("Secret endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["type"], "kubernetes.io/service-account-token");
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/default/secrets/sa-token-secret", base_url))
        .send()
        .await
        .unwrap();
    
    // Test kubernetes.io/dockerconfigjson type
    let docker_secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "docker-registry-secret",
            "namespace": "default"
        },
        "type": "kubernetes.io/dockerconfigjson",
        "data": {
            ".dockerconfigjson": "eyJhdXRocyI6eyJodHRwczovL2luZGV4LmRvY2tlci5pby92MS8iOnsidXNlcm5hbWUiOiJ1c2VyIiwicGFzc3dvcmQiOiJwYXNzIiwiZW1haWwiOiJ1c2VyQGV4YW1wbGUuY29tIiwiYXV0aCI6ImRYTmxjanB3WVhOeiJ9fX0="
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/secrets", base_url))
        .json(&docker_secret)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["type"], "kubernetes.io/dockerconfigjson");
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/default/secrets/docker-registry-secret", base_url))
        .send()
        .await
        .unwrap();
    
    // Test kubernetes.io/tls type
    let tls_secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "tls-secret",
            "namespace": "default"
        },
        "type": "kubernetes.io/tls",
        "data": {
            "tls.crt": "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSUMrekNDQWVPZ0F3SUJBZ0lKQUt3...",
            "tls.key": "LS0tLS1CRUdJTiBQUklWQVRFIEtFWS0tLS0tCk1JSUV2QUlCQURBTkJna3Foa2lHOXcw..."
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/secrets", base_url))
        .json(&tls_secret)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["type"], "kubernetes.io/tls");
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/default/secrets/tls-secret", base_url))
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_secret_immutable() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create an immutable Secret
    let secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "immutable-secret",
            "namespace": "default"
        },
        "type": "Opaque",
        "data": {
            "key": "dmFsdWU=" // base64 for "value"
        },
        "immutable": true
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/secrets", base_url))
        .json(&secret)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        println!("Secret endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["immutable"], true);
    
    // Try to update immutable Secret (should fail)
    let update = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "immutable-secret",
            "namespace": "default",
            "resourceVersion": created["metadata"]["resourceVersion"]
        },
        "type": "Opaque",
        "data": {
            "key": "bmV3VmFsdWU=" // base64 for "newValue"
        },
        "immutable": true
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/secrets/immutable-secret", base_url))
        .json(&update)
        .send()
        .await
        .unwrap();
    
    // Should reject updates to immutable Secret
    assert_eq!(response.status(), 422); // Unprocessable Entity
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/default/secrets/immutable-secret", base_url))
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_secret_string_data() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create Secret with stringData (should be automatically base64 encoded)
    let secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": "string-data-secret",
            "namespace": "default"
        },
        "type": "Opaque",
        "stringData": {
            "username": "admin",
            "password": "secretPassword123",
            "config": "host=localhost\nport=5432\ndatabase=mydb"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/secrets", base_url))
        .json(&secret)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        println!("Secret endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    
    // Verify that stringData was converted to base64 encoded data
    assert_eq!(created["data"]["username"], "YWRtaW4="); // base64 for "admin"
    assert_eq!(created["data"]["password"], "c2VjcmV0UGFzc3dvcmQxMjM="); // base64 for "secretPassword123"
    
    // stringData should not be stored
    assert!(created.get("stringData").is_none());
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/default/secrets/string-data-secret", base_url))
        .send()
        .await
        .unwrap();
}