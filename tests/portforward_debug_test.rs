// Debug test for port-forward functionality
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use std::io::{Write, Read};
use tokio::time::timeout;

fn setup_krust_server() -> std::process::Child {
    // Clean up any existing server
    Command::new("pkill")
        .args(&["-f", "target/debug/krust"])
        .output()
        .ok();
    thread::sleep(Duration::from_secs(1));
    Command::new("rm").args(&["-f", "krust.db"]).output().ok();

    // Start server with debug logging
    let server = Command::new("cargo")
        .args(&["run"])
        .env("RUST_LOG", "debug")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server");

    // Wait for server to be ready
    for i in 0..30 {
        if let Ok(output) = Command::new("curl")
            .args(&["-s", "http://localhost:6443/healthz"])
            .output()
        {
            if String::from_utf8_lossy(&output.stdout) == "ok" {
                println!("✓ Server is ready after {} attempts", i + 1);
                break;
            }
        }
        thread::sleep(Duration::from_millis(500));
    }

    // Configure kubectl to use krust
    Command::new("kubectl")
        .args(&["config", "set-cluster", "krust", "--server=http://localhost:6443"])
        .output()
        .unwrap();
    Command::new("kubectl")
        .args(&["config", "set-context", "krust", "--cluster=krust"])
        .output()
        .unwrap();
    Command::new("kubectl")
        .args(&["config", "use-context", "krust"])
        .output()
        .unwrap();

    println!("✓ kubectl configured for krust");
    server
}

fn cleanup(mut server: std::process::Child) {
    Command::new("pkill")
        .args(&["-f", "kubectl port-forward"])
        .output()
        .ok();
    server.kill().ok();
    thread::sleep(Duration::from_millis(500));
}

#[tokio::test]
#[ignore] // Run with: cargo test portforward_debug_test -- --ignored --nocapture
async fn test_portforward_debug() {
    println!("\n=== Port-Forward Debug Test ===");
    let mut server = setup_krust_server();

    // Create test pod
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-webapp
  namespace: default
spec:
  containers:
  - name: http
    image: nginx:alpine
    ports:
    - containerPort: 80
"#;

    std::fs::write("/tmp/test-pod.yaml", pod_yaml).unwrap();
    
    let output = Command::new("kubectl")
        .args(&["apply", "-f", "/tmp/test-pod.yaml"])
        .output()
        .unwrap();
    
    if !output.status.success() {
        eprintln!("Failed to create pod: {}", String::from_utf8_lossy(&output.stderr));
        cleanup(server);
        panic!("Pod creation failed");
    }
    println!("✓ Pod created");

    // Wait for pod to be running
    println!("Waiting for pod to be ready...");
    for i in 0..60 {
        if let Ok(output) = Command::new("kubectl")
            .args(&["get", "pod", "test-webapp", "-o", "jsonpath={.status.phase}"])
            .output()
        {
            let status = String::from_utf8_lossy(&output.stdout);
            if status == "Running" {
                println!("✓ Pod is running after {} seconds", i);
                break;
            } else if i % 5 == 0 {
                println!("  Pod status: {}", status);
            }
        }
        if i == 59 {
            // Get pod details for debugging
            let output = Command::new("kubectl")
                .args(&["describe", "pod", "test-webapp"])
                .output()
                .unwrap();
            println!("Pod describe:\n{}", String::from_utf8_lossy(&output.stdout));
            cleanup(server);
            panic!("Pod did not become ready within 60 seconds");
        }
        thread::sleep(Duration::from_secs(1));
    }

    // Give container a moment to fully initialize
    thread::sleep(Duration::from_secs(2));

    // Start port-forward with verbose logging
    println!("\nStarting kubectl port-forward with verbose logging...");
    let mut pf = Command::new("kubectl")
        .args(&["port-forward", "pod/test-webapp", "18080:80", "-v=6"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start port-forward");

    // Give port-forward time to establish
    thread::sleep(Duration::from_secs(3));

    // Check if port-forward is still running
    match pf.try_wait() {
        Ok(Some(status)) => {
            // Process has exited, capture output
            let mut stdout = String::new();
            let mut stderr = String::new();
            if let Some(mut out) = pf.stdout {
                out.read_to_string(&mut stdout).ok();
            }
            if let Some(mut err) = pf.stderr {
                err.read_to_string(&mut stderr).ok();
            }
            println!("Port-forward exited with status: {}", status);
            println!("STDOUT:\n{}", stdout);
            println!("STDERR:\n{}", stderr);
            
            // Also check server logs
            if let Some(mut server_stdout) = server.stdout.take() {
                let mut server_out = String::new();
                server_stdout.read_to_string(&mut server_out).ok();
                println!("\nServer STDOUT:\n{}", server_out);
            }
            if let Some(mut server_stderr) = server.stderr.take() {
                let mut server_err = String::new();
                server_stderr.read_to_string(&mut server_err).ok();
                println!("\nServer STDERR:\n{}", server_err);
            }
            
            cleanup(server);
            panic!("Port-forward process exited prematurely");
        }
        Ok(None) => {
            println!("✓ Port-forward process is running");
        }
        Err(e) => {
            println!("Error checking port-forward status: {}", e);
        }
    }

    // Test the connection
    println!("\nTesting HTTP connection to localhost:18080...");
    let result = timeout(Duration::from_secs(5), async {
        reqwest::get("http://localhost:18080").await
    }).await;

    match result {
        Ok(Ok(response)) => {
            println!("✓ Got response with status: {}", response.status());
            if response.status().is_success() {
                let body = response.text().await.unwrap_or_default();
                println!("Response body preview: {}", &body[..body.len().min(200)]);
                println!("✓✓ Port-forward is working!");
            } else {
                println!("✗ Unexpected status code");
            }
        }
        Ok(Err(e)) => {
            println!("✗ Request failed: {}", e);
            
            // Port-forward is still running, don't try to capture output
        }
        Err(_) => {
            println!("✗ Request timed out");
        }
    }

    // Cleanup
    pf.kill().ok();
    Command::new("kubectl")
        .args(&["delete", "pod", "test-webapp", "--force", "--grace-period=0"])
        .output()
        .ok();
    std::fs::remove_file("/tmp/test-pod.yaml").ok();
    
    cleanup(server);
    println!("\n=== Test Complete ===\n");
}

#[tokio::test]
#[ignore]
async fn test_portforward_protocol_inspection() {
    println!("\n=== Port-Forward Protocol Inspection ===");
    
    // This test inspects what kubectl sends when connecting
    let mut server = setup_krust_server();
    
    // Create pod
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: protocol-test
  namespace: default
spec:
  containers:
  - name: http
    image: nginx:alpine
    ports:
    - containerPort: 80
"#;
    
    std::fs::write("/tmp/protocol-test.yaml", pod_yaml).unwrap();
    Command::new("kubectl")
        .args(&["apply", "-f", "/tmp/protocol-test.yaml"])
        .output()
        .unwrap();
    
    // Wait for pod
    for _ in 0..30 {
        if let Ok(output) = Command::new("kubectl")
            .args(&["get", "pod", "protocol-test", "-o", "jsonpath={.status.phase}"])
            .output()
        {
            if String::from_utf8_lossy(&output.stdout) == "Running" {
                break;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
    
    println!("\nStarting kubectl port-forward with maximum verbosity...");
    
    // Use -v=9 for maximum verbosity
    let output = Command::new("timeout")
        .args(&["5", "kubectl", "port-forward", "pod/protocol-test", "18081:80", "-v=9"])
        .output()
        .unwrap();
    
    println!("kubectl output (first 5000 chars):");
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("{}", &stderr[..stderr.len().min(5000)]);
    
    // Check server logs
    if let Some(mut server_stderr) = server.stderr.take() {
        let mut server_err = String::new();
        server_stderr.read_to_string(&mut server_err).ok();
        println!("\nServer logs (last 3000 chars):");
        let start = server_err.len().saturating_sub(3000);
        println!("{}", &server_err[start..]);
    }
    
    // Cleanup
    Command::new("kubectl")
        .args(&["delete", "pod", "protocol-test", "--force", "--grace-period=0"])
        .output()
        .ok();
    cleanup(server);
    
    println!("\n=== Protocol Inspection Complete ===\n");
}