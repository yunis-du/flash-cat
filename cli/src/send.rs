use flash_cat_core::{FileStatus, TransferResults};
use std::{env, path::PathBuf, sync::Arc};

use anyhow::{Result, bail};
use tokio_stream::StreamExt;

use flash_cat_common::{Shutdown, proto::ClientType, utils::gen_share_code};
use flash_cat_core::{RelayType, SenderInteractionMessage, sender::FlashCatSender};

use crate::progress::Progress;

#[derive(Clone)]
pub struct Send {
    share_code: String,
    sender: FlashCatSender,
    relay: Option<String>,

    shutdown: Shutdown,
}

impl Send {
    pub async fn new(
        zip: bool,
        relay: Option<String>,
        files: Vec<String>,
        lan_broadcast: bool,
    ) -> Result<Self> {
        let files = files
            .into_iter()
            .map(|f| {
                if f == "." || f == "./" {
                    return env::current_dir().unwrap_or(PathBuf::from(".")).to_str().unwrap_or(".").to_string();
                }
                f
            })
            .collect::<Vec<_>>();
        let share_code = gen_share_code();
        let scanning = indicatif::ProgressBar::new_spinner();
        scanning.set_style(indicatif::ProgressStyle::with_template("{spinner} {msg}")?);
        scanning.enable_steady_tick(std::time::Duration::from_millis(100));
        scanning.set_message("Scanning files… (Ctrl+C to cancel)");
        let scan_progress = scanning.clone();
        let result = FlashCatSender::new_with_scan_progress(
            share_code.clone(),
            relay.clone(),
            files,
            zip,
            ClientType::Cli,
            lan_broadcast,
            move |progress| {
                scan_progress.set_message(format!(
                    "Scanning: {} files, {} folders, {} • Ctrl+C to cancel",
                    progress.files,
                    progress.folders,
                    indicatif::HumanBytes(progress.bytes)
                ));
            },
        )
        .await;
        scanning.finish_and_clear();
        let sender = result?;
        Ok(Self {
            share_code,
            sender,
            relay,
            shutdown: Shutdown::new(),
        })
    }

    pub async fn run(&self) -> Result<()> {
        let result = tokio::select! {
            result = self.run_inner() => result,
            _ = self.shutdown.wait() => Err(anyhow::anyhow!("Transfer cancelled")),
        };
        self.shutdown();
        result
    }

    async fn run_inner(&self) -> Result<()> {
        let file_collector = self.sender.get_file_collector();
        if file_collector.num_files == 1 {
            print!("Sending {} file ", file_collector.num_files);
        } else {
            print!("Sending {} files ", file_collector.num_files);
        }

        if file_collector.num_folders > 0 {
            if file_collector.num_files == 1 {
                print!("and {} folder ", file_collector.num_folders);
            } else {
                print!("and {} folders ", file_collector.num_folders);
            }
        }
        println!("({})", file_collector.total_size_to_human_readable());

        let mut results = TransferResults::default();
        let mut completed = false;
        let mut progress = Progress::new(
            file_collector.files.len() as u64,
            file_collector.max_file_name_length,
            file_collector.total_size,
        );
        let connecting = progress.add_spinner("Connecting to relay...");

        for file in file_collector.files.iter() {
            progress.register_file(&file.name, file.file_id, file.size);
        }

        match Arc::new(self.sender.clone()).start().await {
            Ok(mut stream) => {
                let mut share_code_printed = false;
                while !self.shutdown.is_terminated() {
                    if let Some(sender_msg) = stream.next().await {
                        match sender_msg {
                            SenderInteractionMessage::TransferMode(relay_type) => {
                                connecting.finish_and_clear();
                                progress.println(&format!(
                                    "\nReceiver connected • {}\n",
                                    Progress::transfer_mode_label(relay_type)
                                ));
                            }
                            SenderInteractionMessage::Message(msg) => progress.println(&msg),
                            SenderInteractionMessage::RelayConnected(relay_type) => {
                                let expected_relay = if self.relay.is_some() {
                                    RelayType::Specify
                                } else {
                                    RelayType::Public
                                };
                                if !share_code_printed && relay_type == expected_relay {
                                    share_code_printed = true;
                                    connecting.finish_and_clear();
                                    progress.println(&format!("Share code is: {}", self.share_code));
                                    progress.println("On the other computer run:");
                                    progress.println("");
                                    if let Some(relay) = &self.relay {
                                        progress.println(&format!("flash-cat recv {} --relay {}", self.share_code, relay));
                                    } else {
                                        progress.println(&format!("flash-cat recv {}", self.share_code));
                                    }
                                }
                            }
                            SenderInteractionMessage::Error(e) => {
                                connecting.finish_and_clear();
                                bail!("{e}; {}", results.summary(file_collector.files.len() as u64));
                            }
                            SenderInteractionMessage::ReceiverReject => {
                                bail!("Receiver rejected the transfer");
                            }
                            SenderInteractionMessage::RelayFailed((relay_type, error)) => {
                                connecting.finish_and_clear();
                                if RelayType::Local.eq(&relay_type) || RelayType::Specify.eq(&relay_type) {
                                    bail!("Could not connect to {} relay: {error}", relay_type.to_string());
                                } else {
                                    progress.println(&format!("connect to {} relay failed: {}", relay_type.to_string(), error));
                                }
                            }
                            SenderInteractionMessage::FileStage(stage) => progress.set_stage(stage),
                            SenderInteractionMessage::FileProgress(file_progress) => {
                                progress.set_position(file_progress.file_id, file_progress.position);
                            }
                            SenderInteractionMessage::FileResult(result) => {
                                if results.record(result.clone()) {
                                    match result.status() {
                                        FileStatus::Success => progress.finish(result.file_id),
                                        FileStatus::Skipped => progress.skip(result.file_id),
                                        FileStatus::Failed => progress.finish_with_message(result.file_id, format!("Failed: {}", result.error)),
                                    }
                                }
                            }
                            SenderInteractionMessage::OtherClose => {
                                bail!("Receiver disconnected: {}", results.summary(file_collector.files.len() as u64));
                            }
                            SenderInteractionMessage::ReconnectFailed(error) => {
                                bail!(
                                    "Reconnect failed: {error}; {}",
                                    results.summary(file_collector.files.len() as u64)
                                );
                            }
                            SenderInteractionMessage::SendDone => {
                                // progress.println("Send files done. waiting for the receiver to receive finish...");
                            }
                            SenderInteractionMessage::Completed => {
                                completed = true;
                                progress.println(&results.summary(file_collector.files.len() as u64));
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                self.shutdown();
                            }
                        }
                    } else {
                        break;
                    }
                }
            }
            Err(e) => {
                connecting.finish_and_clear();
                self.shutdown();
                return Err(e);
            }
        }

        connecting.finish_and_clear();
        if !completed || results.failed > 0 || results.remaining(file_collector.files.len() as u64) > 0 {
            bail!("Transfer incomplete: {}", results.summary(file_collector.files.len() as u64));
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        self.sender.shutdown();
        self.shutdown.shutdown();
    }

    pub async fn terminated(&self) {
        self.shutdown.wait().await
    }

    pub async fn shutdown_complete(&self) {
        self.sender.shutdown_complete().await
    }
}
