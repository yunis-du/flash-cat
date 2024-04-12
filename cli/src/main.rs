use std::{net::SocketAddr, process::ExitCode};

use anyhow::Result;
use clap::{Parser, Subcommand};
use flash_cat_cli::{receive::Receive, send::Send};
use flash_cat_common::{VersionInfo, CLI_VERSION};
use tokio::signal::unix::{signal, SignalKind};

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
}

#[derive(Parser, Debug)]
struct SendCmd {
    /// Zip folder before sending
    #[clap(long)]
    zip: bool,

    /// Relay address (default: public relay)
    #[clap(long, value_parser)]
    relay: Option<SocketAddr>,

    /// File(s) or folder(s) to send
    #[clap(required = true, num_args = 1..)]
    files: Vec<String>,
}

#[derive(Parser, Debug)]
struct RecvCmd {
    /// Share code of receive
    #[clap(required = true, num_args = 1)]
    share_code: String,

    /// Relay address (default: public relay)
    #[clap(long, value_parser)]
    relay: Option<SocketAddr>,
}

const VERSION_INFO: &'static VersionInfo = &VersionInfo {
    name: "Flash-Cat-CLI",
    version: CLI_VERSION,
};

#[tokio::main]
async fn send(send_cmd: SendCmd) -> Result<()> {
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    let send = Send::new(send_cmd.zip, send_cmd.relay, send_cmd.files).await?;

    let send_task = async { send.run().await };

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

    tokio::try_join!(send_task, signals_task)?;
    Ok(())
}

#[tokio::main]
async fn recv(recv_cmd: RecvCmd) -> Result<()> {
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    let receive = Receive::new(recv_cmd.share_code, recv_cmd.relay)?;

    let receive_task = async { receive.run().await };

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

    tokio::try_join!(receive_task, signals_task)?;
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
                        println!("{err}");
                        ExitCode::FAILURE
                    }
                };
            }
            SubCmd::Recv(recv_cmd) => {
                return match recv(recv_cmd) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(err) => {
                        println!("{err}");
                        ExitCode::FAILURE
                    }
                };
            }
        }
    }
    ExitCode::SUCCESS
}
