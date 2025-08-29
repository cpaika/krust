// Common test utilities and helpers

use std::sync::Once;

static INIT: Once = Once::new();
static mut SERVER_AVAILABLE: bool = false;

pub async fn check_server() -> bool {
    INIT.call_once(|| {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = runtime.block_on(async {
            reqwest::get("http://localhost:6443/livez").await.is_ok()
        });
        unsafe {
            SERVER_AVAILABLE = result;
        }
        if !result {
            eprintln!("⚠️  Warning: Server not running at localhost:6443");
            eprintln!("   Integration tests will be skipped");
            eprintln!("   Start the server with: cargo run");
        }
    });
    unsafe { SERVER_AVAILABLE }
}

#[macro_export]
macro_rules! require_server {
    () => {
        if !crate::common::check_server().await {
            eprintln!("Skipping test - server not running");
            return;
        }
    };
}