use crate::Storage;

pub struct ControllerManager {
    storage: Storage,
}

impl ControllerManager {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub async fn run(&self) {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }
}