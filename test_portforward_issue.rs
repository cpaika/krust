// Test to reproduce and verify the port forwarding issue
// Run with: cargo test --test test_portforward_issue -- --nocapture

use reqwest;
use serde_json::json;
use std::process::Command;
use tokio;

#[tokio::test]
async fn test_port_forward_issue() {
    println!("Testing port forwarding issue with nginx on port 80...");
    
    // First, ensure Krust is running
    // This test assumes Krust is already running on localhost:6443
    
    // Check if Krust is running
    let client = reqwest::Client::new();
    let health_check = client.get("http://localhost:6443/livez")
        .send()
        .await;
    
    if health_check.is_err() {
        panic!("Krust is not running. Please start it with 'cargo run' first.");
    }
    
    // Create a test pod with nginx on port 80
    println!("Creating nginx pod...");
    let pod_spec = json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": "nginx-test",
            "namespace": "default"
        },
        "spec": {
            "containers": [{
                "name": "nginx",
                "image": "nginx",
                "ports": [{
                    "containerPort": 80
                }]
            }]
        }
    });
    
    // Delete existing pod if it exists
    let _ = client.delete("http://localhost:6443/api/v1/namespaces/default/pods/nginx-test")
        .send()
        .await;
    
    // Wait a bit for deletion
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    // Create the pod
    let create_response = client.post("http://localhost:6443/api/v1/namespaces/default/pods")
        .json(&pod_spec)
        .send()
        .await
        .expect("Failed to create pod");
    
    if !create_response.status().is_success() {
        let error_text = create_response.text().await.unwrap_or_default();
        panic!("Failed to create pod: {}", error_text);
    }
    
    println!("Pod created successfully");
    
    // Update pod status to Running (simulate kubelet)
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    // Test the port forwarding endpoint directly
    println!("\nTesting port forward endpoint with different port configurations:");
    
    // Test 1: Port forward with explicit mapping 8080:80
    println!("\n1. Testing explicit port mapping 8080:80");
    test_port_forward_request(&client, "8080:80").await;
    
    // Test 2: Port forward with single port (should use same port for local and remote)
    println!("\n2. Testing single port 80");
    test_port_forward_request(&client, "80").await;
    
    // Test 3: Port forward with kubectl command
    println!("\n3. Testing with actual kubectl command");
    test_kubectl_port_forward().await;
    
    // Cleanup
    let _ = client.delete("http://localhost:6443/api/v1/namespaces/default/pods/nginx-test")
        .send()
        .await;
    
    println!("\n✅ Test completed");
}

async fn test_port_forward_request(client: &reqwest::Client, ports: &str) {
    println!("  Testing port forward with ports={}", ports);
    
    // Make a WebSocket upgrade request
    let url = format!("http://localhost:6443/api/v1/namespaces/default/pods/nginx-test/portforward?ports={}", ports);
    
    // We can't easily test WebSocket in a unit test, so let's at least verify the endpoint exists
    let response = client.get(&url)
        .header("Connection", "Upgrade")
        .header("Upgrade", "SPDY/3.1+portforward.k8s.io")
        .send()
        .await
        .expect("Failed to send request");
    
    println!("  Response status: {}", response.status());
    
    // The response should be 101 Switching Protocols for a valid WebSocket upgrade
    // or 409 Conflict if the pod is not running
    // or 404 if the pod doesn't exist
    
    if response.status() == 101 {
        println!("  ✓ WebSocket upgrade successful");
    } else if response.status() == 409 {
        println!("  ⚠ Pod not in Running state");
    } else if response.status() == 404 {
        println!("  ⚠ Pod not found");
    } else {
        println!("  ✗ Unexpected status: {}", response.status());
    }
}

async fn test_kubectl_port_forward() {
    println!("  Running: kubectl port-forward pod/nginx-test 8080:80");
    
    // Start kubectl port-forward in background
    let mut child = Command::new("kubectl")
        .args(&["port-forward", "pod/nginx-test", "8080:80"])
        .spawn()
        .expect("Failed to start kubectl port-forward");
    
    // Wait a moment for it to connect
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // Check if the process is still running
    match child.try_wait() {
        Ok(Some(status)) => {
            println!("  ✗ kubectl port-forward exited with status: {}", status);
            println!("    This indicates the port forwarding failed immediately");
        }
        Ok(None) => {
            println!("  ✓ kubectl port-forward is running");
            
            // Try to connect to the forwarded port
            let client = reqwest::Client::new();
            match client.get("http://localhost:8080").send().await {
                Ok(response) => {
                    println!("  ✓ Successfully connected to forwarded port");
                    println!("    Response status: {}", response.status());
                }
                Err(e) => {
                    println!("  ✗ Failed to connect to forwarded port: {}", e);
                }
            }
            
            // Kill the port-forward process
            let _ = child.kill();
        }
        Err(e) => {
            println!("  ✗ Error checking process status: {}", e);
        }
    }
}

// Helper function to parse port mappings (for testing the parsing logic)
fn parse_port_mappings(ports_str: &str) -> Vec<(u16, u16)> {
    let mut mappings = Vec::new();
    
    for port_spec in ports_str.split(',') {
        let port_spec = port_spec.trim();
        if port_spec.is_empty() {
            continue;
        }
        
        let parts: Vec<&str> = port_spec.split(':').collect();
        let mapping = match parts.len() {
            1 => {
                if let Ok(port) = parts[0].parse::<u16>() {
                    Some((port, port))  // (local, remote)
                } else {
                    None
                }
            }
            2 => {
                if let (Ok(local), Ok(remote)) = (parts[0].parse::<u16>(), parts[1].parse::<u16>()) {
                    Some((local, remote))
                } else {
                    None
                }
            }
            _ => None,
        };
        
        if let Some(mapping) = mapping {
            mappings.push(mapping);
        }
    }
    
    mappings
}

#[test]
fn test_port_parsing() {
    println!("\nTesting port parsing logic:");
    
    // Test various port configurations
    let test_cases = vec![
        ("8080:80", vec![(8080, 80)]),
        ("80", vec![(80, 80)]),
        ("8080:80,8443:443", vec![(8080, 80), (8443, 443)]),
        ("", vec![]),
    ];
    
    for (input, expected) in test_cases {
        let result = parse_port_mappings(input);
        println!("  Input: '{}' -> {:?}", input, result);
        assert_eq!(result, expected, "Failed for input: {}", input);
    }
    
    println!("✅ Port parsing tests passed");
}