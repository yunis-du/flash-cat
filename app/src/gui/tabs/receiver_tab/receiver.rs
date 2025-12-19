use std::{
    hash::Hash,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicU64, Ordering},
    },
};

use iced::{
    Subscription,
    futures::{SinkExt, Stream, StreamExt, channel::mpsc},
    stream::try_channel,
};
use rust_i18n::t;

use flash_cat_core::{ReceiverInteractionMessage, receiver::FlashCatReceiver};

use super::{RECEIVER_NOTIFICATION, RECEIVER_STATE, ReceiverNotification, ReceiverState};
use crate::gui::tabs::receiver_tab::ConfirmType;

pub(super) static RECV_NUM_FILES: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));

pub struct FlashCatReceiverWrapper(pub Arc<FlashCatReceiver>);

impl Hash for FlashCatReceiverWrapper {
    fn hash<H: std::hash::Hasher>(
        &self,
        state: &mut H,
    ) {
        let ptr = Arc::as_ptr(&self.0) as *const ();
        ptr.hash(state);
    }
}

pub fn recv(fcr: FlashCatReceiverWrapper) -> Subscription<Result<(u64, Progress), Error>> {
    Subscription::run_with(fcr, |fcr| run(fcr.0.clone()))
}

pub fn run(fcr: Arc<FlashCatReceiver>) -> impl Stream<Item = Result<(u64, Progress), Error>> {
    try_channel(1, move |mut sender: mpsc::Sender<(u64, Progress)>| async move {
        let mut stream = fcr.start().await.unwrap();
        while let Some(receiver_msg) = stream.next().await {
            match receiver_msg {
                ReceiverInteractionMessage::Message(msg) => {
                    *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Message(msg);
                }
                ReceiverInteractionMessage::Error(e) => {
                    return if e.contains("NotFound") {
                        Err(Error::ShareCodeNotFound)
                    } else {
                        Err(Error::OtherErroe(
                            t!("app.tab.receiver.error-msg", err = e.to_string()).to_string(),
                        ))
                    };
                }
                ReceiverInteractionMessage::SendFilesRequest(send_req) => {
                    let confirm_msg = if send_req.num_folders > 0 {
                        t!(
                            "app.tab.receiver.receiving-description",
                            file_count = send_req.num_files,
                            folder_count = send_req.num_folders
                        )
                    } else {
                        t!(
                            "app.tab.receiver.receiving-without-folder-description",
                            file_count = send_req.num_files
                        )
                    };
                    RECV_NUM_FILES.store(send_req.num_files, Ordering::Relaxed);
                    *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Confirm(ConfirmType::Receive, confirm_msg.to_string());
                    let _ = sender.send((0, Progress::None)).await;
                }
                ReceiverInteractionMessage::FileDuplication(file_duplication) => {
                    let confirm_msg = t!("app.tab.receiver.overwrite", file_path = file_duplication.path);
                    *RECEIVER_NOTIFICATION.write().unwrap() =
                        ReceiverNotification::Confirm(ConfirmType::FileDuplication(file_duplication.file_id), confirm_msg.to_string());
                    let _ = sender.send((file_duplication.file_id, Progress::None)).await;
                }
                ReceiverInteractionMessage::RecvNewFile(recv_new_file) => {
                    let _ = sender
                        .send((
                            recv_new_file.file_id,
                            Progress::New(recv_new_file.file_id, recv_new_file.filename.clone(), recv_new_file.size),
                        ))
                        .await;
                }
                ReceiverInteractionMessage::BreakPoint(break_point) => {
                    let _ = sender.send((break_point.file_id, Progress::Received(break_point.position as f32))).await;
                }
                ReceiverInteractionMessage::FileProgress(fp) => {
                    let _ = sender.send((fp.file_id, Progress::Received(fp.position as f32))).await;
                }
                ReceiverInteractionMessage::FileProgressFinish(file_id) => {
                    let _ = sender.send((file_id, Progress::Finished)).await;
                }
                ReceiverInteractionMessage::OtherClose => {
                    *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Errored(t!("app.tab.receiver.send-stop-err").to_string());
                    let _ = sender.send((0, Progress::Finished)).await;
                }
                ReceiverInteractionMessage::ReceiveDone => {
                    *RECEIVER_STATE.write().unwrap() = ReceiverState::RecvDone;
                    *RECEIVER_NOTIFICATION.write().unwrap() =
                        ReceiverNotification::Confirm(ConfirmType::OpenSavePath, t!("app.tab.receiver.open-saved-dir").to_string());
                    let _ = sender.send((0, Progress::Finished)).await;
                }
            }
        }
        Ok(())
    })
}

#[derive(Debug, Clone)]
pub enum Progress {
    None,
    New(u64, String, u64),
    Started,
    Received(f32),
    Finished,
    Skip,
}

#[derive(Debug, Clone)]
pub enum Error {
    ShareCodeNotFound,
    OtherErroe(String),
}
