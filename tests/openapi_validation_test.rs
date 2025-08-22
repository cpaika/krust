use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
#[ignore] // Run with: cargo test --test openapi_validation_test -- --ignored --nocapture
fn test_kubectl_validation_without_flag() {
    println!("=== Testing kubectl validation without --validate=false ===");
    
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
    println!("Starting Krust server...");
    let mut server = Command::new("cargo")
        .args(&["run"])
        .spawn()
        .expect("Failed to start server");
    
    thread::sleep(Duration::from_secs(3));
    
    // Test OpenAPI endpoint
    println!("Testing OpenAPI endpoint...");
    let output = Command::new("curl")
        .args(&["-s", "http://localhost:6443/openapi/v2"])
        .output()
        .expect("Failed to fetch OpenAPI");
    
    let openapi = String::from_utf8_lossy(&output.stdout);
    assert!(openapi.contains("\"swagger\":\"2.0\""), "OpenAPI endpoint not returning valid swagger");
    
    // Test that kubectl can download and parse OpenAPI
    println!("\nTesting kubectl OpenAPI discovery...");
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "api-resources",
            "-v=6"  // Verbose to see what's happening
        ])
        .output()
        .expect("Failed to run kubectl api-resources");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    println!("STDOUT: {}", stdout);
    println!("STDERR: {}", stderr);
    
    // Check that pods resource is discovered
    assert!(stdout.contains("pods") || stderr.contains("pods"), 
        "kubectl did not discover pods resource");
    
    // The real test: create a pod WITHOUT --validate=false
    println!("\nCreating pod WITHOUT --validate=false...");
    let pod_yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: validation-test
  labels:
    test: validation
spec:
  containers:
  - name: nginx
    image: nginx:alpine
    ports:
    - containerPort: 80
"#;
    
    std::fs::write("validation-test.yaml", pod_yaml).expect("Failed to write pod yaml");
    
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "validation-test.yaml"
            // NO --validate=false flag!
        ])
        .output()
        .expect("Failed to create pod");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    println!("Create STDOUT: {}", stdout);
    println!("Create STDERR: {}", stderr);
    
    // Check if pod was created (validation should pass)
    if stderr.contains("error validating") {
        // If validation failed, check what the error is
        if stderr.contains("proto: cannot parse invalid wire-format data") {
            panic!("OpenAPI format issue: kubectl cannot parse our OpenAPI schema");
        } else if stderr.contains("failed to download openapi") {
            panic!("OpenAPI download issue: kubectl cannot fetch our OpenAPI schema");
        } else {
            panic!("Validation failed with: {}", stderr);
        }
    }
    
    // Pod should be created
    assert!(stdout.contains("created") || stdout.contains("configured"), 
        "Pod was not created successfully without --validate=false");
    
    println!("✅ Pod created successfully WITHOUT --validate=false!");
    
    // Verify pod exists
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "pod",
            "validation-test"
        ])
        .output()
        .expect("Failed to get pod");
    
    assert!(output.status.success(), "Failed to get created pod");
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "pod",
            "validation-test"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "validation-test.yaml"])
        .output()
        .ok();
    
    server.kill().expect("Failed to kill server");
    
    println!("\n✅ OpenAPI validation test passed!");
}

#[test]
#[ignore]
fn test_openapi_completeness() {
    println!("=== Testing OpenAPI Schema Completeness ===");
    
    // Start server
    Command::new("pkill")
        .args(&["-f", "target/debug/krust"])
        .output()
        .ok();
    
    thread::sleep(Duration::from_secs(1));
    
    let mut server = Command::new("cargo")
        .args(&["run"])
        .spawn()
        .expect("Failed to start server");
    
    thread::sleep(Duration::from_secs(3));
    
    // Fetch OpenAPI schema
    let output = Command::new("curl")
        .args(&["-s", "http://localhost:6443/openapi/v2"])
        .output()
        .expect("Failed to fetch OpenAPI");
    
    let openapi_json = String::from_utf8_lossy(&output.stdout);
    
    // Parse as JSON to validate structure
    let schema: serde_json::Value = serde_json::from_str(&openapi_json)
        .expect("OpenAPI schema is not valid JSON");
    
    // Check required top-level fields
    assert!(schema["swagger"].is_string(), "Missing swagger version");
    assert!(schema["info"].is_object(), "Missing info section");
    assert!(schema["paths"].is_object(), "Missing paths section");
    assert!(schema["definitions"].is_object(), "Missing definitions section");
    
    // Check that key paths are defined
    let paths = &schema["paths"];
    assert!(paths["/api/v1/namespaces/{namespace}/pods"].is_object(), 
        "Missing pod namespace path");
    assert!(paths["/api/v1/namespaces/{namespace}/pods/{name}"].is_object(), 
        "Missing pod name path");
    
    // Check that key definitions exist
    let definitions = &schema["definitions"];
    assert!(definitions["io.k8s.api.core.v1.Pod"].is_object(), 
        "Missing Pod definition");
    assert!(definitions["io.k8s.api.core.v1.PodSpec"].is_object(), 
        "Missing PodSpec definition");
    assert!(definitions["io.k8s.api.core.v1.Container"].is_object(), 
        "Missing Container definition");
    
    // Validate Pod definition has required fields
    let pod_def = &definitions["io.k8s.api.core.v1.Pod"];
    assert!(pod_def["properties"]["spec"].is_object(), 
        "Pod missing spec property");
    assert!(pod_def["properties"]["metadata"].is_object(), 
        "Pod missing metadata property");
    
    println!("✅ OpenAPI schema structure is complete");
    
    server.kill().ok();
}