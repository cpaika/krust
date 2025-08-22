use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
#[ignore]
async fn test_demo_yaml_with_validate_false() {
    // Start the server
    let mut server = Command::new("cargo")
        .args(&["run"])
        .spawn()
        .expect("Failed to start server");

    // Wait for server to be ready
    sleep(Duration::from_secs(3)).await;

    // Apply demo.yaml with --validate=false (required workaround)
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "demo.yaml",
            "--validate=false",
        ])
        .output()
        .expect("Failed to run kubectl");

    assert!(output.status.success(), "Failed to apply demo.yaml");

    // Verify deployment was created
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "deployments",
        ])
        .output()
        .expect("Failed to get deployments");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("webapp"), "Deployment not found");

    // Verify service was created
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "services",
        ])
        .output()
        .expect("Failed to get services");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("webapp-service"), "Service not found");

    // Wait for pods to be created
    sleep(Duration::from_secs(2)).await;

    // Verify pods were created
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "pods",
        ])
        .output()
        .expect("Failed to get pods");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("webapp"), "Pods not found");

    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "-f",
            "demo.yaml",
        ])
        .output()
        .expect("Failed to delete resources");

    server.kill().expect("Failed to kill server");
}