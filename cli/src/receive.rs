use std::{
    io::{Write, stdin, stdout},
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use indicatif::HumanBytes;
use tokio_stream::StreamExt;

use flash_cat_common::{Shutdown, proto::ClientType};
use flash_cat_core::{ReceiverConfirm, ReceiverInteractionMessage, receiver::FlashCatReceiver};

use crate::progress::Progress;

#[derive(Clone)]
pub struct Receive {
    receiver: FlashCatReceiver,
    assumeyes: bool,

    shutdown: Shutdown,
}

impl Receive {
    pub fn new(
        share_code: String,
        specify_relay: Option<String>,
        output: Option<String>,
        assumeyes: bool,
        lan: bool,
    ) -> Result<Self> {
        let receiver =
            FlashCatReceiver::new(share_code, specify_relay, output, ClientType::Cli, lan)?;
        Ok(Self {
            receiver,
            assumeyes,
            shutdown: Shutdown::new(),
        })
    }

    pub async fn run(&self) -> Result<()> {
        let mut stream = Arc::new(self.receiver.clone()).start().await.map_err(|e| {
            if e.to_string().contains("NotFound") {
                anyhow::Error::msg("Not found, Please check share code.")
            } else {
                anyhow::Error::msg(format!("An error occurred: {}", e.to_string()))
            }
        })?;
        let mut progress = Progress::new(1, 10);
        while !self.shutdown.is_terminated() {
            if let Some(receiver_msg) = stream.next().await {
                match receiver_msg {
                    ReceiverInteractionMessage::Message(msg) => println!("{msg}"),
                    ReceiverInteractionMessage::Error(e) => {
                        println!("An error occurred: {}", e.to_string());
                        self.shutdown();
                    }
                    ReceiverInteractionMessage::SendFilesRequest(send_req) => {
                        print!("Receiving {} files", send_req.num_files);
                        if send_req.num_folders > 0 {
                            print!(" and {} folders", send_req.num_folders);
                        }
                        if self.assumeyes {
                            println!();
                            progress
                                .update(send_req.num_files, send_req.max_file_name_length as usize);
                            self.receiver
                                .send_confirm(ReceiverConfirm::ReceiveConfirm(true))
                                .await?;
                            continue;
                        }
                        print!(" ({})? (Y/n) ", HumanBytes(send_req.total_size).to_string());
                        stdout().flush()?;
                        let mut input = String::new();
                        stdin().read_line(&mut input)?;
                        let input = input.trim();
                        if input.to_lowercase() == "y" || input.to_lowercase() == "yes" {
                            progress
                                .update(send_req.num_files, send_req.max_file_name_length as usize);
                            self.receiver
                                .send_confirm(ReceiverConfirm::ReceiveConfirm(true))
                                .await?;
                        } else {
                            self.receiver
                                .send_confirm(ReceiverConfirm::ReceiveConfirm(false))
                                .await?;
                            self.shutdown();
                            println!("Refuse to receive, exit...");
                            tokio::time::sleep(Duration::from_millis(200)).await;
                        }
                    }
                    ReceiverInteractionMessage::FileDuplication(file_duplication) => {
                        if self.assumeyes {
                            self.receiver
                                .send_confirm(ReceiverConfirm::FileConfirm((
                                    true,
                                    file_duplication.file_id,
                                )))
                                .await?;
                            continue;
                        }
                        print!("overwrite '{}'? (Y/n) ", file_duplication.path);
                        stdout().flush()?;
                        let mut input = String::new();
                        stdin().read_line(&mut input)?;
                        let input = input.trim();
                        if input.to_lowercase() == "y" || input.to_lowercase() == "yes" {
                            self.receiver
                                .send_confirm(ReceiverConfirm::FileConfirm((
                                    true,
                                    file_duplication.file_id,
                                )))
                                .await?;
                        } else {
                            progress.skip(file_duplication.file_id);
                            self.receiver
                                .send_confirm(ReceiverConfirm::FileConfirm((
                                    false,
                                    file_duplication.file_id,
                                )))
                                .await?;
                        }
                    }
                    ReceiverInteractionMessage::RecvNewFile(recv_new_file) => {
                        progress.add_progress(
                            recv_new_file.filename.as_str(),
                            recv_new_file.file_id,
                            recv_new_file.size,
                        );
                    }
                    ReceiverInteractionMessage::BreakPoint(break_point) => {
                        print!(
                            "File '{}' is {:.2}% complete. Resume transfer? (Y/n) ",
                            break_point.filename, break_point.percent
                        );
                        stdout().flush()?;
                        let mut input = String::new();
                        stdin().read_line(&mut input)?;
                        let input = input.trim();
                        if input.to_lowercase() == "y" || input.to_lowercase() == "yes" {
                            self.receiver
                                .send_confirm(ReceiverConfirm::BreakPointConfirm((
                                    true,
                                    break_point.file_id,
                                    break_point.position,
                                )))
                                .await?;
                        } else {
                            self.receiver
                                .send_confirm(ReceiverConfirm::BreakPointConfirm((
                                    false,
                                    break_point.file_id,
                                    break_point.position,
                                )))
                                .await?;
                        }
                    }
                    ReceiverInteractionMessage::FileProgress(fp) => {
                        progress.set_position(fp.file_id, fp.position);
                    }
                    ReceiverInteractionMessage::FileProgressFinish(file_id) => {
                        progress.finish(file_id);
                    }
                    ReceiverInteractionMessage::OtherClose => {
                        println!("The send end is interrupted. exit...");
                        self.shutdown();
                    }
                    ReceiverInteractionMessage::ReceiveDone => {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        self.shutdown();
                    }
                }
            }
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        self.receiver.shutdown();
        self.shutdown.shutdown();
    }

    pub async fn terminated(&self) {
        self.shutdown.wait().await
    }
}
