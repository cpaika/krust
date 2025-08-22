// Simple HTTP proxy to expose pod containers
use axum::{
    extract::{Path, State},
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Response},
    body::Body,
};
use bollard::Docker;
use bollard::exec::{CreateExecOptions, StartExecResults};
use futures::StreamExt;
use tracing::{error, info};

use super::server::AppState;

// Direct proxy endpoint - access pods via /proxy/pods/{namespace}/{name}/{port}/...
pub async fn proxy_to_pod(
    State(state): State<AppState>,
    Path((namespace, name, port, path)): Path<(String, String, u16, String)>,
    headers: HeaderMap,
    body: String,
) -> Result<Response, StatusCode> {
    // Verify pod exists and is running
    let pod = state
        .storage
        .pods()
        .get(&namespace, &name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let status = pod
        .get("status")
        .and_then(|s| s.as_object())
        .ok_or(StatusCode::CONFLICT)?;
    
    let phase = status
        .get("phase")
        .and_then(|p| p.as_str())
        .ok_or(StatusCode::CONFLICT)?;
    
    if phase != "Running" {
        return Err(StatusCode::CONFLICT);
    }

    // Get container ID
    let container_id = get_container_id(&namespace, &name).await
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Build the curl command to execute inside the container
    let url = if path.is_empty() {
        format!("http://localhost:{}/", port)
    } else {
        format!("http://localhost:{}/{}", port, path)
    };
    
    info!("Proxying request to {} via container exec", url);

    // Execute curl inside the container
    let response = exec_curl_in_container(&container_id, &url).await
        .map_err(|e| {
            error!("Failed to proxy request: {}", e);
            StatusCode::BAD_GATEWAY
        })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Body::from(response))
        .unwrap())
}

// Simpler proxy without path
pub async fn proxy_to_pod_root(
    state: State<AppState>,
    Path((namespace, name, port)): Path<(String, String, u16)>,
    headers: HeaderMap,
    body: String,
) -> Result<Response, StatusCode> {
    proxy_to_pod(state, Path((namespace, name, port, String::new())), headers, body).await
}

async fn get_container_id(namespace: &str, name: &str) -> Option<String> {
    let docker = Docker::connect_with_local_defaults().ok()?;
    
    // Find container by pod labels
    let pod_label = format!("io.kubernetes.pod.name={}", name);
    let namespace_label = format!("io.kubernetes.pod.namespace={}", namespace);
    
    let mut filter_map = std::collections::HashMap::new();
    filter_map.entry("label".to_string()).or_insert_with(Vec::new).push(pod_label);
    filter_map.entry("label".to_string()).or_insert_with(Vec::new).push(namespace_label);
    
    let options = bollard::container::ListContainersOptions {
        all: true,
        filters: filter_map,
        ..Default::default()
    };
    
    let containers = docker.list_containers(Some(options)).await.ok()?;
    let container = containers.first()?;
    container.id.clone()
}

async fn exec_curl_in_container(container_id: &str, url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;
    
    // Try different shell approaches for compatibility with minimal containers
    // First try with sh, then with /bin/sh, then direct curl
    let commands = vec![
        vec!["sh".to_string(), "-c".to_string(), format!("curl -s --max-time 5 {}", url)],
        vec!["/bin/sh".to_string(), "-c".to_string(), format!("curl -s --max-time 5 {}", url)],
        vec!["curl".to_string(), "-s".to_string(), "--max-time".to_string(), "5".to_string(), url.to_string()],
        vec!["wget".to_string(), "-q".to_string(), "-O".to_string(), "-".to_string(), "--timeout=5".to_string(), url.to_string()],
    ];
    
    for cmd in commands {
        let exec_config = CreateExecOptions {
            cmd: Some(cmd.clone()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };
        
        let exec = match docker.create_exec(container_id, exec_config).await {
            Ok(exec) => exec,
            Err(_) => continue, // Try next command
        };
        
        // Start the exec and collect output
        if let Ok(StartExecResults::Attached { mut output, .. }) = docker.start_exec(&exec.id, None).await {
            let mut result = String::new();
            let mut has_error = false;
            
            while let Some(Ok(msg)) = output.next().await {
                match msg {
                    bollard::container::LogOutput::StdOut { message } => {
                        result.push_str(&String::from_utf8_lossy(&message));
                    }
                    bollard::container::LogOutput::StdErr { message } => {
                        let error_msg = String::from_utf8_lossy(&message);
                        // Check if it's a real error or just info on stderr
                        if error_msg.contains("not found") || error_msg.contains("error") {
                            has_error = true;
                        }
                    }
                    _ => {}
                }
            }
            
            if !has_error && !result.is_empty() {
                return Ok(result);
            }
        }
    }
    
    Err("Could not execute HTTP request in container - no suitable command found".into())
}