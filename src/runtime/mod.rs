pub mod container;
pub mod container_runtime;
pub mod cgroups;
pub mod kubelet;

use anyhow::Result;
use bollard::Docker;

pub use kubelet::Kubelet;

pub struct ContainerRuntime {
    docker: Docker,
}

impl ContainerRuntime {
    pub async fn new() -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self { docker })
    }

    pub fn docker(&self) -> &Docker {
        &self.docker
    }
}