// End-to-end tests for port-forward functionality
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use std::io::Write;
use tokio::time::timeout;

fn setup_environment() -> std::process::Child {
    // Clean up
    Command::new("pkill")
        .args(&["-f", "target/debug/krust"])
        .output()
        .ok();
    thread::sleep(Duration::from_secs(1));
    Command::new("rm").args(&["-f", "krust.db"]).output().ok();

    // Start server
    let server = Command::new("cargo")
        .args(&["run"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start server");

    // Wait for server to be ready
    for _ in 0..30 {
        if let Ok(output) = Command::new("curl")
            .args(&["-s", "http://localhost:6443/healthz"])
            .output()
        {
            if String::from_utf8_lossy(&output.stdout) == "ok" {
                println!("✓ Server is ready");
                break;
            }
        }
        thread::sleep(Duration::from_millis(500));
    }

    // Configure kubectl
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

    server
}

fn cleanup(mut server: std::process::Child) {
    Command::new("pkill")
        .args(&["-f", "kubectl port-forward"])
        .output()
        .ok();
    server.kill().ok();
}

#[tokio::test]
#[ignore]
async fn test_e2e_basic_portforward() {
    println!("\n=== E2E Basic Port-Forward Test ===");
    let server = setup_environment();

    // Create test pod
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: e2e-test-pod
  namespace: default
spec:
  containers:
  - name: web
    image: hashicorp/http-echo:latest
    args:
    - "-text=E2E Test Success"
    - "-listen=:8080"
    ports:
    - containerPort: 8080
"#;

    std::fs::write("e2e-test-pod.yaml", pod_yaml).unwrap();
    
    let output = Command::new("kubectl")
        .args(&["apply", "-f", "e2e-test-pod.yaml"])
        .output()
        .unwrap();
    assert!(output.status.success(), "Failed to create pod");
    println!("✓ Pod created");

    // Wait for pod to be running
    for i in 0..30 {
        if let Ok(output) = Command::new("kubectl")
            .args(&["get", "pod", "e2e-test-pod", "-o", "jsonpath={.status.phase}"])
            .output()
        {
            if String::from_utf8_lossy(&output.stdout) == "Running" {
                println!("✓ Pod is running");
                break;
            }
        }
        if i == 29 {
            panic!("Pod did not become ready");
        }
        thread::sleep(Duration::from_secs(1));
    }

    thread::sleep(Duration::from_secs(2));

    // Start port-forward
    let mut pf = Command::new("kubectl")
        .args(&["port-forward", "pod/e2e-test-pod", "18080:8080"])
        .spawn()
        .expect("Failed to start port-forward");

    thread::sleep(Duration::from_secs(2));

    // Test the connection
    let result = timeout(Duration::from_secs(5), async {
        reqwest::get("http://localhost:18080").await
    }).await;

    assert!(result.is_ok(), "Request timed out");
    let response = result.unwrap().unwrap();
    assert_eq!(response.status(), 200);
    
    let body = response.text().await.unwrap();
    assert!(body.contains("E2E Test Success"), "Unexpected response: {}", body);
    println!("✓ Port-forward working!");

    // Cleanup
    pf.kill().ok();
    Command::new("kubectl")
        .args(&["delete", "pod", "e2e-test-pod", "--force", "--grace-period=0"])
        .output()
        .ok();
    std::fs::remove_file("e2e-test-pod.yaml").ok();
    
    cleanup(server);
    println!("✓ Test complete\n");
}

#[tokio::test]
#[ignore]
async fn test_e2e_service_portforward() {
    println!("\n=== E2E Service Port-Forward Test ===");
    let server = setup_environment();

    // Create pod and service
    let yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: e2e-service-pod
  namespace: default
  labels:
    app: e2e-test
spec:
  containers:
  - name: web
    image: hashicorp/http-echo:latest
    args:
    - "-text=Service Forward Success"
    - "-listen=:8080"
    ports:
    - containerPort: 8080
---
apiVersion: v1
kind: Service
metadata:
  name: e2e-service
  namespace: default
spec:
  selector:
    app: e2e-test
  ports:
  - port: 80
    targetPort: 8080
"#;

    std::fs::write("e2e-service.yaml", yaml).unwrap();
    Command::new("kubectl")
        .args(&["apply", "-f", "e2e-service.yaml"])
        .output()
        .unwrap();
    println!("✓ Pod and service created");

    // Wait for pod
    for _ in 0..30 {
        if let Ok(output) = Command::new("kubectl")
            .args(&["get", "pod", "e2e-service-pod", "-o", "jsonpath={.status.phase}"])
            .output()
        {
            if String::from_utf8_lossy(&output.stdout) == "Running" {
                println!("✓ Pod is running");
                break;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }

    thread::sleep(Duration::from_secs(2));

    // Port-forward through service
    let mut pf = Command::new("kubectl")
        .args(&["port-forward", "service/e2e-service", "18081:80"])
        .spawn()
        .expect("Failed to start port-forward");

    thread::sleep(Duration::from_secs(2));

    // Test connection
    let response = reqwest::get("http://localhost:18081")
        .await
        .expect("Failed to connect");
    
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("Service Forward Success"), "Unexpected response: {}", body);
    println!("✓ Service port-forward working!");

    // Cleanup
    pf.kill().ok();
    Command::new("kubectl")
        .args(&["delete", "-f", "e2e-service.yaml", "--force", "--grace-period=0"])
        .output()
        .ok();
    std::fs::remove_file("e2e-service.yaml").ok();
    
    cleanup(server);
    println!("✓ Test complete\n");
}

#[tokio::test]
#[ignore]
async fn test_e2e_multiple_ports() {
    println!("\n=== E2E Multiple Ports Test ===");
    let server = setup_environment();

    // Create multi-port pod
    let yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: e2e-multi-port
  namespace: default
spec:
  containers:
  - name: multi
    image: mendhak/http-https-echo:latest
    ports:
    - containerPort: 8080
      name: http
    - containerPort: 8443
      name: https
"#;

    std::fs::write("e2e-multi.yaml", yaml).unwrap();
    Command::new("kubectl")
        .args(&["apply", "-f", "e2e-multi.yaml"])
        .output()
        .unwrap();
    println!("✓ Multi-port pod created");

    // Wait for pod
    for _ in 0..30 {
        if let Ok(output) = Command::new("kubectl")
            .args(&["get", "pod", "e2e-multi-port", "-o", "jsonpath={.status.phase}"])
            .output()
        {
            if String::from_utf8_lossy(&output.stdout) == "Running" {
                println!("✓ Pod is running");
                break;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }

    thread::sleep(Duration::from_secs(3));

    // Forward multiple ports
    let mut pf = Command::new("kubectl")
        .args(&["port-forward", "pod/e2e-multi-port", "18082:8080", "18083:8443"])
        .spawn()
        .expect("Failed to start port-forward");

    thread::sleep(Duration::from_secs(3));

    // Test HTTP port
    let response = reqwest::get("http://localhost:18082")
        .await
        .expect("Failed to connect to HTTP port");
    assert_eq!(response.status(), 200);
    println!("✓ HTTP port (18082 -> 8080) working");

    // Test HTTPS port (allowing invalid certs for test)
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    
    let response = client.get("https://localhost:18083")
        .send()
        .await
        .expect("Failed to connect to HTTPS port");
    assert_eq!(response.status(), 200);
    println!("✓ HTTPS port (18083 -> 8443) working");

    // Cleanup
    pf.kill().ok();
    Command::new("kubectl")
        .args(&["delete", "pod", "e2e-multi-port", "--force", "--grace-period=0"])
        .output()
        .ok();
    std::fs::remove_file("e2e-multi.yaml").ok();
    
    cleanup(server);
    println!("✓ Test complete\n");
}

#[tokio::test]
#[ignore]
async fn test_e2e_tcp_streaming() {
    println!("\n=== E2E TCP Streaming Test ===");
    let server = setup_environment();

    // Create echo server pod
    let yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: e2e-echo
  namespace: default
spec:
  containers:
  - name: echo
    image: nicolaka/netshoot:latest
    command: ["nc", "-l", "-k", "-p", "8080", "-e", "/bin/cat"]
    ports:
    - containerPort: 8080
"#;

    std::fs::write("e2e-echo.yaml", yaml).unwrap();
    Command::new("kubectl")
        .args(&["apply", "-f", "e2e-echo.yaml"])
        .output()
        .unwrap();
    println!("✓ Echo server pod created");

    // Wait for pod
    for _ in 0..30 {
        if let Ok(output) = Command::new("kubectl")
            .args(&["get", "pod", "e2e-echo", "-o", "jsonpath={.status.phase}"])
            .output()
        {
            if String::from_utf8_lossy(&output.stdout) == "Running" {
                println!("✓ Pod is running");
                break;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }

    thread::sleep(Duration::from_secs(3));

    // Start port-forward
    let mut pf = Command::new("kubectl")
        .args(&["port-forward", "pod/e2e-echo", "18084:8080"])
        .spawn()
        .expect("Failed to start port-forward");

    thread::sleep(Duration::from_secs(2));

    // Test TCP streaming
    use tokio::net::TcpStream;
    use tokio::io::{AsyncWriteExt, AsyncReadExt};

    let mut stream = TcpStream::connect("127.0.0.1:18084")
        .await
        .expect("Failed to connect");

    // Send and receive multiple messages
    for i in 1..=5 {
        let msg = format!("Message {}\n", i);
        stream.write_all(msg.as_bytes()).await.unwrap();
        
        let mut buf = vec![0u8; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);
        
        assert_eq!(response, msg, "Echo mismatch");
        println!("✓ Echoed: {}", response.trim());
    }

    println!("✓ TCP streaming working!");

    // Cleanup
    pf.kill().ok();
    Command::new("kubectl")
        .args(&["delete", "pod", "e2e-echo", "--force", "--grace-period=0"])
        .output()
        .ok();
    std::fs::remove_file("e2e-echo.yaml").ok();
    
    cleanup(server);
    println!("✓ Test complete\n");
}

#[tokio::test]
#[ignore]
async fn test_e2e_concurrent_connections() {
    println!("\n=== E2E Concurrent Connections Test ===");
    let server = setup_environment();

    // Create web server pod
    let yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: e2e-concurrent
  namespace: default
spec:
  containers:
  - name: web
    image: hashicorp/http-echo:latest
    args:
    - "-text=Concurrent Test"
    - "-listen=:8080"
    ports:
    - containerPort: 8080
"#;

    std::fs::write("e2e-concurrent.yaml", yaml).unwrap();
    Command::new("kubectl")
        .args(&["apply", "-f", "e2e-concurrent.yaml"])
        .output()
        .unwrap();
    println!("✓ Pod created");

    // Wait for pod
    for _ in 0..30 {
        if let Ok(output) = Command::new("kubectl")
            .args(&["get", "pod", "e2e-concurrent", "-o", "jsonpath={.status.phase}"])
            .output()
        {
            if String::from_utf8_lossy(&output.stdout) == "Running" {
                println!("✓ Pod is running");
                break;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }

    thread::sleep(Duration::from_secs(2));

    // Start port-forward
    let mut pf = Command::new("kubectl")
        .args(&["port-forward", "pod/e2e-concurrent", "18085:8080"])
        .spawn()
        .expect("Failed to start port-forward");

    thread::sleep(Duration::from_secs(2));

    // Test concurrent connections
    let mut handles = vec![];
    
    for i in 0..10 {
        let handle = tokio::spawn(async move {
            let response = reqwest::get("http://localhost:18085")
                .await
                .expect(&format!("Request {} failed", i));
            
            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            assert!(body.contains("Concurrent Test"));
            i
        });
        handles.push(handle);
    }

    // Wait for all requests
    for handle in handles {
        let i = handle.await.unwrap();
        println!("✓ Request {} completed", i);
    }

    println!("✓ All concurrent connections successful!");

    // Cleanup
    pf.kill().ok();
    Command::new("kubectl")
        .args(&["delete", "pod", "e2e-concurrent", "--force", "--grace-period=0"])
        .output()
        .ok();
    std::fs::remove_file("e2e-concurrent.yaml").ok();
    
    cleanup(server);
    println!("✓ Test complete\n");
}