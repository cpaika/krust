// Mock kubectl client test to simulate kubectl port-forward behavior
use std::process::Command;
use std::thread;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

// Mock kubectl client that implements SPDY protocol
struct MockKubectl {
    api_server: String,
}

impl MockKubectl {
    fn new(api_server: &str) -> Self {
        Self {
            api_server: api_server.to_string(),
        }
    }

    async fn port_forward(
        &self,
        namespace: &str,
        pod: &str,
        ports: &str,
    ) -> Result<SpdyConnection, String> {
        let url = format!(
            "{}/api/v1/namespaces/{}/pods/{}/portforward?ports={}",
            self.api_server, namespace, pod, ports
        );

        // Connect to API server
        let mut stream = TcpStream::connect("127.0.0.1:6443")
            .await
            .map_err(|e| format!("Connection failed: {}", e))?;

        // Send HTTP upgrade request
        let request = format!(
            "GET /api/v1/namespaces/{}/pods/{}/portforward?ports={} HTTP/1.1\r\n\
             Host: localhost:6443\r\n\
             Upgrade: SPDY/3.1\r\n\
             Connection: Upgrade\r\n\
             \r\n",
            namespace, pod, ports
        );

        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        // Read response
        let mut buf = vec![0u8; 1024];
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        let response = String::from_utf8_lossy(&buf[..n]);
        
        // Check for successful upgrade
        if !response.contains("101 Switching Protocols") {
            return Err(format!("Upgrade failed: {}", response));
        }

        if !response.contains("Upgrade: SPDY/3.1") {
            return Err("Server did not confirm SPDY/3.1 upgrade".to_string());
        }

        Ok(SpdyConnection { stream })
    }
}

struct SpdyConnection {
    stream: TcpStream,
}

impl SpdyConnection {
    async fn send_data(&mut self, stream_id: u32, data: &[u8]) -> Result<(), String> {
        let frame = create_spdy_frame(stream_id, data);
        self.stream
            .write_all(&frame)
            .await
            .map_err(|e| format!("Failed to send frame: {}", e))
    }

    async fn receive_frame(&mut self) -> Result<(u32, Vec<u8>), String> {
        let mut header = vec![0u8; 8];
        self.stream
            .read_exact(&mut header)
            .await
            .map_err(|e| format!("Failed to read header: {}", e))?;

        let stream_id = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let length = ((header[5] as usize) << 16) | ((header[6] as usize) << 8) | (header[7] as usize);

        let mut data = vec![0u8; length];
        self.stream
            .read_exact(&mut data)
            .await
            .map_err(|e| format!("Failed to read data: {}", e))?;

        Ok((stream_id, data))
    }

    async fn close(mut self) -> Result<(), String> {
        self.stream
            .shutdown()
            .await
            .map_err(|e| format!("Failed to close: {}", e))
    }
}

fn create_spdy_frame(stream_id: u32, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(&stream_id.to_be_bytes());
    frame.push(0); // flags
    frame.push(((data.len() >> 16) & 0xFF) as u8);
    frame.push(((data.len() >> 8) & 0xFF) as u8);
    frame.push((data.len() & 0xFF) as u8);
    frame.extend_from_slice(data);
    frame
}

fn setup_test() -> std::process::Child {
    // Clean environment
    Command::new("pkill")
        .args(&["-f", "target/debug/krust"])
        .output()
        .ok();
    thread::sleep(Duration::from_secs(1));
    Command::new("rm").args(&["-f", "krust.db"]).output().ok();

    // Start server
    let server = Command::new("cargo")
        .args(&["run"])
        .spawn()
        .expect("Failed to start server");

    // Wait for server
    for _ in 0..30 {
        if let Ok(output) = Command::new("curl")
            .args(&["-s", "http://localhost:6443/healthz"])
            .output()
        {
            if String::from_utf8_lossy(&output.stdout) == "ok" {
                break;
            }
        }
        thread::sleep(Duration::from_millis(500));
    }

    server
}

fn create_pod(name: &str, port: u16) {
    let yaml = format!(
        r#"apiVersion: v1
kind: Pod
metadata:
  name: {}
  namespace: default
spec:
  containers:
  - name: echo
    image: hashicorp/http-echo:latest
    args:
    - "-text=Hello from {}"
    - "-listen=:{}"
    ports:
    - containerPort: {}"#,
        name, name, port, port
    );

    std::fs::write(format!("{}.yaml", name), yaml).unwrap();
    Command::new("kubectl")
        .args(&["apply", "-f", &format!("{}.yaml", name)])
        .output()
        .unwrap();

    // Wait for pod to be running
    for _ in 0..30 {
        if let Ok(output) = Command::new("kubectl")
            .args(&["get", "pod", name, "-o", "jsonpath={.status.phase}"])
            .output()
        {
            if String::from_utf8_lossy(&output.stdout) == "Running" {
                break;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
    thread::sleep(Duration::from_secs(2));
}

#[tokio::test]
#[ignore]
async fn test_mock_kubectl_basic() {
    let mut server = setup_test();
    create_pod("mock-test-pod", 8080);

    // Create mock kubectl client
    let kubectl = MockKubectl::new("http://localhost:6443");

    // Test port-forward
    let result = kubectl.port_forward("default", "mock-test-pod", "8080").await;
    assert!(result.is_ok(), "Port-forward failed");

    let mut conn = result.unwrap();

    // Send HTTP request through SPDY
    let http_request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    conn.send_data(0, http_request).await.unwrap();

    // Receive response
    let (stream_id, data) = conn.receive_frame().await.unwrap();
    assert_eq!(stream_id, 0, "Response should come on same stream");
    
    let response = String::from_utf8_lossy(&data);
    assert!(response.contains("Hello from mock-test-pod"), 
            "Unexpected response: {}", response);

    conn.close().await.unwrap();

    // Cleanup
    Command::new("kubectl")
        .args(&["delete", "pod", "mock-test-pod", "--force", "--grace-period=0"])
        .output()
        .ok();
    std::fs::remove_file("mock-test-pod.yaml").ok();
    server.kill().ok();
}

#[tokio::test]
#[ignore]
async fn test_mock_kubectl_multiple_ports() {
    let mut server = setup_test();

    // Create multi-port pod
    let yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: multi-port-test
  namespace: default
spec:
  containers:
  - name: multi
    image: nicolaka/netshoot:latest
    command: ["/bin/sh"]
    args:
    - -c
    - |
      while true; do echo "Port 8080" | nc -l -p 8080; done &
      while true; do echo "Port 8081" | nc -l -p 8081; done &
      sleep infinity
    ports:
    - containerPort: 8080
    - containerPort: 8081"#;

    std::fs::write("multi-port-test.yaml", yaml).unwrap();
    Command::new("kubectl")
        .args(&["apply", "-f", "multi-port-test.yaml"])
        .output()
        .unwrap();

    // Wait for pod
    for _ in 0..30 {
        if let Ok(output) = Command::new("kubectl")
            .args(&["get", "pod", "multi-port-test", "-o", "jsonpath={.status.phase}"])
            .output()
        {
            if String::from_utf8_lossy(&output.stdout) == "Running" {
                break;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
    thread::sleep(Duration::from_secs(3));

    let kubectl = MockKubectl::new("http://localhost:6443");
    let result = kubectl.port_forward("default", "multi-port-test", "8080,8081").await;
    assert!(result.is_ok());

    let mut conn = result.unwrap();

    // Test port 1 (stream 0)
    conn.send_data(0, b"test1\n").await.unwrap();
    let (stream_id, data) = conn.receive_frame().await.unwrap();
    assert_eq!(stream_id, 0);
    assert!(String::from_utf8_lossy(&data).contains("Port 8080"));

    // Test port 2 (stream 2)
    conn.send_data(2, b"test2\n").await.unwrap();
    let (stream_id, data) = conn.receive_frame().await.unwrap();
    assert_eq!(stream_id, 2);
    assert!(String::from_utf8_lossy(&data).contains("Port 8081"));

    conn.close().await.unwrap();

    // Cleanup
    Command::new("kubectl")
        .args(&["delete", "pod", "multi-port-test", "--force", "--grace-period=0"])
        .output()
        .ok();
    std::fs::remove_file("multi-port-test.yaml").ok();
    server.kill().ok();
}

#[tokio::test]
#[ignore]
async fn test_mock_kubectl_error_cases() {
    let mut server = setup_test();

    let kubectl = MockKubectl::new("http://localhost:6443");

    // Test non-existent pod
    let result = kubectl.port_forward("default", "non-existent", "8080").await;
    assert!(result.is_err());

    // Test invalid namespace
    let result = kubectl.port_forward("invalid-ns", "some-pod", "8080").await;
    assert!(result.is_err());

    // Test no ports
    let result = kubectl.port_forward("default", "some-pod", "").await;
    assert!(result.is_err());

    server.kill().ok();
}

#[tokio::test]
#[ignore]
async fn test_bidirectional_streaming() {
    let mut server = setup_test();

    // Create echo server pod
    let yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: echo-test
  namespace: default
spec:
  containers:
  - name: echo
    image: nicolaka/netshoot:latest
    command: ["nc", "-l", "-k", "-p", "8080", "-e", "/bin/cat"]
    ports:
    - containerPort: 8080"#;

    std::fs::write("echo-test.yaml", yaml).unwrap();
    Command::new("kubectl")
        .args(&["apply", "-f", "echo-test.yaml"])
        .output()
        .unwrap();

    // Wait for pod
    for _ in 0..30 {
        if let Ok(output) = Command::new("kubectl")
            .args(&["get", "pod", "echo-test", "-o", "jsonpath={.status.phase}"])
            .output()
        {
            if String::from_utf8_lossy(&output.stdout) == "Running" {
                break;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
    thread::sleep(Duration::from_secs(3));

    let kubectl = MockKubectl::new("http://localhost:6443");
    let mut conn = kubectl.port_forward("default", "echo-test", "8080")
        .await
        .unwrap();

    // Send multiple messages
    for i in 1..=5 {
        let msg = format!("Message {}\n", i);
        conn.send_data(0, msg.as_bytes()).await.unwrap();
        
        let (stream_id, data) = conn.receive_frame().await.unwrap();
        assert_eq!(stream_id, 0);
        assert_eq!(String::from_utf8_lossy(&data), msg);
    }

    conn.close().await.unwrap();

    // Cleanup
    Command::new("kubectl")
        .args(&["delete", "pod", "echo-test", "--force", "--grace-period=0"])
        .output()
        .ok();
    std::fs::remove_file("echo-test.yaml").ok();
    server.kill().ok();
}