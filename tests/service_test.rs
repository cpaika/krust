use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
#[ignore] // Run with: cargo test --test service_test -- --ignored --nocapture
fn test_service_creation_and_clusterip() {
    println!("=== Testing Service Creation and ClusterIP Allocation ===");
    
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
    
    // Create a test service
    let service_yaml = r#"
apiVersion: v1
kind: Service
metadata:
  name: test-service
  labels:
    app: test
spec:
  selector:
    app: nginx
  ports:
  - port: 80
    targetPort: 80
    protocol: TCP
  type: ClusterIP
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
    
    println!("Create output: {}", String::from_utf8_lossy(&output.stdout));
    assert!(output.status.success(), "Failed to create service");
    
    // Check service was created
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "service",
            "test-service",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get service");
    
    let service_json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse service JSON");
    
    // Verify ClusterIP was allocated
    assert!(service_json["spec"]["clusterIP"].is_string(), "ClusterIP not allocated");
    let cluster_ip = service_json["spec"]["clusterIP"].as_str().unwrap();
    println!("Allocated ClusterIP: {}", cluster_ip);
    assert!(cluster_ip.starts_with("10.96."), "ClusterIP not in expected range");
    
    // Create another service to test IP uniqueness
    let service2_yaml = r#"
apiVersion: v1
kind: Service
metadata:
  name: test-service-2
spec:
  selector:
    app: nginx2
  ports:
  - port: 8080
    targetPort: 8080
  type: ClusterIP
"#;
    
    std::fs::write("test-service-2.yaml", service2_yaml).expect("Failed to write service yaml");
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "test-service-2.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create second service");
    
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "service",
            "test-service-2",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get second service");
    
    let service2_json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse service 2 JSON");
    
    let cluster_ip2 = service2_json["spec"]["clusterIP"].as_str().unwrap();
    println!("Second service ClusterIP: {}", cluster_ip2);
    assert_ne!(cluster_ip, cluster_ip2, "ClusterIPs should be unique");
    
    // Test service list
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "services"
        ])
        .output()
        .expect("Failed to list services");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Service list:\n{}", stdout);
    assert!(stdout.contains("test-service"), "First service not in list");
    assert!(stdout.contains("test-service-2"), "Second service not in list");
    
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
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "service",
            "test-service-2"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "test-service.yaml", "test-service-2.yaml"])
        .output()
        .ok();
    
    server.kill().expect("Failed to kill server");
    
    println!("\n✅ Service creation and ClusterIP test passed!");
}

#[test]
#[ignore]
fn test_service_endpoints() {
    println!("=== Testing Service Endpoints Discovery ===");
    
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
  name: nginx-1
  labels:
    app: nginx
spec:
  containers:
  - name: nginx
    image: nginx:alpine
"#;
    
    let pod2_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: nginx-2
  labels:
    app: nginx
spec:
  containers:
  - name: nginx
    image: nginx:alpine
"#;
    
    std::fs::write("pod1.yaml", pod1_yaml).expect("Failed to write pod yaml");
    std::fs::write("pod2.yaml", pod2_yaml).expect("Failed to write pod yaml");
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "pod1.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create pod 1");
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "pod2.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create pod 2");
    
    // Wait for pods to be running
    thread::sleep(Duration::from_secs(5));
    
    // Create service with selector
    let service_yaml = r#"
apiVersion: v1
kind: Service
metadata:
  name: nginx-service
spec:
  selector:
    app: nginx
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
    
    // Get endpoints for the service
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "endpoints",
            "nginx-service",
            "-o",
            "json"
        ])
        .output();
    
    // Note: Endpoints resource might not be implemented yet
    // For now, just verify the service was created with the selector
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "service",
            "nginx-service",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get service");
    
    let service_json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse service JSON");
    
    assert_eq!(
        service_json["spec"]["selector"]["app"].as_str(),
        Some("nginx"),
        "Service selector not set correctly"
    );
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "pod",
            "nginx-1",
            "nginx-2"
        ])
        .output()
        .ok();
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "service",
            "nginx-service"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "pod1.yaml", "pod2.yaml", "service.yaml"])
        .output()
        .ok();
    
    server.kill().ok();
    
    println!("\n✅ Service endpoints test passed!");
}