use reqwest;
use serde_json::Value;

#[tokio::test]
async fn test_health_endpoints() {
    let client = reqwest::Client::new();
    
    // Note: Server should be running for integration tests
    // These tests assume the server is running on localhost:6443
    
    // Test liveness endpoint
    let resp = client
        .get("http://localhost:6443/livez")
        .send()
        .await;
    
    if resp.is_err() {
        eprintln!("Server not running, skipping integration test");
        return;
    }
    
    let resp = resp.unwrap();
    assert_eq!(resp.status(), 200);
    
    // Test readiness endpoint
    let resp = client
        .get("http://localhost:6443/readyz")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    
    // Test health endpoint
    let resp = client
        .get("http://localhost:6443/healthz")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_version_endpoint() {
    let client = reqwest::Client::new();
    
    let resp = client
        .get("http://localhost:6443/version")
        .send()
        .await;
    
    if resp.is_err() {
        eprintln!("Server not running, skipping integration test");
        return;
    }
    
    let resp = resp.unwrap();
    assert_eq!(resp.status(), 200);
    
    let version: Value = resp.json().await.unwrap();
    assert_eq!(version["major"], "1");
    assert_eq!(version["minor"], "29");
    assert!(version["gitVersion"].as_str().unwrap().contains("krust"));
}

#[tokio::test]
async fn test_api_discovery() {
    let client = reqwest::Client::new();
    
    let resp = client
        .get("http://localhost:6443/api")
        .send()
        .await;
    
    if resp.is_err() {
        eprintln!("Server not running, skipping integration test");
        return;
    }
    
    let resp = resp.unwrap();
    assert_eq!(resp.status(), 200);
    
    let api_versions: Value = resp.json().await.unwrap();
    assert_eq!(api_versions["kind"], "APIVersions");
    assert!(api_versions["versions"].as_array().unwrap().contains(&Value::String("v1".to_string())));
}

#[tokio::test]
async fn test_api_v1_resources() {
    let client = reqwest::Client::new();
    
    let resp = client
        .get("http://localhost:6443/api/v1")
        .send()
        .await;
    
    if resp.is_err() {
        eprintln!("Server not running, skipping integration test");
        return;
    }
    
    let resp = resp.unwrap();
    assert_eq!(resp.status(), 200);
    
    let resources: Value = resp.json().await.unwrap();
    assert_eq!(resources["kind"], "APIResourceList");
    assert_eq!(resources["groupVersion"], "v1");
    
    let resource_names: Vec<String> = resources["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_string())
        .collect();
    
    assert!(resource_names.contains(&"pods".to_string()));
    assert!(resource_names.contains(&"services".to_string()));
    assert!(resource_names.contains(&"namespaces".to_string()));
    assert!(resource_names.contains(&"nodes".to_string()));
}