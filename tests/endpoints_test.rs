use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
#[ignore] // Run with: cargo test --test endpoints_test -- --ignored --nocapture
fn test_endpoints_automatic_creation() {
    println!("=== Testing Automatic Endpoints Creation for Services ===");
    
    // Clean state
    Command::new("pkill")
        .args(&["-f", "target/release/krust"])
        .output()
        .ok();
    
    thread::sleep(Duration::from_secs(1));
    
    Command::new("rm")
        .args(&["-f", "krust.db"])
        .output()
        .ok();
    
    // Start server
    println!("Starting Krust server...");
    let mut server = Command::new("./target/release/krust")
        .spawn()
        .expect("Failed to start server");
    
    thread::sleep(Duration::from_secs(2));
    
    // Create a service with selector
    let service_yaml = r#"
apiVersion: v1
kind: Service
metadata:
  name: test-service
spec:
  selector:
    app: test-app
  ports:
  - port: 80
    targetPort: 8080
"#;
    
    std::fs::write("test-service.yaml", service_yaml).expect("Failed to write service yaml");
    
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "test-service.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create service");
    
    assert!(output.status.success(), "Failed to create service");
    
    // Check that endpoints were created automatically
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "endpoints",
            "test-service",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get endpoints");
    
    let endpoints_json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse endpoints JSON");
    
    // Verify endpoints exist but have no addresses (no pods match yet)
    assert_eq!(
        endpoints_json["metadata"]["name"].as_str(),
        Some("test-service"),
        "Endpoints name mismatch"
    );
    
    let subsets = &endpoints_json["subsets"];
    assert!(subsets.is_array(), "Endpoints should have subsets array");
    assert_eq!(subsets.as_array().unwrap().len(), 0, "Should have no addresses initially");
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "service",
            "test-service"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "test-service.yaml"])
        .output()
        .ok();
    
    server.kill().expect("Failed to kill server");
    
    println!("\n✅ Automatic endpoints creation test passed!");
}

#[test]
#[ignore]
fn test_endpoints_pod_selection() {
    println!("=== Testing Endpoints Pod Selection ===");
    
    // Clean state
    Command::new("pkill")
        .args(&["-f", "target/release/krust"])
        .output()
        .ok();
    
    thread::sleep(Duration::from_secs(1));
    
    Command::new("rm")
        .args(&["-f", "krust.db"])
        .output()
        .ok();
    
    // Start server
    let mut server = Command::new("./target/release/krust")
        .spawn()
        .expect("Failed to start server");
    
    thread::sleep(Duration::from_secs(2));
    
    // Create pods with matching labels
    let pod1_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: app-pod-1
  labels:
    app: myapp
    version: v1
spec:
  containers:
  - name: app
    image: nginx:alpine
    ports:
    - containerPort: 80
"#;
    
    let pod2_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: app-pod-2
  labels:
    app: myapp
    version: v2
spec:
  containers:
  - name: app
    image: nginx:alpine
    ports:
    - containerPort: 80
"#;
    
    let pod3_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: other-pod
  labels:
    app: otherapp
spec:
  containers:
  - name: app
    image: nginx:alpine
    ports:
    - containerPort: 80
"#;
    
    std::fs::write("pod1.yaml", pod1_yaml).expect("Failed to write pod yaml");
    std::fs::write("pod2.yaml", pod2_yaml).expect("Failed to write pod yaml");
    std::fs::write("pod3.yaml", pod3_yaml).expect("Failed to write pod yaml");
    
    // Create all pods
    for pod_file in &["pod1.yaml", "pod2.yaml", "pod3.yaml"] {
        Command::new("kubectl")
            .args(&[
                "--server=http://localhost:6443",
                "apply",
                "-f",
                pod_file,
                "--validate=false"
            ])
            .output()
            .expect(&format!("Failed to create pod from {}", pod_file));
    }
    
    // Wait for pods to be running and get IPs
    thread::sleep(Duration::from_secs(5));
    
    // Create service with selector for app=myapp
    let service_yaml = r#"
apiVersion: v1
kind: Service
metadata:
  name: myapp-service
spec:
  selector:
    app: myapp
  ports:
  - port: 80
    targetPort: 80
"#;
    
    std::fs::write("service.yaml", service_yaml).expect("Failed to write service yaml");
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "service.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create service");
    
    // Give the controller time to update endpoints
    thread::sleep(Duration::from_secs(2));
    
    // Get endpoints
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "endpoints",
            "myapp-service",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get endpoints");
    
    let endpoints_json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse endpoints JSON");
    
    // Verify that only pods with app=myapp are included
    let subsets = endpoints_json["subsets"].as_array().unwrap();
    assert_eq!(subsets.len(), 1, "Should have one subset");
    
    let addresses = subsets[0]["addresses"].as_array().unwrap();
    assert_eq!(addresses.len(), 2, "Should have 2 addresses (app-pod-1 and app-pod-2)");
    
    // Verify the pod names in the addresses
    let pod_names: Vec<String> = addresses.iter()
        .filter_map(|addr| addr["targetRef"]["name"].as_str())
        .map(|s| s.to_string())
        .collect();
    
    assert!(pod_names.contains(&"app-pod-1".to_string()), "app-pod-1 should be in endpoints");
    assert!(pod_names.contains(&"app-pod-2".to_string()), "app-pod-2 should be in endpoints");
    assert!(!pod_names.contains(&"other-pod".to_string()), "other-pod should NOT be in endpoints");
    
    // Verify ports are set correctly
    let ports = subsets[0]["ports"].as_array().unwrap();
    assert_eq!(ports.len(), 1, "Should have one port");
    assert_eq!(ports[0]["port"].as_i64(), Some(80), "Port should be 80");
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "pod",
            "app-pod-1",
            "app-pod-2", 
            "other-pod"
        ])
        .output()
        .ok();
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "service",
            "myapp-service"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "pod1.yaml", "pod2.yaml", "pod3.yaml", "service.yaml"])
        .output()
        .ok();
    
    server.kill().ok();
    
    println!("\n✅ Endpoints pod selection test passed!");
}

#[test]
#[ignore]
fn test_endpoints_dynamic_update() {
    println!("=== Testing Dynamic Endpoints Update ===");
    
    // Clean state
    Command::new("pkill")
        .args(&["-f", "target/release/krust"])
        .output()
        .ok();
    
    thread::sleep(Duration::from_secs(1));
    
    Command::new("rm")
        .args(&["-f", "krust.db"])
        .output()
        .ok();
    
    // Start server
    let mut server = Command::new("./target/release/krust")
        .spawn()
        .expect("Failed to start server");
    
    thread::sleep(Duration::from_secs(2));
    
    // Create service first
    let service_yaml = r#"
apiVersion: v1
kind: Service
metadata:
  name: dynamic-service
spec:
  selector:
    app: dynamic
  ports:
  - port: 80
"#;
    
    std::fs::write("service.yaml", service_yaml).expect("Failed to write service yaml");
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "service.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create service");
    
    // Check endpoints are empty initially
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "endpoints",
            "dynamic-service",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get endpoints");
    
    let endpoints_json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse endpoints JSON");
    
    let subsets = endpoints_json["subsets"].as_array().unwrap();
    assert_eq!(subsets.len(), 0, "Should have no addresses initially");
    
    // Add a pod with matching label
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: dynamic-pod
  labels:
    app: dynamic
spec:
  containers:
  - name: app
    image: nginx:alpine
"#;
    
    std::fs::write("pod.yaml", pod_yaml).expect("Failed to write pod yaml");
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "pod.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create pod");
    
    // Wait for pod to be running and endpoints to update
    thread::sleep(Duration::from_secs(5));
    
    // Check endpoints now have the pod
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "endpoints",
            "dynamic-service",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get endpoints");
    
    let endpoints_json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse endpoints JSON");
    
    let subsets = endpoints_json["subsets"].as_array().unwrap();
    assert_eq!(subsets.len(), 1, "Should have one subset after pod creation");
    
    let addresses = subsets[0]["addresses"].as_array().unwrap();
    assert_eq!(addresses.len(), 1, "Should have one address");
    assert_eq!(
        addresses[0]["targetRef"]["name"].as_str(),
        Some("dynamic-pod"),
        "Should reference the created pod"
    );
    
    // Delete the pod
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "pod",
            "dynamic-pod"
        ])
        .output()
        .expect("Failed to delete pod");
    
    // Wait for endpoints to update
    thread::sleep(Duration::from_secs(3));
    
    // Check endpoints are empty again
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "endpoints",
            "dynamic-service",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get endpoints");
    
    let endpoints_json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse endpoints JSON");
    
    let subsets = endpoints_json["subsets"].as_array().unwrap();
    assert_eq!(subsets.len(), 0, "Should have no addresses after pod deletion");
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "service",
            "dynamic-service"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "service.yaml", "pod.yaml"])
        .output()
        .ok();
    
    server.kill().ok();
    
    println!("\n✅ Dynamic endpoints update test passed!");
}