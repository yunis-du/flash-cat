use std::process::ExitCode;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use flash_cat_cli::{built_info, receive::Receive, send::Send};
use flash_cat_common::{utils::fs::is_file, VersionInfo, CLI_VERSION};
#[cfg(windows)]
use tokio::signal::ctrl_c;
#[cfg(unix)]
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

    /// Relay address (default: public relay [https://flashcat.duyunzhi.cn])
    #[clap(long)]
    relay: Option<String>,

    /// File(s) or folder(s) to send
    #[clap(required = true, num_args = 1..)]
    files: Vec<String>,
}

#[derive(Parser, Debug)]
struct RecvCmd {
    /// Share code of receive
    #[clap(required = true, num_args = 1)]
    share_code: String,

    /// Relay address (default: public relay [https://flashcat.duyunzhi.cn])
    #[clap(long)]
    relay: Option<String>,

    /// The save path of the received file(s) or folder(s)
    #[clap(short = 'o', long)]
    output: Option<String>,

    /// Automatically answer yes for all questions
    #[clap(short = 'y', long)]
    assumeyes: bool,
}

const VERSION_INFO: &'static VersionInfo = &VersionInfo {
    name: "Flash-Cat-CLI",
    version: CLI_VERSION,
    commit_hash: built_info::GIT_COMMIT_HASH,
    build_time: built_info::BUILT_TIME_UTC,
};

#[tokio::main]
async fn send(send_cmd: SendCmd) -> Result<()> {
    #[cfg(unix)]
    let mut sigterm = signal(SignalKind::terminate())?;
    #[cfg(unix)]
    let mut sigint = signal(SignalKind::interrupt())?;
    #[cfg(windows)]
    let sigint = ctrl_c();

    let send = Send::new(send_cmd.zip, send_cmd.relay, send_cmd.files).await?;

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
            return Err(anyhow::Error::msg("The output path is a file."));
        }
    }

    let receive = Receive::new(
        recv_cmd.share_code,
        recv_cmd.relay,
        recv_cmd.output,
        recv_cmd.assumeyes,
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
    } else {
        Cmd::command().print_help().unwrap();
    }
    ExitCode::SUCCESS
}
