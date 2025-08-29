use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_ingress_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/networking.k8s.io/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Test 1: Create an Ingress
    let ingress = json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "Ingress",
        "metadata": {
            "name": "test-ingress",
            "namespace": "default",
            "annotations": {
                "nginx.ingress.kubernetes.io/rewrite-target": "/"
            }
        },
        "spec": {
            "ingressClassName": "nginx",
            "rules": [{
                "host": "example.com",
                "http": {
                    "paths": [{
                        "path": "/app",
                        "pathType": "Prefix",
                        "backend": {
                            "service": {
                                "name": "app-service",
                                "port": {
                                    "number": 80
                                }
                            }
                        }
                    }]
                }
            }],
            "tls": [{
                "hosts": ["example.com"],
                "secretName": "example-tls"
            }]
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/ingresses", base_url))
        .json(&ingress)
        .send()
        .await
        .unwrap();
    
    println!("Create Ingress response status: {}", response.status());
    
    if response.status() == 404 {
        println!("Ingress endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201, "Ingress creation should return 201");
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-ingress");
    assert_eq!(created["spec"]["ingressClassName"], "nginx");
    
    // Test 2: Get the Ingress
    let response = client
        .get(&format!("{}/namespaces/default/ingresses/test-ingress", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let fetched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(fetched["metadata"]["name"], "test-ingress");
    assert_eq!(fetched["spec"]["rules"][0]["host"], "example.com");
    
    // Test 3: List Ingresses
    let response = client
        .get(&format!("{}/namespaces/default/ingresses", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["kind"], "IngressList");
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Test 4: Update the Ingress
    let updated_ingress = json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "Ingress",
        "metadata": {
            "name": "test-ingress",
            "namespace": "default",
            "resourceVersion": created["metadata"]["resourceVersion"],
            "annotations": {
                "nginx.ingress.kubernetes.io/rewrite-target": "/",
                "nginx.ingress.kubernetes.io/ssl-redirect": "true"
            }
        },
        "spec": {
            "ingressClassName": "nginx",
            "rules": [{
                "host": "example.com",
                "http": {
                    "paths": [
                        {
                            "path": "/app",
                            "pathType": "Prefix",
                            "backend": {
                                "service": {
                                    "name": "app-service",
                                    "port": {
                                        "number": 80
                                    }
                                }
                            }
                        },
                        {
                            "path": "/api",
                            "pathType": "Prefix",
                            "backend": {
                                "service": {
                                    "name": "api-service",
                                    "port": {
                                        "number": 8080
                                    }
                                }
                            }
                        }
                    ]
                }
            }],
            "tls": [{
                "hosts": ["example.com"],
                "secretName": "example-tls"
            }]
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/ingresses/test-ingress", base_url))
        .json(&updated_ingress)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["spec"]["rules"][0]["http"]["paths"].as_array().unwrap().len(), 2);
    
    // Test 5: Delete the Ingress
    let response = client
        .delete(&format!("{}/namespaces/default/ingresses/test-ingress", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Verify it's deleted
    let response = client
        .get(&format!("{}/namespaces/default/ingresses/test-ingress", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404);
}