use std::{future::Future, net::SocketAddr};

use anyhow::Result;
use log::info;

pub mod grpc;
pub mod listen;
pub mod relay;
pub mod session;

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

pub async fn run_relay(addr: SocketAddr, signal: impl Future<Output = ()>) -> Result<()> {
    let relay = relay::Relay::new()?;

    let relay_task = async {
        info!("relay listening at {addr}");
        relay.bind(addr).await
    };

    let signals_task = async {
        tokio::select! {
            () = signal => (),
            else => return Ok(()),
        }
        info!("gracefully shutting down...");
        // Waiting done message send to the right end.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        relay.shutdown();
        Ok(())
    };

    tokio::try_join!(relay_task, signals_task)?;
    Ok(())
}
