use flash_cat_core::{FileStatus, TransferResults};
use std::{sync::Arc, time::Duration};

use anyhow::{Result, anyhow, bail};
use indicatif::HumanBytes;
use tokio_stream::StreamExt;

use flash_cat_common::{Shutdown, proto::ClientType};
use flash_cat_core::{ReceiverConfirm, ReceiverInteractionMessage, receiver::FlashCatReceiver};

use crate::{
    progress::Progress,
    prompt::{self, ExistingAction},
};

// Clear the spinner on errors and cancellation as well as normal completion.
#[derive(Default)]
struct ResumeVerification(Option<(u64, indicatif::ProgressBar)>);

impl ResumeVerification {
    fn finish(
        &mut self,
        file_id: u64,
    ) {
        if self.0.as_ref().is_some_and(|(id, _)| *id == file_id) {
            if let Some((_, spinner)) = self.0.take() {
                spinner.finish_and_clear();
            }
        }
    }
}

impl Drop for ResumeVerification {
    fn drop(&mut self) {
        if let Some((_, spinner)) = self.0.take() {
            spinner.finish_and_clear();
        }
    }
}

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
        let receiver = FlashCatReceiver::new(share_code, specify_relay, output, ClientType::Cli, lan)?;
        Ok(Self {
            receiver,
            assumeyes,
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
        let mut stream = Arc::new(self.receiver.clone()).start().await.map_err(|e| {
            self.shutdown();
            if e.to_string().contains("NotFound") {
                anyhow!("Not found, Please check share code.")
            } else {
                anyhow!(format!("An error occurred: {}", e.to_string()))
            }
        })?;
        let mut results = TransferResults::default();
        let mut completed = false;
        let mut progress = Progress::new(1, 10, 0);
        let mut total_entries = 0;
        let mut transfer_mode = None;
        let mut conflict_action = None;
        let mut verifying = ResumeVerification::default();
        while !self.shutdown.is_terminated() {
            if let Some(receiver_msg) = stream.next().await {
                match receiver_msg {
                    ReceiverInteractionMessage::TransferMode(relay_type) => {
                        transfer_mode = Some(Progress::transfer_mode_label(relay_type));
                    }
                    ReceiverInteractionMessage::Message(msg) => progress.println(&msg),
                    ReceiverInteractionMessage::Error(e) => {
                        bail!("{e}; {}", results.summary(total_entries));
                    }
                    ReceiverInteractionMessage::SendFilesRequest(send_req) => {
                        total_entries = send_req.num_entries;
                        let files_label = if send_req.num_files == 1 {
                            "file"
                        } else {
                            "files"
                        };
                        print!("Receiving {} {files_label}", send_req.num_files);
                        if send_req.num_folders > 0 {
                            let folders_label = if send_req.num_folders == 1 {
                                "folder"
                            } else {
                                "folders"
                            };
                            print!(" and {} {folders_label}", send_req.num_folders);
                        }
                        print!(" • {}", HumanBytes(send_req.total_size));
                        if let Some(mode) = transfer_mode {
                            print!(" • {mode}");
                        }
                        println!();
                        if self.assumeyes {
                            progress.update(
                                send_req.num_entries,
                                send_req.max_file_name_length as usize,
                                send_req.total_size,
                            );
                            self.receiver.send_confirm(ReceiverConfirm::ReceiveConfirm(true)).await?;
                            continue;
                        }
                        if prompt::confirm("Accept transfer?").await? {
                            progress.update(
                                send_req.num_entries,
                                send_req.max_file_name_length as usize,
                                send_req.total_size,
                            );
                            self.receiver.send_confirm(ReceiverConfirm::ReceiveConfirm(true)).await?;
                        } else {
                            self.receiver.send_confirm(ReceiverConfirm::ReceiveConfirm(false)).await?;
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            bail!("Transfer declined");
                        }
                    }
                    ReceiverInteractionMessage::FileDuplication(file) => {
                        let action = self.existing_action(&file.path, &mut conflict_action, &progress).await?;
                        self.resolve_file(file.file_id, action).await?;
                    }
                    ReceiverInteractionMessage::FileRenamed(_) => {}
                    ReceiverInteractionMessage::RecvNewFile(recv_new_file) => {
                        progress.add_progress(recv_new_file.filename.as_str(), recv_new_file.file_id, recv_new_file.size);
                    }
                    ReceiverInteractionMessage::BreakPoint(file) => {
                        let resume = if self.assumeyes {
                            true
                        } else {
                            let _display = progress.pause_for_prompt();
                            prompt::confirm_transient(&format!(
                                "File '{}' is {:.2}% complete. Resume transfer? (n restarts from zero)",
                                file.filename, file.percent,
                            ))
                            .await?
                        };
                        if resume {
                            verifying = ResumeVerification(Some((
                                file.file_id,
                                progress.add_spinner("Verifying existing file before resuming..."),
                            )));
                        }
                        self.receiver.send_confirm(ReceiverConfirm::BreakPointConfirm((resume, file.file_id, file.position))).await?;
                    }
                    ReceiverInteractionMessage::FileStage(stage) => {
                        verifying.finish(stage.file_id);
                        progress.set_stage(stage);
                    }
                    ReceiverInteractionMessage::FileProgress(fp) => {
                        progress.set_position(fp.file_id, fp.position);
                    }
                    ReceiverInteractionMessage::FileResult(result) => {
                        verifying.finish(result.file_id);
                        if results.record(result.clone()) {
                            match result.status() {
                                FileStatus::Success => progress.finish(result.file_id),
                                FileStatus::Skipped => progress.skip(result.file_id),
                                FileStatus::Failed => progress.finish_with_message(result.file_id, format!("Failed: {}", result.error)),
                            }
                        }
                    }
                    ReceiverInteractionMessage::OtherClose => {
                        bail!("Sender disconnected: {}", results.summary(total_entries));
                    }
                    ReceiverInteractionMessage::ReconnectFailed(error) => {
                        bail!("Reconnect failed: {error}; {}", results.summary(total_entries));
                    }
                    ReceiverInteractionMessage::ReceiveDone => {
                        completed = true;
                        progress.println(&results.summary(total_entries));
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        self.shutdown();
                    }
                }
            } else {
                break;
            }
        }
        if !completed || results.failed > 0 || results.remaining(total_entries) > 0 {
            bail!("Transfer incomplete: {}", results.summary(total_entries));
        }
        Ok(())
    }

    async fn existing_action(
        &self,
        path: &str,
        remembered: &mut Option<ExistingAction>,
        progress: &Progress,
    ) -> Result<ExistingAction> {
        if self.assumeyes {
            return Ok(ExistingAction::Overwrite);
        }
        if let Some(action) = *remembered {
            return Ok(action);
        }
        let _display = progress.pause_for_prompt();
        let action = prompt::existing(path).await?;
        *remembered = Some(action);
        Ok(action)
    }

    async fn resolve_file(
        &self,
        file_id: u64,
        action: ExistingAction,
    ) -> Result<()> {
        let confirm = match action {
            ExistingAction::Overwrite => ReceiverConfirm::FileConfirm((true, file_id)),
            ExistingAction::Skip => ReceiverConfirm::FileConfirm((false, file_id)),
            ExistingAction::Rename => ReceiverConfirm::RenameFile(file_id),
        };
        self.receiver.send_confirm(confirm).await
    }

    pub fn shutdown(&self) {
        self.receiver.shutdown();
        self.shutdown.shutdown();
    }

    pub async fn terminated(&self) {
        self.shutdown.wait().await
    }

    pub async fn shutdown_complete(&self) {
        self.receiver.shutdown_complete().await;
    }
}
