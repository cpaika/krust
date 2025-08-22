use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
#[ignore] // Run with: cargo test --test deployment_test -- --ignored --nocapture
fn test_deployment_creation() {
    println!("=== Testing Deployment Creation ===");
    
    // Clean state
    Command::new("pkill")
        .args(&["-f", "target/release/krust"])
        .output()
        .ok();
    
    thread::sleep(Duration::from_secs(1));
    
    Command::new("rm")
        .args(&["-f", "krust.db"])
        .output()
        .ok();
    
    // Start server
    println!("Starting Krust server...");
    let mut server = Command::new("./target/release/krust")
        .spawn()
        .expect("Failed to start server");
    
    thread::sleep(Duration::from_secs(2));
    
    // Create a deployment
    let deployment_yaml = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: nginx-deployment
spec:
  replicas: 3
  selector:
    matchLabels:
      app: nginx
  template:
    metadata:
      labels:
        app: nginx
    spec:
      containers:
      - name: nginx
        image: nginx:alpine
        ports:
        - containerPort: 80
"#;
    
    std::fs::write("deployment.yaml", deployment_yaml).expect("Failed to write deployment yaml");
    
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "deployment.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create deployment");
    
    assert!(output.status.success(), "Failed to create deployment");
    println!("Deployment created: {}", String::from_utf8_lossy(&output.stdout));
    
    // Check deployment was created
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "deployment",
            "nginx-deployment",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get deployment");
    
    let deployment_json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse deployment JSON");
    
    assert_eq!(
        deployment_json["spec"]["replicas"].as_i64(),
        Some(3),
        "Replicas not set correctly"
    );
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "deployment",
            "nginx-deployment"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "deployment.yaml"])
        .output()
        .ok();
    
    server.kill().expect("Failed to kill server");
    
    println!("\n✅ Deployment creation test passed!");
}

#[test]
#[ignore]
fn test_deployment_creates_replicaset() {
    println!("=== Testing Deployment Creates ReplicaSet ===");
    
    // Clean state
    Command::new("pkill")
        .args(&["-f", "target/release/krust"])
        .output()
        .ok();
    
    thread::sleep(Duration::from_secs(1));
    
    Command::new("rm")
        .args(&["-f", "krust.db"])
        .output()
        .ok();
    
    // Start server
    let mut server = Command::new("./target/release/krust")
        .spawn()
        .expect("Failed to start server");
    
    thread::sleep(Duration::from_secs(2));
    
    // Create a deployment
    let deployment_yaml = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test-deployment
spec:
  replicas: 2
  selector:
    matchLabels:
      app: test
  template:
    metadata:
      labels:
        app: test
    spec:
      containers:
      - name: nginx
        image: nginx:alpine
"#;
    
    std::fs::write("deployment.yaml", deployment_yaml).expect("Failed to write deployment yaml");
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "deployment.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create deployment");
    
    // Wait for controller to create ReplicaSet
    thread::sleep(Duration::from_secs(3));
    
    // Check that a ReplicaSet was created
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "replicasets",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get replicasets");
    
    let rs_list: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse replicaset list");
    
    let items = rs_list["items"].as_array().unwrap();
    assert!(items.len() > 0, "No ReplicaSet created by Deployment");
    
    // Verify the ReplicaSet is owned by the Deployment
    let rs = &items[0];
    let owner_refs = rs["metadata"]["ownerReferences"].as_array();
    assert!(owner_refs.is_some(), "ReplicaSet should have ownerReferences");
    
    let owner_ref = &owner_refs.unwrap()[0];
    assert_eq!(
        owner_ref["kind"].as_str(),
        Some("Deployment"),
        "ReplicaSet should be owned by Deployment"
    );
    assert_eq!(
        owner_ref["name"].as_str(),
        Some("test-deployment"),
        "ReplicaSet should be owned by test-deployment"
    );
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "deployment",
            "test-deployment"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "deployment.yaml"])
        .output()
        .ok();
    
    server.kill().ok();
    
    println!("\n✅ Deployment ReplicaSet creation test passed!");
}

#[test]
#[ignore]
fn test_deployment_creates_pods() {
    println!("=== Testing Deployment Creates Pods ===");
    
    // Clean state
    Command::new("pkill")
        .args(&["-f", "target/release/krust"])
        .output()
        .ok();
    
    thread::sleep(Duration::from_secs(1));
    
    Command::new("rm")
        .args(&["-f", "krust.db"])
        .output()
        .ok();
    
    // Start server
    let mut server = Command::new("./target/release/krust")
        .spawn()
        .expect("Failed to start server");
    
    thread::sleep(Duration::from_secs(2));
    
    // Create a deployment with 3 replicas
    let deployment_yaml = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: webapp
spec:
  replicas: 3
  selector:
    matchLabels:
      app: webapp
  template:
    metadata:
      labels:
        app: webapp
        tier: frontend
    spec:
      containers:
      - name: webapp
        image: nginx:alpine
"#;
    
    std::fs::write("deployment.yaml", deployment_yaml).expect("Failed to write deployment yaml");
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "deployment.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create deployment");
    
    // Wait for pods to be created
    thread::sleep(Duration::from_secs(5));
    
    // Check that pods were created
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "pods",
            "-l",
            "app=webapp",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get pods");
    
    let pods_list: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse pods list");
    
    let items = pods_list["items"].as_array().unwrap();
    assert_eq!(items.len(), 3, "Should have exactly 3 pods");
    
    // Verify pods have correct labels
    for pod in items {
        assert_eq!(
            pod["metadata"]["labels"]["app"].as_str(),
            Some("webapp"),
            "Pod should have app=webapp label"
        );
        assert_eq!(
            pod["metadata"]["labels"]["tier"].as_str(),
            Some("frontend"),
            "Pod should have tier=frontend label"
        );
    }
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "deployment",
            "webapp"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "deployment.yaml"])
        .output()
        .ok();
    
    server.kill().ok();
    
    println!("\n✅ Deployment pod creation test passed!");
}

#[test]
#[ignore]
fn test_deployment_scaling() {
    println!("=== Testing Deployment Scaling ===");
    
    // Clean state
    Command::new("pkill")
        .args(&["-f", "target/release/krust"])
        .output()
        .ok();
    
    thread::sleep(Duration::from_secs(1));
    
    Command::new("rm")
        .args(&["-f", "krust.db"])
        .output()
        .ok();
    
    // Start server
    let mut server = Command::new("./target/release/krust")
        .spawn()
        .expect("Failed to start server");
    
    thread::sleep(Duration::from_secs(2));
    
    // Create a deployment with 2 replicas
    let deployment_yaml = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: scalable-app
spec:
  replicas: 2
  selector:
    matchLabels:
      app: scalable
  template:
    metadata:
      labels:
        app: scalable
    spec:
      containers:
      - name: app
        image: nginx:alpine
"#;
    
    std::fs::write("deployment.yaml", deployment_yaml).expect("Failed to write deployment yaml");
    
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "apply",
            "-f",
            "deployment.yaml",
            "--validate=false"
        ])
        .output()
        .expect("Failed to create deployment");
    
    thread::sleep(Duration::from_secs(3));
    
    // Check initial pod count
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "pods",
            "-l",
            "app=scalable",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get pods");
    
    let pods_list: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse pods list");
    
    assert_eq!(
        pods_list["items"].as_array().unwrap().len(),
        2,
        "Should initially have 2 pods"
    );
    
    // Scale up to 5 replicas
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "scale",
            "deployment",
            "scalable-app",
            "--replicas=5"
        ])
        .output()
        .expect("Failed to scale deployment");
    
    thread::sleep(Duration::from_secs(3));
    
    // Check pod count after scaling up
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "pods",
            "-l",
            "app=scalable",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get pods");
    
    let pods_list: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse pods list");
    
    assert_eq!(
        pods_list["items"].as_array().unwrap().len(),
        5,
        "Should have 5 pods after scaling up"
    );
    
    // Scale down to 1 replica
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "scale",
            "deployment",
            "scalable-app",
            "--replicas=1"
        ])
        .output()
        .expect("Failed to scale deployment");
    
    thread::sleep(Duration::from_secs(3));
    
    // Check pod count after scaling down
    let output = Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "get",
            "pods",
            "-l",
            "app=scalable",
            "-o",
            "json"
        ])
        .output()
        .expect("Failed to get pods");
    
    let pods_list: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse pods list");
    
    assert_eq!(
        pods_list["items"].as_array().unwrap().len(),
        1,
        "Should have 1 pod after scaling down"
    );
    
    // Clean up
    Command::new("kubectl")
        .args(&[
            "--server=http://localhost:6443",
            "delete",
            "deployment",
            "scalable-app"
        ])
        .output()
        .ok();
    
    Command::new("rm")
        .args(&["-f", "deployment.yaml"])
        .output()
        .ok();
    
    server.kill().ok();
    
    println!("\n✅ Deployment scaling test passed!");
}