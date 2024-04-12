use anyhow::Result;
use clap::Parser;
use flash_cat_common::{init_logger, VersionInfo, RELAY_VERSION};
use flash_cat_relay::relay::Relay;
use log::{error, info};
use std::net::{IpAddr, SocketAddr};
use tokio::signal::unix::{signal, SignalKind};

#[derive(Parser, Debug)]
#[clap(name = "flash-cat-relay")]
struct Cmd {
    /// Which IP address or network interface to listen on.
    #[clap(long, value_parser, default_value = "::1")]
    listen: IpAddr,

    /// Relay port
    #[clap(short = 'p', long, default_value = "6880")]
    port: u16,

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
};

#[tokio::main]
async fn start(addr: SocketAddr) -> Result<()> {
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    let relay = Relay::new()?;

    let relay_task = async {
        info!("relay listening at {addr}");
        relay.bind(addr).await
    };

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
    let addr = SocketAddr::new(cmd.listen, cmd.port);
    match start(addr) {
        Ok(()) => Ok(()),
        Err(err) => {
            error!("{err:?}");
            Err(err)
        }
    }
}
