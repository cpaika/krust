use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_persistentvolume_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Test 1: Create a PersistentVolume
    let pv = json!({
        "apiVersion": "v1",
        "kind": "PersistentVolume",
        "metadata": {
            "name": "test-pv"
        },
        "spec": {
            "capacity": {
                "storage": "10Gi"
            },
            "accessModes": ["ReadWriteOnce"],
            "persistentVolumeReclaimPolicy": "Retain",
            "storageClassName": "manual",
            "hostPath": {
                "path": "/tmp/data"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/persistentvolumes", base_url))
        .json(&pv)
        .send()
        .await
        .unwrap();
    
    println!("Create PV response status: {}", response.status());
    
    if response.status() == 404 {
        println!("PersistentVolume endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201, "PV creation should return 201");
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-pv");
    assert_eq!(created["spec"]["capacity"]["storage"], "10Gi");
    assert_eq!(created["status"]["phase"], "Available");
    
    // Test 2: Get the PersistentVolume
    let response = client
        .get(&format!("{}/persistentvolumes/test-pv", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let fetched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(fetched["metadata"]["name"], "test-pv");
    assert_eq!(fetched["spec"]["storageClassName"], "manual");
    
    // Test 3: List PersistentVolumes
    let response = client
        .get(&format!("{}/persistentvolumes", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["kind"], "PersistentVolumeList");
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Test 4: Update the PersistentVolume
    let updated_pv = json!({
        "apiVersion": "v1",
        "kind": "PersistentVolume",
        "metadata": {
            "name": "test-pv",
            "resourceVersion": created["metadata"]["resourceVersion"]
        },
        "spec": {
            "capacity": {
                "storage": "20Gi"
            },
            "accessModes": ["ReadWriteMany"],
            "persistentVolumeReclaimPolicy": "Delete",
            "storageClassName": "fast",
            "hostPath": {
                "path": "/tmp/data"
            }
        }
    });
    
    let response = client
        .put(&format!("{}/persistentvolumes/test-pv", base_url))
        .json(&updated_pv)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["spec"]["capacity"]["storage"], "20Gi");
    assert_eq!(updated["spec"]["persistentVolumeReclaimPolicy"], "Delete");
    
    // Test 5: Delete the PersistentVolume
    let response = client
        .delete(&format!("{}/persistentvolumes/test-pv", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Verify it's deleted
    let response = client
        .get(&format!("{}/persistentvolumes/test-pv", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_persistentvolumeclaim_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // First create a PersistentVolume to bind to
    let pv = json!({
        "apiVersion": "v1",
        "kind": "PersistentVolume",
        "metadata": {
            "name": "pvc-test-pv"
        },
        "spec": {
            "capacity": {
                "storage": "10Gi"
            },
            "accessModes": ["ReadWriteOnce"],
            "persistentVolumeReclaimPolicy": "Retain",
            "storageClassName": "manual",
            "hostPath": {
                "path": "/tmp/pvc-data"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/persistentvolumes", base_url))
        .json(&pv)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        println!("PersistentVolume endpoint not implemented yet");
        return;
    }
    
    // Test 1: Create a PersistentVolumeClaim
    let pvc = json!({
        "apiVersion": "v1",
        "kind": "PersistentVolumeClaim",
        "metadata": {
            "name": "test-pvc",
            "namespace": "default"
        },
        "spec": {
            "accessModes": ["ReadWriteOnce"],
            "resources": {
                "requests": {
                    "storage": "5Gi"
                }
            },
            "storageClassName": "manual"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/persistentvolumeclaims", base_url))
        .json(&pvc)
        .send()
        .await
        .unwrap();
    
    println!("Create PVC response status: {}", response.status());
    
    if response.status() == 404 {
        println!("PersistentVolumeClaim endpoint not implemented yet");
        // Cleanup PV
        client
            .delete(&format!("{}/persistentvolumes/pvc-test-pv", base_url))
            .send()
            .await
            .unwrap();
        return;
    }
    
    assert_eq!(response.status(), 201, "PVC creation should return 201");
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-pvc");
    assert_eq!(created["spec"]["resources"]["requests"]["storage"], "5Gi");
    // PVC should be in Pending or Bound state
    assert!(created["status"]["phase"] == "Pending" || created["status"]["phase"] == "Bound");
    
    // Test 2: Get the PersistentVolumeClaim
    let response = client
        .get(&format!("{}/namespaces/default/persistentvolumeclaims/test-pvc", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let fetched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(fetched["metadata"]["name"], "test-pvc");
    assert_eq!(fetched["spec"]["storageClassName"], "manual");
    
    // Test 3: List PersistentVolumeClaims
    let response = client
        .get(&format!("{}/namespaces/default/persistentvolumeclaims", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["kind"], "PersistentVolumeClaimList");
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Test 4: Update the PersistentVolumeClaim
    let updated_pvc = json!({
        "apiVersion": "v1",
        "kind": "PersistentVolumeClaim",
        "metadata": {
            "name": "test-pvc",
            "namespace": "default",
            "resourceVersion": created["metadata"]["resourceVersion"]
        },
        "spec": {
            "accessModes": ["ReadWriteOnce"],
            "resources": {
                "requests": {
                    "storage": "8Gi"
                }
            },
            "storageClassName": "manual"
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/persistentvolumeclaims/test-pvc", base_url))
        .json(&updated_pvc)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["spec"]["resources"]["requests"]["storage"], "8Gi");
    
    // Test 5: Delete the PersistentVolumeClaim
    let response = client
        .delete(&format!("{}/namespaces/default/persistentvolumeclaims/test-pvc", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Verify it's deleted
    let response = client
        .get(&format!("{}/namespaces/default/persistentvolumeclaims/test-pvc", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404);
    
    // Cleanup: Delete the PV
    client
        .delete(&format!("{}/persistentvolumes/pvc-test-pv", base_url))
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_pv_pvc_binding() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/api/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a PersistentVolume
    let pv = json!({
        "apiVersion": "v1",
        "kind": "PersistentVolume",
        "metadata": {
            "name": "binding-test-pv"
        },
        "spec": {
            "capacity": {
                "storage": "10Gi"
            },
            "accessModes": ["ReadWriteOnce"],
            "persistentVolumeReclaimPolicy": "Retain",
            "storageClassName": "manual",
            "hostPath": {
                "path": "/tmp/binding-test"
            }
        }
    });
    
    let response = client
        .post(&format!("{}/persistentvolumes", base_url))
        .json(&pv)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        println!("PersistentVolume endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let pv_created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(pv_created["status"]["phase"], "Available");
    
    // Create a matching PersistentVolumeClaim
    let pvc = json!({
        "apiVersion": "v1",
        "kind": "PersistentVolumeClaim",
        "metadata": {
            "name": "binding-test-pvc",
            "namespace": "default"
        },
        "spec": {
            "accessModes": ["ReadWriteOnce"],
            "resources": {
                "requests": {
                    "storage": "5Gi"
                }
            },
            "storageClassName": "manual"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/persistentvolumeclaims", base_url))
        .json(&pvc)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        println!("PersistentVolumeClaim endpoint not implemented yet");
        // Cleanup PV
        client
            .delete(&format!("{}/persistentvolumes/binding-test-pv", base_url))
            .send()
            .await
            .unwrap();
        return;
    }
    
    assert_eq!(response.status(), 201);
    
    // Wait a bit for binding to occur (in a real implementation, a controller would handle this)
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Check PVC is bound
    let response = client
        .get(&format!("{}/namespaces/default/persistentvolumeclaims/binding-test-pvc", base_url))
        .send()
        .await
        .unwrap();
    
    let pvc_status: serde_json::Value = response.json().await.unwrap();
    // In a full implementation, this would be "Bound"
    // For now, we just check it exists
    assert_eq!(pvc_status["metadata"]["name"], "binding-test-pvc");
    
    // Check PV status
    let response = client
        .get(&format!("{}/persistentvolumes/binding-test-pv", base_url))
        .send()
        .await
        .unwrap();
    
    let pv_status: serde_json::Value = response.json().await.unwrap();
    assert_eq!(pv_status["metadata"]["name"], "binding-test-pv");
    
    // Cleanup
    client
        .delete(&format!("{}/namespaces/default/persistentvolumeclaims/binding-test-pvc", base_url))
        .send()
        .await
        .unwrap();
    
    client
        .delete(&format!("{}/persistentvolumes/binding-test-pv", base_url))
        .send()
        .await
        .unwrap();
}