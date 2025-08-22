use std::process::Command;
use std::thread;
use std::time::Duration;
use std::io::Write;

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
#[ignore] // Run with: cargo test --test portforward_test -- --ignored --nocapture
fn test_port_forward_basic() {
    println!("=== Port Forward Basic Test ===");
    
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
    
    // Create a test pod with an HTTP server
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-http-pod
  namespace: default
spec:
  containers:
  - name: http-echo
    image: hashicorp/http-echo:latest
    args:
    - "-text=Hello from port-forward test"
    - "-listen=:8080"
    ports:
    - containerPort: 8080
"#;

    std::fs::write("test-http-pod.yaml", pod_yaml).expect("Failed to write pod yaml");
    
    // Apply the pod
    let output = Command::new("kubectl")
        .args(&["apply", "-f", "test-http-pod.yaml"])
        .output()
        .expect("Failed to create pod");
    
    println!("Pod creation output: {}", String::from_utf8_lossy(&output.stdout));
    
    // Wait for pod to be running
    let pod_running = wait_for_condition(
        || {
            let output = Command::new("kubectl")
                .args(&["get", "pod", "test-http-pod", "-o", "jsonpath={.status.phase}"])
                .output()
                .ok();
            
            if let Some(output) = output {
                let phase = String::from_utf8_lossy(&output.stdout);
                phase == "Running"
            } else {
                false
            }
        },
        30,
        "pod to be running"
    );
    
    assert!(pod_running, "Pod did not reach Running state");
    println!("✓ Pod is running");
    
    // Give container time to fully start
    thread::sleep(Duration::from_secs(2));
    
    // Start port-forward in background
    println!("Starting port-forward...");
    let mut port_forward = Command::new("kubectl")
        .args(&["port-forward", "pod/test-http-pod", "9090:8080"])
        .spawn()
        .expect("Failed to start port-forward");
    
    // Give port-forward time to establish
    thread::sleep(Duration::from_secs(3));
    
    // Test the forwarded connection
    println!("Testing forwarded connection...");
    let output = Command::new("curl")
        .args(&["-s", "http://localhost:9090"])
        .output()
        .expect("Failed to curl");
    
    let response = String::from_utf8_lossy(&output.stdout);
    println!("Response: {}", response);
    
    assert!(response.contains("Hello from port-forward test"), 
            "Expected response not found. Got: {}", response);
    println!("✓ Port forward working!");
    
    // Clean up
    port_forward.kill().ok();
    Command::new("kubectl")
        .args(&["delete", "pod", "test-http-pod", "--force", "--grace-period=0"])
        .output()
        .ok();
    
    std::fs::remove_file("test-http-pod.yaml").ok();
    
    server.kill().ok();
    println!("✓ Test completed successfully");
}

#[test]
#[ignore]
fn test_port_forward_tcp_streaming() {
    println!("=== Port Forward TCP Streaming Test ===");
    
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
    
    // Create a pod with netcat for TCP testing
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-tcp-pod
  namespace: default
spec:
  containers:
  - name: netcat
    image: nicolaka/netshoot:latest
    command: ["/bin/sh"]
    args: ["-c", "nc -l -k -p 8080 -e /bin/cat"]
    ports:
    - containerPort: 8080
"#;

    std::fs::write("test-tcp-pod.yaml", pod_yaml).expect("Failed to write pod yaml");
    
    // Apply the pod
    Command::new("kubectl")
        .args(&["apply", "-f", "test-tcp-pod.yaml"])
        .output()
        .expect("Failed to create pod");
    
    // Wait for pod to be running
    let pod_running = wait_for_condition(
        || {
            let output = Command::new("kubectl")
                .args(&["get", "pod", "test-tcp-pod", "-o", "jsonpath={.status.phase}"])
                .output()
                .ok();
            
            if let Some(output) = output {
                let phase = String::from_utf8_lossy(&output.stdout);
                phase == "Running"
            } else {
                false
            }
        },
        30,
        "pod to be running"
    );
    
    assert!(pod_running, "Pod did not reach Running state");
    println!("✓ Pod is running");
    
    thread::sleep(Duration::from_secs(3));
    
    // Start port-forward
    println!("Starting port-forward...");
    let mut port_forward = Command::new("kubectl")
        .args(&["port-forward", "pod/test-tcp-pod", "9092:8080"])
        .spawn()
        .expect("Failed to start port-forward");
    
    thread::sleep(Duration::from_secs(3));
    
    // Test TCP streaming with echo
    println!("Testing TCP connection...");
    let output = Command::new("sh")
        .args(&["-c", "echo 'Hello TCP' | nc localhost 9092"])
        .output()
        .expect("Failed to test TCP");
    
    let response = String::from_utf8_lossy(&output.stdout);
    println!("TCP Response: {}", response);
    
    assert!(response.contains("Hello TCP"), 
            "Expected echo response not found. Got: {}", response);
    println!("✓ TCP streaming working!");
    
    // Clean up
    port_forward.kill().ok();
    Command::new("kubectl")
        .args(&["delete", "pod", "test-tcp-pod", "--force", "--grace-period=0"])
        .output()
        .ok();
    
    std::fs::remove_file("test-tcp-pod.yaml").ok();
    
    server.kill().ok();
    println!("✓ Test completed successfully");
}