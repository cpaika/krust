use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_pdb_crud() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/policy/v1";
    
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
            "name": "test-pdb"
        }
    });
    
    let _ = client
        .post("http://localhost:6443/api/v1/namespaces")
        .json(&namespace)
        .send()
        .await;
    
    // Create PodDisruptionBudget
    let pdb = json!({
        "apiVersion": "policy/v1",
        "kind": "PodDisruptionBudget",
        "metadata": {
            "name": "test-pdb",
            "namespace": "test-pdb"
        },
        "spec": {
            "minAvailable": 1,
            "selector": {
                "matchLabels": {
                    "app": "test-app"
                }
            },
            "unhealthyPodEvictionPolicy": "IfHealthyBudget"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/test-pdb/poddisruptionbudgets", base_url))
        .json(&pdb)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-pdb");
    
    // Get PodDisruptionBudget
    let response = client
        .get(&format!("{}/namespaces/test-pdb/poddisruptionbudgets/test-pdb", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    // Update PodDisruptionBudget
    let mut updated_pdb = created.clone();
    updated_pdb["spec"]["minAvailable"] = json!(2);
    
    let response = client
        .put(&format!("{}/namespaces/test-pdb/poddisruptionbudgets/test-pdb", base_url))
        .json(&updated_pdb)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    // List PodDisruptionBudgets
    let response = client
        .get(&format!("{}/namespaces/test-pdb/poddisruptionbudgets", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    
    // Update status
    let status_update = json!({
        "status": {
            "currentHealthy": 3,
            "desiredHealthy": 2,
            "disruptionsAllowed": 1,
            "expectedPods": 3,
            "observedGeneration": 1,
            "conditions": [
                {
                    "type": "SufficientPods",
                    "status": "True",
                    "lastTransitionTime": chrono::Utc::now().to_rfc3339(),
                    "reason": "EnoughPods",
                    "message": "Enough pods are available"
                }
            ]
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/test-pdb/poddisruptionbudgets/test-pdb/status", base_url))
        .json(&status_update)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    // Delete PodDisruptionBudget
    let response = client
        .delete(&format!("{}/namespaces/test-pdb/poddisruptionbudgets/test-pdb", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn test_pdb_with_max_unavailable() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/policy/v1";
    
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
            "name": "test-pdb-max"
        }
    });
    
    let _ = client
        .post("http://localhost:6443/api/v1/namespaces")
        .json(&namespace)
        .send()
        .await;
    
    // Create PodDisruptionBudget with maxUnavailable
    let pdb = json!({
        "apiVersion": "policy/v1",
        "kind": "PodDisruptionBudget",
        "metadata": {
            "name": "test-pdb-max",
            "namespace": "test-pdb-max"
        },
        "spec": {
            "maxUnavailable": "30%",
            "selector": {
                "matchLabels": {
                    "app": "test-app"
                }
            }
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/test-pdb-max/poddisruptionbudgets", base_url))
        .json(&pdb)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
}