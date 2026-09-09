use std::{future::Future, sync::OnceLock};

use anyhow::{Result, anyhow};
use tokio::runtime::Runtime;

/// All transfers and confirmations use one runtime owned by the application.
/// A cancelled view can stop its transfer without destroying unrelated tasks.
pub fn spawn_transfer(future: impl Future<Output = ()> + Send + 'static) -> Result<()> {
    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().map_err(|e| e.to_string()));
    let runtime = runtime.as_ref().map_err(|e| anyhow!("Cannot start transfer runtime: {e}"))?;
    runtime.spawn(future);
    Ok(())
}
