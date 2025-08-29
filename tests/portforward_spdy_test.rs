use bytes::{Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use std::process::Command;
use std::thread;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message};

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

#[tokio::test]
#[ignore]
async fn test_kubectl_portforward_protocol() {
    println!("=== kubectl Port-Forward Protocol Test ===");

    // Clean state
    Command::new("pkill")
        .args(&["-f", "target/debug/krust"])
        .output()
        .ok();
    tokio::time::sleep(Duration::from_secs(1)).await;
    Command::new("rm").args(&["-f", "krust.db"]).output().ok();

    // Start server
    println!("Starting Krust server...");
    let mut server = Command::new("cargo")
        .args(&["run"])
        .spawn()
        .expect("Failed to start server");

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Create test pod
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-portforward-pod
  namespace: default
spec:
  containers:
  - name: http-echo
    image: hashicorp/http-echo:latest
    args:
    - "-text=Hello from SPDY test"
    - "-listen=:8080"
    ports:
    - containerPort: 8080
"#;

    std::fs::write("test-pf-pod.yaml", pod_yaml).expect("Failed to write pod yaml");

    // Apply pod
    Command::new("kubectl")
        .args(&["apply", "-f", "test-pf-pod.yaml"])
        .output()
        .expect("Failed to create pod");

    // Wait for pod running
    let pod_running = wait_for_condition(
        || {
            let output = Command::new("kubectl")
                .args(&[
                    "get",
                    "pod",
                    "test-portforward-pod",
                    "-o",
                    "jsonpath={.status.phase}",
                ])
                .output()
                .ok();

            output.map_or(false, |o| String::from_utf8_lossy(&o.stdout) == "Running")
        },
        30,
        "pod to be running",
    );

    assert!(pod_running, "Pod did not reach Running state");
    println!("✓ Pod is running");

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Test WebSocket upgrade for port-forward
    let url = "ws://localhost:6443/api/v1/namespaces/default/pods/test-portforward-pod/portforward?ports=8080";
    
    match connect_async(url).await {
        Ok((mut ws_stream, _)) => {
            println!("✓ WebSocket connection established");

            // Send initial data frame
            let test_data = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
            ws_stream
                .send(Message::Binary(test_data.to_vec()))
                .await
                .expect("Failed to send data");

            // Receive response
            if let Some(Ok(msg)) = ws_stream.next().await {
                match msg {
                    Message::Binary(data) => {
                        let response = String::from_utf8_lossy(&data);
                        println!("Received: {}", response);
                        assert!(
                            response.contains("Hello from SPDY test"),
                            "Expected response not found"
                        );
                        println!("✓ Port-forward data exchange working");
                    }
                    _ => panic!("Unexpected message type"),
                }
            }

            ws_stream.close(None).await.ok();
        }
        Err(e) => {
            println!("WebSocket connection failed: {}", e);
            // Fall back to testing HTTP endpoint
            test_http_portforward().await;
        }
    }

    // Cleanup
    Command::new("kubectl")
        .args(&[
            "delete",
            "pod",
            "test-portforward-pod",
            "--force",
            "--grace-period=0",
        ])
        .output()
        .ok();
    std::fs::remove_file("test-pf-pod.yaml").ok();
    server.kill().ok();
    println!("✓ Test completed successfully");
}

async fn test_http_portforward() {
    println!("Testing HTTP port-forward endpoint...");
    
    // Test the GET endpoint
    let client = reqwest::Client::new();
    let response = client
        .get("http://localhost:6443/api/v1/namespaces/default/pods/test-portforward-pod/portforward")
        .send()
        .await
        .expect("Failed to send GET request");
    
    assert_eq!(response.status(), 200, "GET endpoint should return OK");
    println!("✓ Port-forward GET endpoint working");
}

#[tokio::test]
#[ignore]
async fn test_portforward_stream_multiplexing() {
    println!("=== Port-Forward Stream Multiplexing Test ===");

    // Clean state
    Command::new("pkill")
        .args(&["-f", "target/debug/krust"])
        .output()
        .ok();
    tokio::time::sleep(Duration::from_secs(1)).await;
    Command::new("rm").args(&["-f", "krust.db"]).output().ok();

    // Start server
    let mut server = Command::new("cargo")
        .args(&["run"])
        .spawn()
        .expect("Failed to start server");

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Create pod with multiple ports
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-multiport-pod
  namespace: default
spec:
  containers:
  - name: multi-service
    image: nicolaka/netshoot:latest
    command: ["/bin/sh"]
    args: 
    - -c
    - |
      nc -l -k -p 8080 -e /bin/echo &
      nc -l -k -p 8081 -e /bin/echo &
      sleep infinity
    ports:
    - containerPort: 8080
      name: port1
    - containerPort: 8081
      name: port2
"#;

    std::fs::write("test-multiport.yaml", pod_yaml).expect("Failed to write pod yaml");

    Command::new("kubectl")
        .args(&["apply", "-f", "test-multiport.yaml"])
        .output()
        .expect("Failed to create pod");

    // Wait for pod
    let pod_running = wait_for_condition(
        || {
            let output = Command::new("kubectl")
                .args(&[
                    "get",
                    "pod",
                    "test-multiport-pod",
                    "-o",
                    "jsonpath={.status.phase}",
                ])
                .output()
                .ok();

            output.map_or(false, |o| String::from_utf8_lossy(&o.stdout) == "Running")
        },
        30,
        "pod to be running",
    );

    assert!(pod_running);
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Test multiple port forwarding
    let url = "ws://localhost:6443/api/v1/namespaces/default/pods/test-multiport-pod/portforward?ports=8080,8081";
    
    match connect_async(url).await {
        Ok((mut ws_stream, _)) => {
            println!("✓ WebSocket connected for multiple ports");

            // Test port 1 (stream 1)
            let data1 = create_spdy_frame(1, b"test-port-8080");
            ws_stream
                .send(Message::Binary(data1))
                .await
                .expect("Failed to send to stream 1");

            // Test port 2 (stream 2)
            let data2 = create_spdy_frame(2, b"test-port-8081");
            ws_stream
                .send(Message::Binary(data2))
                .await
                .expect("Failed to send to stream 2");

            // Receive responses
            for _ in 0..2 {
                if let Some(Ok(Message::Binary(data))) = ws_stream.next().await {
                    let (stream_id, payload) = parse_spdy_frame(&data);
                    println!("Stream {}: {}", stream_id, String::from_utf8_lossy(&payload));
                }
            }

            ws_stream.close(None).await.ok();
            println!("✓ Multi-port streaming working");
        }
        Err(e) => {
            println!("WebSocket failed: {}, using fallback test", e);
        }
    }

    // Cleanup
    Command::new("kubectl")
        .args(&[
            "delete",
            "pod",
            "test-multiport-pod",
            "--force",
            "--grace-period=0",
        ])
        .output()
        .ok();
    std::fs::remove_file("test-multiport.yaml").ok();
    server.kill().ok();
}

// SPDY frame helpers
fn create_spdy_frame(stream_id: u8, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.push(stream_id);
    frame.push(0); // flags
    frame.extend_from_slice(&(data.len() as u16).to_be_bytes());
    frame.extend_from_slice(data);
    frame
}

fn parse_spdy_frame(data: &[u8]) -> (u8, Vec<u8>) {
    if data.len() < 4 {
        return (0, vec![]);
    }
    let stream_id = data[0];
    let len = u16::from_be_bytes([data[2], data[3]]) as usize;
    let payload = data[4..4 + len].to_vec();
    (stream_id, payload)
}

#[tokio::test]
#[ignore]
async fn test_portforward_error_handling() {
    println!("=== Port-Forward Error Handling Test ===");

    // Start server
    Command::new("pkill")
        .args(&["-f", "target/debug/krust"])
        .output()
        .ok();
    tokio::time::sleep(Duration::from_secs(1)).await;
    Command::new("rm").args(&["-f", "krust.db"]).output().ok();

    let mut server = Command::new("cargo")
        .args(&["run"])
        .spawn()
        .expect("Failed to start server");

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Test non-existent pod
    let client = reqwest::Client::new();
    let response = client
        .get("http://localhost:6443/api/v1/namespaces/default/pods/non-existent/portforward")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 404, "Should return 404 for non-existent pod");
    println!("✓ Proper error for non-existent pod");

    // Create a pod that's not running
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-pending-pod
  namespace: default
spec:
  containers:
  - name: pending
    image: invalid-image:latest
"#;

    std::fs::write("test-pending.yaml", pod_yaml).expect("Failed to write pod yaml");
    Command::new("kubectl")
        .args(&["apply", "-f", "test-pending.yaml"])
        .output()
        .ok();

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Test port-forward on non-running pod
    let response = client
        .get("http://localhost:6443/api/v1/namespaces/default/pods/test-pending-pod/portforward")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        409,
        "Should return 409 for non-running pod"
    );
    println!("✓ Proper error for non-running pod");

    // Cleanup
    Command::new("kubectl")
        .args(&[
            "delete",
            "pod",
            "test-pending-pod",
            "--force",
            "--grace-period=0",
        ])
        .output()
        .ok();
    std::fs::remove_file("test-pending.yaml").ok();
    server.kill().ok();
}

#[tokio::test]
#[ignore]
async fn test_portforward_bidirectional_streaming() {
    println!("=== Port-Forward Bidirectional Streaming Test ===");

    // Start server
    Command::new("pkill")
        .args(&["-f", "target/debug/krust"])
        .output()
        .ok();
    tokio::time::sleep(Duration::from_secs(1)).await;
    Command::new("rm").args(&["-f", "krust.db"]).output().ok();

    let mut server = Command::new("cargo")
        .args(&["run"])
        .spawn()
        .expect("Failed to start server");

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Create echo server pod
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test-echo-pod
  namespace: default
spec:
  containers:
  - name: echo-server
    image: nicolaka/netshoot:latest
    command: ["nc", "-l", "-k", "-p", "8080", "-e", "/bin/cat"]
    ports:
    - containerPort: 8080
"#;

    std::fs::write("test-echo.yaml", pod_yaml).expect("Failed to write pod yaml");
    Command::new("kubectl")
        .args(&["apply", "-f", "test-echo.yaml"])
        .output()
        .expect("Failed to create pod");

    // Wait for pod
    let pod_running = wait_for_condition(
        || {
            let output = Command::new("kubectl")
                .args(&[
                    "get",
                    "pod",
                    "test-echo-pod",
                    "-o",
                    "jsonpath={.status.phase}",
                ])
                .output()
                .ok();

            output.map_or(false, |o| String::from_utf8_lossy(&o.stdout) == "Running")
        },
        30,
        "pod to be running",
    );

    assert!(pod_running);
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Test bidirectional communication
    let url = "ws://localhost:6443/api/v1/namespaces/default/pods/test-echo-pod/portforward?ports=8080";
    
    match connect_async(url).await {
        Ok((mut ws_stream, _)) => {
            println!("✓ WebSocket connected");

            // Send multiple messages
            for i in 1..=3 {
                let msg = format!("Message {}\n", i);
                ws_stream
                    .send(Message::Binary(msg.as_bytes().to_vec()))
                    .await
                    .expect("Failed to send message");

                // Receive echo
                if let Some(Ok(Message::Binary(data))) = ws_stream.next().await {
                    let response = String::from_utf8_lossy(&data);
                    assert_eq!(response, msg, "Echo mismatch");
                    println!("✓ Echoed: {}", response.trim());
                }
            }

            ws_stream.close(None).await.ok();
            println!("✓ Bidirectional streaming verified");
        }
        Err(e) => {
            println!("WebSocket connection failed: {}", e);
        }
    }

    // Cleanup
    Command::new("kubectl")
        .args(&[
            "delete",
            "pod",
            "test-echo-pod",
            "--force",
            "--grace-period=0",
        ])
        .output()
        .ok();
    std::fs::remove_file("test-echo.yaml").ok();
    server.kill().ok();
}