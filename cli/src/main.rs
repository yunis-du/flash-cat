use std::{
    net::{IpAddr, SocketAddr},
    process::ExitCode,
};

use anyhow::{Result, bail};
use clap::{ArgAction, CommandFactory, Parser, Subcommand};
use log::info;
#[cfg(windows)]
use tokio::signal::ctrl_c;
#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal};

use flash_cat_cli::{built_info, receive::Receive, send::Send, update};
use flash_cat_common::{VersionInfo, init_logger, utils::fs::is_file};
use flash_cat_relay::relay::Relay;

#[derive(Parser, Debug)]
#[clap(name = "flash-cat-cli")]
struct Cmd {
    #[clap(subcommand)]
    sub_cmd: Option<SubCmd>,

    /// Display the relay version
    #[clap(short, long)]
    version: bool,
}

#[derive(Subcommand, Debug)]
enum SubCmd {
    /// Send file(s) or folder(s)
    Send(SendCmd),
    /// Receive file(s) or folder(s)
    Recv(RecvCmd),
    /// Start relay server
    Relay(RelayCmd),
    /// Update to the latest version
    Update,
}

#[derive(Parser, Debug)]
struct SendCmd {
    /// Zip folder before sending
    #[clap(long)]
    zip: bool,

    /// Relay address (default: public relay [https://flashcat.yunisdu.com])
    #[clap(long, env = "FLASH_CAT_RELAY")]
    relay: Option<String>,

    /// Disable LAN discovery while sending
    #[clap(long = "no-lan", action = ArgAction::SetFalse, default_value_t = true)]
    lan_broadcast: bool,

    /// File(s) or folder(s) to send
    #[clap(required = true, num_args = 1..)]
    files: Vec<String>,
}

#[derive(Parser, Debug)]
struct RecvCmd {
    /// Share code of receive
    #[clap(required = true, num_args = 1)]
    share_code: String,

    /// Relay address (default: public relay [https://flashcat.yunisdu.com])
    #[clap(long)]
    relay: Option<String>,

    /// The save path of the received file(s) or folder(s)
    #[clap(short = 'o', long)]
    output: Option<String>,

    /// Automatically answer yes for all questions
    #[clap(short = 'y', long)]
    assumeyes: bool,

    /// Sender is in the same local area network
    #[clap(short, long)]
    lan: bool,
}

#[derive(Parser, Debug)]
struct RelayCmd {
    /// Which IP address or network interface to listen on.
    #[clap(long, value_parser, default_value = "0.0.0.0")]
    ip: IpAddr,

    /// Relay port
    #[clap(short = 'p', long, default_value = "6880")]
    port: u16,

    /// External access address.
    #[clap(long, value_parser)]
    external_ip: Option<IpAddr>,

    /// Forward session setup to this relay; clients transfer data directly through it.
    #[clap(long, value_parser, conflicts_with = "external_ip")]
    forward: Option<SocketAddr>,

    /// Log file path of the relay server.
    #[clap(long, default_value = "flash-cat-relay.log", env = "FLASH_CAT_RELAY_LOG_PATH")]
    log_file: String,

    /// Log file path of the relay server.
    #[clap(long, default_value = "info", env = "RUST_LOG")]
    log_level: String,
}

const VERSION_INFO: &'static VersionInfo = &VersionInfo {
    name: "Flash-Cat",
    version: built_info::PKG_VERSION,
    commit_hash: built_info::GIT_COMMIT_HASH,
    build_time: built_info::BUILT_TIME_UTC,
};

#[tokio::main]
async fn update() -> Result<()> {
    update::update().await
}

#[tokio::main]
async fn send(send_cmd: SendCmd) -> Result<()> {
    #[cfg(unix)]
    let mut sigterm = signal(SignalKind::terminate())?;
    #[cfg(unix)]
    let mut sigint = signal(SignalKind::interrupt())?;
    #[cfg(windows)]
    let sigint = ctrl_c();

    let send = Send::new(send_cmd.zip, send_cmd.relay, send_cmd.files, send_cmd.lan_broadcast).await?;

    let send_task = async { send.run().await };

    #[cfg(unix)]
    let signals_task = async {
        tokio::select! {
            Some(()) = sigterm.recv() => (),
            Some(()) = sigint.recv() => (),
            _ = send.terminated() => return Ok(()),
            else => return Ok(()),
        }
        send.shutdown();
        Ok(())
    };

    #[cfg(windows)]
    let signals_task = async {
        tokio::select! {
            Ok(()) = sigint => (),
            _ = send.terminated() => return Ok(()),
            else => return Ok(()),
        }
        send.shutdown();
        Ok(())
    };

    tokio::try_join!(send_task, signals_task)?;
    // Ensure that the channel is closed
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    Ok(())
}

#[tokio::main]
async fn recv(recv_cmd: RecvCmd) -> Result<()> {
    #[cfg(unix)]
    let mut sigterm = signal(SignalKind::terminate())?;
    #[cfg(unix)]
    let mut sigint = signal(SignalKind::interrupt())?;
    #[cfg(windows)]
    let sigint = ctrl_c();

    if recv_cmd.output.is_some() {
        if is_file(recv_cmd.output.clone().unwrap().as_str()) {
            bail!("The output path is a file.");
        }
    }

    let receive = Receive::new(
        recv_cmd.share_code,
        recv_cmd.relay,
        recv_cmd.output,
        recv_cmd.assumeyes,
        recv_cmd.lan,
    )?;

    let receive_task = async { receive.run().await };

    #[cfg(unix)]
    let signals_task = async {
        tokio::select! {
            Some(()) = sigterm.recv() => (),
            Some(()) = sigint.recv() => (),
            _ = receive.terminated() => return Ok(()),
            else => return Ok(()),
        }
        receive.shutdown();
        Ok(())
    };

    #[cfg(windows)]
    let signals_task = async {
        tokio::select! {
            Ok(()) = sigint => (),
            _ = receive.terminated() => return Ok(()),
            else => return Ok(()),
        }
        receive.shutdown();
        Ok(())
    };

    tokio::try_join!(receive_task, signals_task)?;
    // Ensure that the channel is closed
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    Ok(())
}

#[tokio::main]
async fn start_relay(
    addr: SocketAddr,
    external_ip: Option<IpAddr>,
    forward: Option<SocketAddr>,
) -> Result<()> {
    #[cfg(unix)]
    let mut sigterm = signal(SignalKind::terminate())?;
    #[cfg(unix)]
    let mut sigint = signal(SignalKind::interrupt())?;
    #[cfg(windows)]
    let sigint = ctrl_c();

    if forward == Some(addr) {
        bail!("forward relay must not be the relay's own listening address");
    }
    let relay = Relay::new_with_forward(external_ip, false, forward)?;

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

fn main() -> ExitCode {
    let cmd = Cmd::parse();

    if cmd.version {
        println!("{}", VERSION_INFO);
        return ExitCode::SUCCESS;
    }

    if cmd.sub_cmd.is_some() {
        match cmd.sub_cmd.unwrap() {
            SubCmd::Send(send_cmd) => {
                return match send(send_cmd) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(err) => {
                        println!("{err:?}");
                        ExitCode::FAILURE
                    }
                };
            }
            SubCmd::Recv(recv_cmd) => {
                return match recv(recv_cmd) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(err) => {
                        println!("{err:?}");
                        ExitCode::FAILURE
                    }
                };
            }
            SubCmd::Relay(relay_cmd) => {
                init_logger(relay_cmd.log_level, relay_cmd.log_file);
                let addr = SocketAddr::new(relay_cmd.ip, relay_cmd.port);
                return match start_relay(addr, relay_cmd.external_ip, relay_cmd.forward) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(err) => {
                        println!("{err:?}");
                        ExitCode::FAILURE
                    }
                };
            }
            SubCmd::Update => {
                return match update() {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(err) => {
                        println!("{err:?}");
                        ExitCode::FAILURE
                    }
                };
            }
        }
    } else {
        Cmd::command().print_help().unwrap();
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_forward_relay() {
        let command = Cmd::try_parse_from(["flash-cat", "relay", "--forward", "203.0.113.10:6880"]).unwrap();
        let Some(SubCmd::Relay(relay)) = command.sub_cmd else {
            panic!("relay command was not parsed");
        };
        assert_eq!(relay.forward, Some("203.0.113.10:6880".parse().unwrap()));
    }

    #[test]
    fn forward_relay_conflicts_with_external_ip() {
        let result = Cmd::try_parse_from(["flash-cat", "relay", "--external-ip", "198.51.100.20", "--forward", "203.0.113.10:6880"]);
        assert!(result.is_err());
    }
}
