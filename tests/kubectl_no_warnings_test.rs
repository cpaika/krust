use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
#[ignore] // Run with: cargo test --test kubectl_no_warnings_test -- --ignored --nocapture
fn test_kubectl_no_warnings() {
    println!("Testing that kubectl commands produce no warnings...");
    
    // Ensure server is not running
    Command::new("pkill")
        .args(&["-f", "target/debug/krust"])
        .output()
        .ok();
    
    thread::sleep(Duration::from_secs(1));
    
    // Clean database
    Command::new("rm")
        .args(&["-f", "krust.db"])
        .output()
        .ok();
    
    // Start Krust server in background
    println!("Starting Krust server...");
    let mut server = Command::new("cargo")
        .args(&["run"])
        .spawn()
        .expect("Failed to start server");
    
    // Wait for server to start
    thread::sleep(Duration::from_secs(3));
    
    // Test server is running
    let output = Command::new("curl")
        .args(&["-s", "http://localhost:6443/healthz"])
        .output()
        .expect("Failed to check health");
    
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ok");
    println!("✓ Server is healthy");
    
    // Test kubectl get nodes - should have NO warnings
    println!("\nTesting 'kubectl get nodes' for warnings...");
    let output = Command::new("kubectl")
        .args(&["--server=http://localhost:6443", "get", "nodes"])
        .output()
        .expect("Failed to get nodes");
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("STDERR: {}", stderr);
    
    // Check for the specific warning
    assert!(!stderr.contains("couldn't get resource list for apps/v1"), 
        "Found apps/v1 warning in stderr: {}", stderr);
    assert!(!stderr.contains("Unhandled Error"), 
        "Found unhandled error warning in stderr: {}", stderr);
    
    println!("✓ No apps/v1 warnings");
    
    // Test kubectl get pods - should have NO warnings
    println!("\nTesting 'kubectl get pods' for warnings...");
    let output = Command::new("kubectl")
        .args(&["--server=http://localhost:6443", "get", "pods"])
        .output()
        .expect("Failed to get pods");
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("STDERR: {}", stderr);
    
    assert!(!stderr.contains("couldn't get resource list for apps/v1"), 
        "Found apps/v1 warning in stderr: {}", stderr);
    assert!(!stderr.contains("Unhandled Error"), 
        "Found unhandled error warning in stderr: {}", stderr);
    
    println!("✓ No apps/v1 warnings");
    
    // Test kubectl get deployments - should work (even if empty)
    println!("\nTesting 'kubectl get deployments' for warnings...");
    let output = Command::new("kubectl")
        .args(&["--server=http://localhost:6443", "get", "deployments"])
        .output()
        .expect("Failed to get deployments");
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("STDOUT: {}", stdout);
    println!("STDERR: {}", stderr);
    
    assert!(!stderr.contains("couldn't get resource list for apps/v1"), 
        "Found apps/v1 warning in stderr: {}", stderr);
    assert!(!stderr.contains("the server could not find the requested resource"),
        "Found 'resource not found' error in stderr: {}", stderr);
    
    println!("✓ No apps/v1 warnings for deployments");
    
    // Test API discovery endpoint directly
    println!("\nTesting API discovery endpoint...");
    let output = Command::new("curl")
        .args(&["-s", "http://localhost:6443/apis/apps/v1"])
        .output()
        .expect("Failed to get apps/v1 endpoint");
    
    let status = output.status.code().unwrap_or(-1);
    assert_eq!(status, 0, "curl failed");
    
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(body.contains("\"kind\":\"APIResourceList\""), "apps/v1 endpoint not returning APIResourceList");
    assert!(body.contains("deployments"), "apps/v1 endpoint not listing deployments");
    
    println!("✓ apps/v1 endpoint works correctly");
    
    // Create a pod and verify no warnings
    println!("\nCreating a test pod...");
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-no-warning
spec:
  containers:
  - name: test
    image: busybox
    command: ["sleep", "3600"]
"#;
    
    std::fs::write("test-pod-no-warning.yaml", pod_yaml).expect("Failed to write pod yaml");
    
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "test-pod-no-warning.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create pod");
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("Create pod STDERR: {}", stderr);
    
    assert!(!stderr.contains("couldn't get resource list for apps/v1"), 
        "Found apps/v1 warning when creating pod: {}", stderr);
    
    println!("✓ Pod created without apps/v1 warnings");
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "pod",
            "test-no-warning"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "test-pod-no-warning.yaml"])
        .output()
        .ok();
    
    // Stop server
    server.kill().expect("Failed to kill server");
    
    println!("\n✅ All tests passed - NO warnings about apps/v1!");
}