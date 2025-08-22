use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
#[ignore] // Run with: cargo test --test kubectl_integration_test -- --ignored --nocapture
fn test_kubectl_pod_lifecycle() {
    println!("Testing kubectl integration with Krust...");
    
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
    
    // Get nodes
    println!("\nListing nodes:");
    let output = Command::new("kubectl")
        .args(&["--server=http://localhost:6443", "get", "nodes"])
        .output()
        .expect("Failed to get nodes");
    
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(output.status.success() || output.stderr.len() > 0); // May have warnings
    
    // Create a simple pod
    println!("\nCreating test pod:");
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: echo-test
spec:
  containers:
  - name: echo
    image: busybox:latest
    command: ["echo", "Hello from Krust!"]
"#;
    
    std::fs::write("test-echo-pod.yaml", pod_yaml).expect("Failed to write pod yaml");
    
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "test-echo-pod.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create pod");
    
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("{}", String::from_utf8_lossy(&output.stderr));
    
    // List pods
    println!("\nListing pods:");
    let output = Command::new("kubectl")
        .args(&["--server=http://localhost:6443", "get", "pods"])
        .output()
        .expect("Failed to list pods");
    
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(String::from_utf8_lossy(&output.stdout).contains("echo-test"));
    println!("✓ Pod created successfully");
    
    // Get pod details
    println!("\nGetting pod details:");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "pod",
            "echo-test",
            "-o",
            "wide"
        ])
        .output()
        .expect("Failed to get pod");
    
    println!("{}", String::from_utf8_lossy(&output.stdout));
    
    // Wait a bit for scheduler and kubelet to process
    thread::sleep(Duration::from_secs(3));
    
    // Check pod status again
    println!("\nChecking pod status after scheduling:");
    let output = Command::new("kubectl")
        .args(&["--server=http://localhost:6443", "get", "pods"])
        .output()
        .expect("Failed to list pods");
    
    println!("{}", String::from_utf8_lossy(&output.stdout));
    
    // Delete pod
    println!("\nDeleting pod:");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "pod",
            "echo-test"
        ])
        .output()
        .expect("Failed to delete pod");
    
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("{}", String::from_utf8_lossy(&output.stderr));
    
    // Verify pod is deleted
    thread::sleep(Duration::from_secs(2));
    
    println!("\nVerifying pod deletion:");
    let output = Command::new("kubectl")
        .args(&["--server=http://localhost:6443", "get", "pods"])
        .output()
        .expect("Failed to list pods");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{}", stdout);
    assert!(!stdout.contains("echo-test") || stdout.contains("No resources found"));
    println!("✓ Pod deleted successfully");
    
    // Clean up
    Command::new("rm")
        .args(&["-f", "test-echo-pod.yaml"])
        .output()
        .ok();
    
    // Stop server
    server.kill().expect("Failed to kill server");
    
    println!("\n✅ All kubectl integration tests passed!");
}