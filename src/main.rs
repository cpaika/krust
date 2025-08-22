use anyhow::Result;
use krust::{
    api::server::start_server, 
    controllers::{
        deployment_controller::DeploymentController,
        endpoints_controller::EndpointsController,
        replicaset_controller::ReplicaSetController,
    },
    runtime::Kubelet, 
    scheduler::Scheduler, 
    Storage
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "krust=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Krust - Kubernetes in Rust");

    let database_url = "sqlite:krust.db?mode=rwc";
    let storage = Storage::new(database_url).await?;
    
    tracing::info!("Running database migrations");
    storage.migrate().await?;
    
    // Start scheduler in background
    let scheduler = Scheduler::new(storage.clone());
    tokio::spawn(async move {
        if let Err(e) = scheduler.run().await {
            tracing::error!("Scheduler failed: {}", e);
        }
    });
    
    // Start kubelet in background
    match Kubelet::new(storage.clone()).await {
        Ok(kubelet) => {
            tokio::spawn(async move {
                if let Err(e) = kubelet.run().await {
                    tracing::error!("Kubelet failed: {}", e);
                }
            });
        }
        Err(e) => {
            tracing::warn!("Failed to start kubelet (Docker may not be available): {}", e);
        }
    }
    
    // Start endpoints controller in background
    let endpoints_controller = EndpointsController::new(storage.clone());
    tokio::spawn(async move {
        if let Err(e) = endpoints_controller.run().await {
            tracing::error!("Endpoints controller failed: {}", e);
        }
    });
    
    // Start deployment controller in background
    let deployment_controller = DeploymentController::new(storage.clone());
    tokio::spawn(async move {
        if let Err(e) = deployment_controller.run().await {
            tracing::error!("Deployment controller failed: {}", e);
        }
    });
    
    // Start replicaset controller in background
    let replicaset_controller = ReplicaSetController::new(storage.clone());
    tokio::spawn(async move {
        if let Err(e) = replicaset_controller.run().await {
            tracing::error!("ReplicaSet controller failed: {}", e);
        }
    });
    
    tracing::info!("Starting API server on port 6443");
    start_server(storage).await?;

    Ok(())
}
