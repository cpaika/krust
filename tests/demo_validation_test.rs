use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
#[ignore]
async fn test_demo_yaml_validation() {
    // Start the server
    let mut server = Command::new("cargo")
        .args(&["run"])
        .spawn()
        .expect("Failed to start server");

    // Wait for server to be ready
    sleep(Duration::from_secs(3)).await;

    // First, test that we get the OpenAPI validation error
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "demo.yaml",
        ])
        .output()
        .expect("Failed to run kubectl");

    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("kubectl stderr: {}", stderr);
    
    // Check if validation succeeds (it should!)
    if !output.status.success() {
        if stderr.contains("error validating") && stderr.contains("openapi") {
            panic!("OpenAPI validation failed: {}", stderr);
        }
        panic!("kubectl apply failed: {}", stderr);
    }

    // Verify resources were created
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "deployments",
        ])
        .output()
        .expect("Failed to get deployments");

    assert!(output.status.success(), "Failed to get deployments");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("webapp"), "Deployment not found");

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