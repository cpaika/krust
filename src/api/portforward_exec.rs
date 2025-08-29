use bollard::{
    container::{Config, CreateContainerOptions, StartContainerOptions},
    exec::{CreateExecOptions, StartExecResults},
    Docker,
};
use futures::StreamExt;
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info};

/// Port forwarding using Docker exec with socat
pub async fn setup_exec_portforward(
    container_id: &str,
    remote_port: u16,
    local_port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;
    
    // First, check if socat is available in the container
    let check_socat = CreateExecOptions {
        cmd: Some(vec!["which", "socat"]),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Default::default()
    };
    
    let exec = docker.create_exec(container_id, check_socat).await?;
    let output = docker.start_exec(&exec.id, None).await?;
    
    let has_socat = match output {
        StartExecResults::Attached { mut output, .. } => {
            let mut has_socat = false;
            while let Some(chunk) = output.next().await {
                if let Ok(chunk) = chunk {
                    match chunk {
                        bollard::container::LogOutput::StdOut { message } |
                        bollard::container::LogOutput::StdErr { message } => {
                            let msg = String::from_utf8_lossy(&message);
                            if msg.contains("socat") {
                                has_socat = true;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            has_socat
        }
        _ => false,
    };
    
    if has_socat {
        info!("Using socat for port forwarding");
        setup_socat_forward(&docker, container_id, remote_port, local_port).await
    } else {
        info!("Socat not available, using netcat fallback");
        setup_netcat_forward(&docker, container_id, remote_port, local_port).await
    }
}

async fn setup_socat_forward(
    docker: &Docker,
    container_id: &str,
    remote_port: u16,
    local_port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a listener on the local port
    let listener = TcpListener::bind(format!("127.0.0.1:{}", local_port)).await?;
    info!("Listening on localhost:{} for socat forward", local_port);
    
    // Accept connections and forward them
    loop {
        let (stream, _addr) = listener.accept().await?;
        let container_id = container_id.to_string();
        let docker = docker.clone();
        
        tokio::spawn(async move {
            if let Err(e) = handle_socat_connection(docker, container_id, remote_port, stream).await {
                error!("Socat connection error: {}", e);
            }
        });
    }
}

async fn handle_socat_connection(
    docker: Docker,
    container_id: String,
    remote_port: u16,
    stream: TcpStream,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create exec with socat to forward to localhost:remote_port
    let socat_target = format!("TCP:localhost:{}", remote_port);
    let exec_config = CreateExecOptions {
        cmd: Some(vec![
            "socat",
            "-",
            &socat_target,
        ]),
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        tty: Some(false),
        ..Default::default()
    };
    
    let exec = docker.create_exec(&container_id, exec_config).await?;
    let exec_output = docker.start_exec(&exec.id, None).await?;
    
    match exec_output {
        StartExecResults::Attached {
            mut output,
            mut input,
        } => {
            // Spawn task to copy from TCP to exec stdin
            let (mut tcp_read, mut tcp_write) = tokio::io::split(stream);
            
            let input_handle = tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                loop {
                    match tcp_read.read(&mut buffer).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if input.write_all(&buffer[..n]).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
            
            // Copy from exec stdout to TCP
            let output_handle = tokio::spawn(async move {
                while let Some(chunk) = output.next().await {
                    if let Ok(chunk) = chunk {
                        match chunk {
                            bollard::container::LogOutput::StdOut { message } => {
                                if tcp_write.write_all(&message).await.is_err() {
                                    break;
                                }
                            }
                            bollard::container::LogOutput::StdErr { message } => {
                                debug!("Exec stderr: {}", String::from_utf8_lossy(&message));
                            }
                            _ => {}
                        }
                    }
                }
            });
            
            // Wait for either task to complete
            tokio::select! {
                _ = input_handle => {},
                _ = output_handle => {},
            }
        }
        _ => {
            error!("Failed to attach to exec");
        }
    }
    
    Ok(())
}

async fn setup_netcat_forward(
    docker: &Docker,
    container_id: &str,
    remote_port: u16,
    local_port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check for nc or netcat
    let check_nc = CreateExecOptions {
        cmd: Some(vec!["sh", "-c", "which nc || which netcat"]),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Default::default()
    };
    
    let exec = docker.create_exec(container_id, check_nc).await?;
    let output = docker.start_exec(&exec.id, None).await?;
    
    let nc_cmd = match output {
        StartExecResults::Attached { mut output, .. } => {
            let mut nc_cmd = None;
            while let Some(chunk) = output.next().await {
                if let Ok(chunk) = chunk {
                    match chunk {
                        bollard::container::LogOutput::StdOut { message } => {
                            let msg = String::from_utf8_lossy(&message);
                            if msg.contains("nc") {
                                nc_cmd = Some("nc");
                                break;
                            } else if msg.contains("netcat") {
                                nc_cmd = Some("netcat");
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            nc_cmd
        }
        _ => None,
    };
    
    if let Some(nc) = nc_cmd {
        // Create a listener on the local port
        let listener = TcpListener::bind(format!("127.0.0.1:{}", local_port)).await?;
        info!("Listening on localhost:{} for netcat forward", local_port);
        
        loop {
            let (stream, _) = listener.accept().await?;
            let container_id = container_id.to_string();
            let docker = docker.clone();
            let nc = nc.to_string();
            
            tokio::spawn(async move {
                if let Err(e) = handle_netcat_connection(docker, container_id, nc, remote_port, stream).await {
                    error!("Netcat connection error: {}", e);
                }
            });
        }
    } else {
        error!("Neither socat nor netcat available in container");
        Err("No suitable forwarding tool available".into())
    }
}

async fn handle_netcat_connection(
    docker: Docker,
    container_id: String,
    nc_cmd: String,
    remote_port: u16,
    stream: TcpStream,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create exec with netcat
    let port_str = remote_port.to_string();
    let exec_config = CreateExecOptions {
        cmd: Some(vec![
            &nc_cmd,
            "localhost",
            &port_str,
        ]),
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        tty: Some(false),
        ..Default::default()
    };
    
    let exec = docker.create_exec(&container_id, exec_config).await?;
    let exec_output = docker.start_exec(&exec.id, None).await?;
    
    match exec_output {
        StartExecResults::Attached {
            mut output,
            mut input,
        } => {
            let (mut tcp_read, mut tcp_write) = tokio::io::split(stream);
            
            // Copy from TCP to exec stdin
            let input_handle = tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                loop {
                    match tcp_read.read(&mut buffer).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if input.write_all(&buffer[..n]).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
            
            // Copy from exec stdout to TCP
            let output_handle = tokio::spawn(async move {
                while let Some(chunk) = output.next().await {
                    if let Ok(chunk) = chunk {
                        match chunk {
                            bollard::container::LogOutput::StdOut { message } => {
                                if tcp_write.write_all(&message).await.is_err() {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            });
            
            // Wait for either task to complete
            tokio::select! {
                _ = input_handle => {},
                _ = output_handle => {},
            }
        }
        _ => {
            error!("Failed to attach to exec");
        }
    }
    
    Ok(())
}

/// Alternative: Use a sidecar container for port forwarding
pub async fn setup_sidecar_portforward(
    docker: &Docker,
    pod_namespace: &str,
    pod_name: &str,
    container_ip: &str,
    remote_port: u16,
    local_port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a sidecar container with socat that forwards traffic
    let sidecar_name = format!("krust-pf-{}-{}-{}", pod_namespace, pod_name, local_port);
    
    // Check if sidecar already exists
    let mut filters = HashMap::new();
    filters.insert("name".to_string(), vec![sidecar_name.clone()]);
    let options = bollard::container::ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };
    
    let containers = docker.list_containers(Some(options)).await?;
    if !containers.is_empty() {
        // Remove existing sidecar
        docker.remove_container(&sidecar_name, None).await.ok();
    }
    
    // Create sidecar container
    let listen_cmd = format!("TCP-LISTEN:{},fork", local_port);
    let connect_cmd = format!("TCP:{}:{}", container_ip, remote_port);
    let config = Config {
        image: Some("alpine/socat:latest".to_string()),
        cmd: Some(vec![listen_cmd, connect_cmd]),
        host_config: Some(bollard::models::HostConfig {
            network_mode: Some("host".to_string()),
            auto_remove: Some(true),
            ..Default::default()
        }),
        labels: Some(
            vec![
                ("krust.portforward".to_string(), "true".to_string()),
                ("krust.pod.namespace".to_string(), pod_namespace.to_string()),
                ("krust.pod.name".to_string(), pod_name.to_string()),
            ]
            .into_iter()
            .collect(),
        ),
        ..Default::default()
    };
    
    let options = CreateContainerOptions {
        name: sidecar_name.as_str(),
        ..Default::default()
    };
    
    docker.create_container(Some(options), config).await?;
    docker.start_container(&sidecar_name, None::<StartContainerOptions<String>>).await?;
    
    info!("Started sidecar container {} for port forwarding", sidecar_name);
    
    Ok(())
}