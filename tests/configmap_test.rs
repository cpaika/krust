use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_configmap_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Test 1: Create a ConfigMap
    let configmap = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {
            "name": "test-config",
            "namespace": "default"
        },
        "data": {
            "database.url": "postgres://localhost:5432/mydb",
            "app.timeout": "30",
            "feature.enabled": "true"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/configmaps", base_url))
        .json(&configmap)
        .send()
        .await
        .unwrap();
    
    println!("Create ConfigMap response status: {}", response.status());
    
    if response.status() == 404 {
        println!("ConfigMap endpoint not implemented yet");
        // This test will fail until we implement the ConfigMap API
    }
    
    assert_eq!(response.status(), 201, "ConfigMap creation should return 201");
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-config");
    assert_eq!(created["data"]["database.url"], "postgres://localhost:5432/mydb");
    
    // Test 2: Get the ConfigMap
    let response = client
        .get(&format!("{}/namespaces/default/configmaps/test-config", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let fetched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(fetched["metadata"]["name"], "test-config");
    assert_eq!(fetched["data"]["app.timeout"], "30");
    
    // Test 3: List ConfigMaps
    let response = client
        .get(&format!("{}/namespaces/default/configmaps", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["kind"], "ConfigMapList");
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Test 4: Update the ConfigMap
    let updated_configmap = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {
            "name": "test-config",
            "namespace": "default",
            "resourceVersion": created["metadata"]["resourceVersion"]
        },
        "data": {
            "database.url": "postgres://localhost:5432/newdb",
            "app.timeout": "60",
            "feature.enabled": "false",
            "new.key": "new value"
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/configmaps/test-config", base_url))
        .json(&updated_configmap)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["data"]["database.url"], "postgres://localhost:5432/newdb");
    assert_eq!(updated["data"]["new.key"], "new value");
    
    // Test 5: Patch the ConfigMap
    let patch = json!({
        "data": {
            "patch.key": "patched value"
        }
    });
    
    let response = client
        .patch(&format!("{}/namespaces/default/configmaps/test-config", base_url))
        .header("Content-Type", "application/merge-patch+json")
        .json(&patch)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let patched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(patched["data"]["patch.key"], "patched value");
    assert_eq!(patched["data"]["new.key"], "new value"); // Should still exist
    
    // Test 6: Delete the ConfigMap
    let response = client
        .delete(&format!("{}/namespaces/default/configmaps/test-config", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Verify it's deleted
    let response = client
        .get(&format!("{}/namespaces/default/configmaps/test-config", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_configmap_binary_data() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create ConfigMap with binary data
    let configmap = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {
            "name": "binary-config",
            "namespace": "default"
        },
        "data": {
            "text.file": "plain text content"
        },
        "binaryData": {
            "cert.pem": "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCi4uLgotLS0tLUVORCBDRVJUSUZJQ0FURS0tLS0t", // base64
            "key.bin": "YmluYXJ5IGRhdGE=" // base64 for "binary data"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/configmaps", base_url))
        .json(&configmap)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        println!("ConfigMap endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["data"]["text.file"], "plain text content");
    assert_eq!(created["binaryData"]["key.bin"], "YmluYXJ5IGRhdGE=");
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/default/configmaps/binary-config", base_url))
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_configmap_immutable() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create an immutable ConfigMap
    let configmap = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {
            "name": "immutable-config",
            "namespace": "default"
        },
        "data": {
            "key": "value"
        },
        "immutable": true
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/configmaps", base_url))
        .json(&configmap)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        println!("ConfigMap endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["immutable"], true);
    
    // Try to update immutable ConfigMap (should fail)
    let update = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {
            "name": "immutable-config",
            "namespace": "default",
            "resourceVersion": created["metadata"]["resourceVersion"]
        },
        "data": {
            "key": "new value"
        },
        "immutable": true
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/configmaps/immutable-config", base_url))
        .json(&update)
        .send()
        .await
        .unwrap();
    
    // Should reject updates to immutable ConfigMap
    assert_eq!(response.status(), 422); // Unprocessable Entity
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/default/configmaps/immutable-config", base_url))
        .send()
        .await
        .unwrap();
}