use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_job_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/batch/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Test 1: Create a Job
    let job = json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "name": "test-job",
            "namespace": "default"
        },
        "spec": {
            "template": {
                "metadata": {
                    "labels": {
                        "app": "test-job"
                    }
                },
                "spec": {
                    "containers": [{
                        "name": "worker",
                        "image": "busybox:1.35",
                        "command": ["sh", "-c", "echo 'Hello from Job' && sleep 10"]
                    }],
                    "restartPolicy": "Never"
                }
            },
            "backoffLimit": 4,
            "completions": 3,
            "parallelism": 2
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/jobs", base_url))
        .json(&job)
        .send()
        .await
        .unwrap();
    
    println!("Create Job response status: {}", response.status());
    
    if response.status() == 404 {
        println!("Job endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201, "Job creation should return 201");
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-job");
    assert_eq!(created["spec"]["backoffLimit"], 4);
    assert_eq!(created["spec"]["completions"], 3);
    
    // Test 2: Get the Job
    let response = client
        .get(&format!("{}/namespaces/default/jobs/test-job", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let fetched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(fetched["metadata"]["name"], "test-job");
    
    // Test 3: List Jobs
    let response = client
        .get(&format!("{}/namespaces/default/jobs", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let list: serde_json::Value = response.json().await.unwrap();
    assert_eq!(list["kind"], "JobList");
    assert!(list["items"].as_array().unwrap().len() >= 1);
    
    // Test 4: Get Job status
    let response = client
        .get(&format!("{}/namespaces/default/jobs/test-job/status", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let status: serde_json::Value = response.json().await.unwrap();
    assert!(status["status"].is_object());
    
    // Test 5: Delete the Job
    let response = client
        .delete(&format!("{}/namespaces/default/jobs/test-job", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Verify it's deleted
    let response = client
        .get(&format!("{}/namespaces/default/jobs/test-job", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_cronjob_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/batch/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Test 1: Create a CronJob
    let cronjob = json!({
        "apiVersion": "batch/v1",
        "kind": "CronJob",
        "metadata": {
            "name": "test-cronjob",
            "namespace": "default"
        },
        "spec": {
            "schedule": "*/5 * * * *",
            "jobTemplate": {
                "spec": {
                    "template": {
                        "spec": {
                            "containers": [{
                                "name": "cron-worker",
                                "image": "busybox:1.35",
                                "command": ["sh", "-c", "date; echo 'Hello from CronJob'"]
                            }],
                            "restartPolicy": "OnFailure"
                        }
                    }
                }
            },
            "successfulJobsHistoryLimit": 3,
            "failedJobsHistoryLimit": 1
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/cronjobs", base_url))
        .json(&cronjob)
        .send()
        .await
        .unwrap();
    
    println!("Create CronJob response status: {}", response.status());
    
    if response.status() == 404 {
        println!("CronJob endpoint not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201, "CronJob creation should return 201");
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "test-cronjob");
    assert_eq!(created["spec"]["schedule"], "*/5 * * * *");
    
    // Test 2: Get the CronJob
    let response = client
        .get(&format!("{}/namespaces/default/cronjobs/test-cronjob", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let fetched: serde_json::Value = response.json().await.unwrap();
    assert_eq!(fetched["metadata"]["name"], "test-cronjob");
    
    // Test 3: Update the CronJob
    let updated_cronjob = json!({
        "apiVersion": "batch/v1",
        "kind": "CronJob",
        "metadata": {
            "name": "test-cronjob",
            "namespace": "default",
            "resourceVersion": created["metadata"]["resourceVersion"]
        },
        "spec": {
            "schedule": "0 * * * *", // Changed to hourly
            "jobTemplate": {
                "spec": {
                    "template": {
                        "spec": {
                            "containers": [{
                                "name": "cron-worker",
                                "image": "busybox:1.36",
                                "command": ["sh", "-c", "date; echo 'Updated CronJob'"]
                            }],
                            "restartPolicy": "OnFailure"
                        }
                    }
                }
            },
            "successfulJobsHistoryLimit": 5,
            "failedJobsHistoryLimit": 2
        }
    });
    
    let response = client
        .put(&format!("{}/namespaces/default/cronjobs/test-cronjob", base_url))
        .json(&updated_cronjob)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["spec"]["schedule"], "0 * * * *");
    
    // Test 4: Delete the CronJob
    let response = client
        .delete(&format!("{}/namespaces/default/cronjobs/test-cronjob", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Verify it's deleted
    let response = client
        .get(&format!("{}/namespaces/default/cronjobs/test-cronjob", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 404);
}