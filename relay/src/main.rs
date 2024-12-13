use std::net::{IpAddr, SocketAddr};

use anyhow::Result;
use clap::Parser;
use flash_cat_common::{init_logger, VersionInfo, RELAY_VERSION};
use flash_cat_relay::{built_info, relay::Relay};
use log::{error, info};
#[cfg(windows)]
use tokio::signal::ctrl_c;
#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};

#[derive(Parser, Debug)]
#[clap(name = "flash-cat-relay")]
struct Cmd {
    /// Which IP address or network interface to listen on.
    #[clap(long, value_parser, default_value = "0.0.0.0")]
    ip: IpAddr,

    /// Relay port
    #[clap(short = 'p', long, default_value = "6880")]
    port: u16,

    /// External access address.
    #[clap(long, value_parser)]
    external_ip: Option<IpAddr>,

    /// Log file path of the relay server.
    #[clap(
        long,
        default_value = "/var/log/flash_cat_relay/flash-cat-relay.log",
        env = "RELAY_LOG_PATH"
    )]
    log_file: String,

    /// Log file path of the relay server.
    #[clap(long, default_value = "info", env = "RUST_LOG")]
    log_level: String,

    /// Display the relay version
    #[clap(short, long)]
    version: bool,
}

const VERSION_INFO: &'static VersionInfo = &VersionInfo {
    name: "Flash-Cat-Relay",
    version: RELAY_VERSION,
    commit_hash: built_info::GIT_COMMIT_HASH,
    build_time: built_info::BUILT_TIME_UTC,
};

#[tokio::main]
async fn start(addr: SocketAddr, external_ip: Option<IpAddr>) -> Result<()> {
    #[cfg(unix)]
    let mut sigterm = signal(SignalKind::terminate())?;
    #[cfg(unix)]
    let mut sigint = signal(SignalKind::interrupt())?;
    #[cfg(windows)]
    let sigint = ctrl_c();

    let relay = Relay::new(external_ip)?;

    let relay_task = async {
        info!("relay listening at {addr}");
        relay.bind(addr).await
    };

    #[cfg(unix)]
    let signals_task = async {
        tokio::select! {
            Some(()) = sigterm.recv() => (),
            Some(()) = sigint.recv() => (),
            else => return Ok(()),
        }
        info!("gracefully shutting down...");
        relay.shutdown();
        Ok(())
    };

    #[cfg(windows)]
    let signals_task = async {
        tokio::select! {
            Ok(()) = sigint => (),
            else => return Ok(()),
        }
        info!("gracefully shutting down...");
        relay.shutdown();
        Ok(())
    };

    tokio::try_join!(relay_task, signals_task)?;
    Ok(())
}

fn main() -> Result<()> {
    let cmd = Cmd::parse();

    if cmd.version {
        println!("{}", VERSION_INFO);
        return Ok(());
    }
    init_logger(cmd.log_level, cmd.log_file);
    let addr = SocketAddr::new(cmd.ip, cmd.port);
    match start(addr, cmd.external_ip) {
        Ok(()) => Ok(()),
        Err(err) => {
            error!("{err:?}");
            Err(err)
        }
    }
}
