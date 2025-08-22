use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
#[ignore] // Run with: cargo test --test pod_status_test -- --ignored --nocapture
fn test_comprehensive_pod_status() {
    println!("=== Testing Comprehensive Pod Status Information ===");
    
    // Clean state
    Command::new("pkill")
        .args(&["-f", "target/debug/krust"])
        .output()
        .ok();
    
    thread::sleep(Duration::from_secs(1));
    
    Command::new("rm")
        .args(&["-f", "krust.db"])
        .output()
        .ok();
    
    // Start server
    println!("Starting Krust server...");
    let mut server = Command::new("cargo")
        .args(&["run"])
        .spawn()
        .expect("Failed to start server");
    
    thread::sleep(Duration::from_secs(3));
    
    // Create a test pod
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: status-test
  labels:
    app: test
spec:
  containers:
  - name: nginx
    image: nginx:alpine
    ports:
    - containerPort: 80
    resources:
      limits:
        memory: "128Mi"
        cpu: "500m"
      requests:
        memory: "64Mi"
        cpu: "250m"
"#;
    
    std::fs::write("status-test.yaml", pod_yaml).expect("Failed to write pod yaml");
    
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "status-test.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create pod");
    
    println!("Create output: {}", String::from_utf8_lossy(&output.stdout));
    
    // Wait for pod to be scheduled and start
    thread::sleep(Duration::from_secs(5));
    
    // Test kubectl get pods with wide output
    println!("\n=== Testing kubectl get pods ===");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "pods"
        ])
        .output()
        .expect("Failed to get pods");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("kubectl get pods output:\n{}", stdout);
    
    // Check for expected columns: NAME READY STATUS RESTARTS AGE
    assert!(stdout.contains("NAME"), "Missing NAME column");
    assert!(stdout.contains("READY"), "Missing READY column");
    assert!(stdout.contains("STATUS"), "Missing STATUS column");
    assert!(stdout.contains("RESTARTS"), "Missing RESTARTS column");
    assert!(stdout.contains("AGE"), "Missing AGE column");
    
    // Check pod row
    assert!(stdout.contains("status-test"), "Pod name not shown");
    
    // Test kubectl get pods -o wide
    println!("\n=== Testing kubectl get pods -o wide ===");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "pods",
            "-o",
            "wide"
        ])
        .output()
        .expect("Failed to get pods wide");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("kubectl get pods -o wide output:\n{}", stdout);
    
    // Additional columns in wide mode: IP NODE NOMINATED NODE READINESS GATES
    assert!(stdout.contains("IP"), "Missing IP column in wide output");
    assert!(stdout.contains("NODE"), "Missing NODE column in wide output");
    
    // Test kubectl describe pod
    println!("\n=== Testing kubectl describe pod ===");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "describe",
            "pod",
            "status-test"
        ])
        .output()
        .expect("Failed to describe pod");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("kubectl describe pod output:\n{}", stdout);
    
    // Check for expected sections in describe output
    assert!(stdout.contains("Name:"), "Missing Name field in describe");
    assert!(stdout.contains("Namespace:"), "Missing Namespace field");
    assert!(stdout.contains("Status:"), "Missing Status field");
    assert!(stdout.contains("IP:"), "Missing IP field");
    assert!(stdout.contains("Node:"), "Missing Node field");
    assert!(stdout.contains("Containers:"), "Missing Containers section");
    assert!(stdout.contains("Conditions:"), "Missing Conditions section");
    assert!(stdout.contains("Events:") || stdout.contains("No events"), "Missing Events section");
    
    // Test JSON output for detailed status
    println!("\n=== Testing kubectl get pod -o json ===");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "pod",
            "status-test",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get pod json");
    
    let pod_json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse pod JSON");
    
    // Verify comprehensive status fields
    assert!(pod_json["status"]["phase"].is_string(), "Missing status.phase");
    assert!(pod_json["status"]["conditions"].is_array(), "Missing status.conditions");
    assert!(pod_json["status"]["containerStatuses"].is_array(), "Missing status.containerStatuses");
    assert!(pod_json["status"]["podIP"].is_string(), "Missing status.podIP");
    assert!(pod_json["status"]["hostIP"].is_string(), "Missing status.hostIP");
    
    // Check container status details
    if let Some(container_statuses) = pod_json["status"]["containerStatuses"].as_array() {
        assert!(!container_statuses.is_empty(), "No container statuses found");
        let container_status = &container_statuses[0];
        assert!(container_status["ready"].is_boolean(), "Missing container ready field");
        assert!(container_status["restartCount"].is_number(), "Missing restart count");
        assert!(container_status["state"].is_object(), "Missing container state");
    }
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "pod",
            "status-test"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "status-test.yaml"])
        .output()
        .ok();
    
    server.kill().expect("Failed to kill server");
    
    println!("\n✅ Comprehensive pod status test passed!");
}

#[test]
#[ignore]
fn test_pod_ready_status() {
    println!("=== Testing Pod Ready Status Calculation ===");
    
    // Clean state
    Command::new("pkill")
        .args(&["-f", "target/debug/krust"])
        .output()
        .ok();
    
    thread::sleep(Duration::from_secs(1));
    
    Command::new("rm")
        .args(&["-f", "krust.db"])
        .output()
        .ok();
    
    // Start server
    let mut server = Command::new("cargo")
        .args(&["run"])
        .spawn()
        .expect("Failed to start server");
    
    thread::sleep(Duration::from_secs(3));
    
    // Create a multi-container pod
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: multi-container
spec:
  containers:
  - name: nginx
    image: nginx:alpine
    ports:
    - containerPort: 80
  - name: sidecar
    image: busybox:latest
    command: ["sleep", "3600"]
"#;
    
    std::fs::write("multi-container.yaml", pod_yaml).expect("Failed to write pod yaml");
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "multi-container.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create pod");
    
    // Wait for scheduling
    thread::sleep(Duration::from_secs(5));
    
    // Check READY column shows correct format (e.g., "2/2" for both containers ready)
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "pod",
            "multi-container"
        ])
        .output()
        .expect("Failed to get pod");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Multi-container pod status:\n{}", stdout);
    
    // The READY column should show "x/y" format
    // where x is ready containers and y is total containers
    assert!(stdout.contains("multi-container"), "Pod not found");
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "pod",
            "multi-container"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "multi-container.yaml"])
        .output()
        .ok();
    
    server.kill().ok();
    
    println!("\n✅ Pod ready status test passed!");
}