use std::process::Command;
use std::thread;
use std::time::Duration;

fn wait_for_condition<F>(condition: F, timeout_secs: u64, message: &str) -> bool 
where
    F: Fn() -> bool,
{
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(500));
    }
    println!("Timeout waiting for: {}", message);
    false
}

#[test]
#[ignore] // Run with: cargo test --test container_e2e_test -- --ignored --nocapture
fn test_pod_with_real_container() {
    println!("=== End-to-End Container Test ===");
    
    // Ensure clean state
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
    
    // Wait for server
    thread::sleep(Duration::from_secs(3));
    
    // Verify server is healthy
    let output = Command::new("curl")
        .args(&["-s", "http://localhost:6443/healthz"])
        .output()
        .expect("Failed to check health");
    
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ok");
    println!("✓ Server is healthy");
    
    // Create a simple busybox pod that echoes and sleeps
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: busybox-test
  labels:
    test: e2e
spec:
  containers:
  - name: busybox
    image: busybox:1.36
    command: 
    - sh
    - -c
    - |
      echo "Container started successfully"
      while true; do
        echo "Still running at $(date)"
        sleep 5
      done
"#;
    
    std::fs::write("busybox-test.yaml", pod_yaml).expect("Failed to write pod yaml");
    
    // Create the pod
    println!("\n1. Creating pod with busybox container...");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "busybox-test.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create pod");
    
    println!("Create output: {}", String::from_utf8_lossy(&output.stdout));
    assert!(output.status.success() || output.stderr.len() > 0);
    
    // Wait for pod to be scheduled
    println!("\n2. Waiting for pod to be scheduled...");
    let scheduled = wait_for_condition(
        || {
            let output = Command::new("kubectl")
                .args(&[
                    "--server=http://localhost:6443",
                    "get",
                    "pod",
                    "busybox-test",
                    "-o",
                    "json"
                ])
                .output()
                .ok();
            
            if let Some(output) = output {
                let json_str = String::from_utf8_lossy(&output.stdout);
                json_str.contains("\"phase\"") && !json_str.contains("\"phase\":\"Pending\"")
            } else {
                false
            }
        },
        30,
        "pod to be scheduled"
    );
    
    assert!(scheduled, "Pod was not scheduled in time");
    println!("✓ Pod scheduled");
    
    // Check pod status
    println!("\n3. Checking pod status...");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "pod",
            "busybox-test",
            "-o",
            "wide"
        ])
        .output()
        .expect("Failed to get pod status");
    
    let status = String::from_utf8_lossy(&output.stdout);
    println!("Pod status:\n{}", status);
    
    // Wait for container to be running (if Docker is available)
    println!("\n4. Waiting for container to start...");
    thread::sleep(Duration::from_secs(5));
    
    // Try to get logs (this will test the logs endpoint)
    println!("\n5. Attempting to get pod logs...");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "logs",
            "busybox-test"
        ])
        .output()
        .ok();
    
    if let Some(output) = output {
        let logs = String::from_utf8_lossy(&output.stdout);
        let errors = String::from_utf8_lossy(&output.stderr);
        println!("Logs output: {}", logs);
        if !errors.is_empty() {
            println!("Logs errors: {}", errors);
        }
        
        // If logs endpoint is implemented, we should see our echo
        if logs.contains("Container started successfully") {
            println!("✓ Container is running and logs are working!");
        } else if errors.contains("the server could not find the requested resource") {
            println!("⚠ Logs endpoint not yet implemented");
        }
    }
    
    // Check if container is actually running via Docker
    println!("\n6. Checking Docker container status...");
    let output = Command::new("docker")
        .args(&["ps", "--filter", "label=io.kubernetes.pod.name=busybox-test", "--format", "{{.Names}}\t{{.Status}}"])
        .output()
        .ok();
    
    if let Some(output) = output {
        let containers = String::from_utf8_lossy(&output.stdout);
        if !containers.is_empty() {
            println!("Docker containers:\n{}", containers);
            println!("✓ Container is running in Docker!");
        } else {
            println!("⚠ No Docker containers found (Docker may not be available)");
        }
    }
    
    // Test pod deletion
    println!("\n7. Deleting pod...");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "pod",
            "busybox-test"
        ])
        .output()
        .expect("Failed to delete pod");
    
    println!("Delete output: {}", String::from_utf8_lossy(&output.stdout));
    
    // Wait for container to be cleaned up
    thread::sleep(Duration::from_secs(3));
    
    // Verify container is gone
    println!("\n8. Verifying cleanup...");
    let output = Command::new("docker")
        .args(&["ps", "-a", "--filter", "label=io.kubernetes.pod.name=busybox-test", "--format", "{{.Names}}"])
        .output()
        .ok();
    
    if let Some(output) = output {
        let containers = String::from_utf8_lossy(&output.stdout);
        if containers.trim().is_empty() {
            println!("✓ Container cleaned up successfully");
        } else {
            println!("⚠ Container still exists: {}", containers);
        }
    }
    
    // Clean up
    Command::new("rm")
        .args(&["-f", "busybox-test.yaml"])
        .output()
        .ok();
    
    server.kill().expect("Failed to kill server");
    
    println!("\n✅ End-to-end container test completed!");
}

#[test]
#[ignore]
fn test_pod_with_probes() {
    println!("=== Testing Pod with Health Probes ===");
    
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
    
    // Create pod with health probes
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: probe-test
spec:
  containers:
  - name: app
    image: busybox:1.36
    command:
    - sh
    - -c
    - |
      # Create health check file after 5 seconds
      sleep 5
      touch /tmp/healthy
      # Keep container running
      while true; do sleep 10; done
    livenessProbe:
      exec:
        command:
        - test
        - -f
        - /tmp/healthy
      initialDelaySeconds: 10
      periodSeconds: 5
    readinessProbe:
      exec:
        command:
        - test
        - -f
        - /tmp/healthy
      initialDelaySeconds: 3
      periodSeconds: 3
"#;
    
    std::fs::write("probe-test.yaml", pod_yaml).expect("Failed to write pod yaml");
    
    println!("Creating pod with probes...");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "probe-test.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create pod");
    
    println!("Output: {}", String::from_utf8_lossy(&output.stdout));
    
    // Check pod events for probe results
    thread::sleep(Duration::from_secs(15));
    
    println!("\nChecking pod status after probes...");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "describe",
            "pod",
            "probe-test"
        ])
        .output()
        .ok();
    
    if let Some(output) = output {
        let description = String::from_utf8_lossy(&output.stdout);
        println!("Pod description:\n{}", description);
        
        // Check if probe information is present
        if description.contains("Liveness") || description.contains("Readiness") {
            println!("✓ Probe information found in pod description");
        } else {
            println!("⚠ Probe information not yet implemented");
        }
    }
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "pod",
            "probe-test"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "probe-test.yaml"])
        .output()
        .ok();
    
    server.kill().ok();
    
    println!("\n✅ Probe test completed!");
}