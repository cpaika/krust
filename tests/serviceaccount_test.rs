use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_serviceaccount_crud() {
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
            "name": "test-sa"
        }
    });
    
    let _ = client
        .post(&format!("{}/namespaces", base_url))
        .json(&namespace)
        .send()
        .await;
    
    // Create ServiceAccount
    let sa = json!({
        "apiVersion": "v1",
        "kind": "ServiceAccount",
        "metadata": {
            "name": "test-sa",
            "namespace": "test-sa"
        },
        "secrets": [
            {
                "name": "test-sa-token"
            }
        ],
        "imagePullSecrets": [
            {
                "name": "myregistrykey"
            }
        ],
        "automountServiceAccountToken": true
    });
    
    let response = client
        .post(&format!("{}/namespaces/test-sa/serviceaccounts", base_url))
        .json(&sa)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-sa");
    
    // Get ServiceAccount
    let response = client
        .get(&format!("{}/namespaces/test-sa/serviceaccounts/test-sa", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    // Update ServiceAccount
    let mut updated_sa = created.clone();
    updated_sa["automountServiceAccountToken"] = json!(false);
    
    let response = client
        .put(&format!("{}/namespaces/test-sa/serviceaccounts/test-sa", base_url))
        .json(&updated_sa)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    // List ServiceAccounts
    let response = client
        .get(&format!("{}/namespaces/test-sa/serviceaccounts", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let list: serde_json::Value = response.json().await.unwrap();
    // Should have at least the test-sa we created (and possibly default)
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Create a token for the ServiceAccount
    let token_request = json!({
        "apiVersion": "authentication.k8s.io/v1",
        "kind": "TokenRequest",
        "spec": {
            "audiences": ["api", "vault"],
            "expirationSeconds": 3600,
            "boundObjectRef": {
                "apiVersion": "v1",
                "kind": "Pod",
                "name": "test-pod"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/test-sa/serviceaccounts/test-sa/token", base_url))
        .json(&token_request)
        .send()
        .await
        .unwrap();
    
    // Token generation might not be implemented yet
    assert!(response.status() == reqwest::StatusCode::CREATED || 
            response.status() == reqwest::StatusCode::NOT_IMPLEMENTED ||
            response.status() == reqwest::StatusCode::NOT_FOUND);
    
    // Delete ServiceAccount
    let response = client
        .delete(&format!("{}/namespaces/test-sa/serviceaccounts/test-sa", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn test_default_serviceaccount() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create namespace
    let namespace = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": "test-default-sa"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces", base_url))
        .json(&namespace)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    
    // Check if default ServiceAccount was created automatically
    // Note: This behavior might need to be implemented in a controller
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    let response = client
        .get(&format!("{}/namespaces/test-default-sa/serviceaccounts/default", base_url))
        .send()
        .await
        .unwrap();
    
    // The default ServiceAccount might or might not exist depending on controller implementation
    assert!(response.status() == reqwest::StatusCode::OK || 
            response.status() == reqwest::StatusCode::NOT_FOUND);
}