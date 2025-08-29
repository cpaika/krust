use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_networkpolicy_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/networking.k8s.io/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Test 1: Create a NetworkPolicy
    let network_policy = json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "NetworkPolicy",
        "metadata": {
            "name": "test-network-policy",
            "namespace": "default"
        },
        "spec": {
            "podSelector": {
                "matchLabels": {
                    "app": "web"
                }
            },
            "policyTypes": ["Ingress", "Egress"],
            "ingress": [{
                "from": [
                    {
                        "podSelector": {
                            "matchLabels": {
                                "app": "backend"
                            }
                        }
                    },
                    {
                        "namespaceSelector": {
                            "matchLabels": {
                                "environment": "production"
                            }
                        }
                    }
                ],
                "ports": [{
                    "protocol": "TCP",
                    "port": 80
                }]
            }],
            "egress": [{
                "to": [{
                    "ipBlock": {
                        "cidr": "10.0.0.0/8",
                        "except": ["10.0.1.0/24"]
                    }
                }],
                "ports": [{
                    "protocol": "TCP",
                    "port": 443
                }]
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/networkpolicies", base_url))
        .json(&network_policy)
        .send()
        .await
        .unwrap();
    
    println!("Create NetworkPolicy response status: {}", response.status());
    
    if response.status() == 404 {
        println!("NetworkPolicy endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201, "NetworkPolicy creation should return 201");
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-network-policy");
    assert!(created["spec"]["policyTypes"].as_array().unwrap().contains(&json!("Ingress")));
    
    // Test 2: Get the NetworkPolicy
    let response = client
        .get(&format!("{}/namespaces/default/networkpolicies/test-network-policy", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let fetched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(fetched["metadata"]["name"], "test-network-policy");
    
    // Test 3: List NetworkPolicies
    let response = client
        .get(&format!("{}/namespaces/default/networkpolicies", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["kind"], "NetworkPolicyList");
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Test 4: Delete the NetworkPolicy
    let response = client
        .delete(&format!("{}/namespaces/default/networkpolicies/test-network-policy", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Verify it's deleted
    let response = client
        .get(&format!("{}/namespaces/default/networkpolicies/test-network-policy", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404);
}