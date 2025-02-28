use std::sync::{
    Arc, LazyLock,
    atomic::{AtomicU64, Ordering},
};

use iced::futures::{SinkExt, Stream, StreamExt};
use iced::stream::try_channel;

use flash_cat_core::{ReceiverInteractionMessage, receiver::FlashCatReceiver};

use super::{RECEIVER_NOTIFICATION, RECEIVER_STATE, ReceiverNotification, ReceiverState};
use crate::gui::tabs::receiver_tab::ConfirmType;

pub(super) static RECV_NUM_FILES: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));

pub fn recv(fcr: Arc<FlashCatReceiver>) -> iced::Subscription<Result<(u64, Progress), Error>> {
    iced::Subscription::run_with_id(0, run(fcr).map(move |progress| progress))
}

pub fn run(fcr: Arc<FlashCatReceiver>) -> impl Stream<Item = Result<(u64, Progress), Error>> {
    try_channel(1, move |mut sender| async move {
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
                        Err(Error::OtherErroe(format!(
                            "An error occurred: {}",
                            e.to_string()
                        )))
                    };
                }
                ReceiverInteractionMessage::SendFilesRequest(send_req) => {
                    let confirm_msg = if send_req.num_folders > 0 {
                        format!(
                            "Receiving {} files and {} folders?",
                            send_req.num_files, send_req.num_folders
                        )
                    } else {
                        format!("Receiving {} files?", send_req.num_files)
                    };
                    RECV_NUM_FILES.store(send_req.num_files, Ordering::Relaxed);
                    *RECEIVER_NOTIFICATION.write().unwrap() =
                        ReceiverNotification::Confirm(ConfirmType::Receive, confirm_msg);
                    let _ = sender.send((0, Progress::None)).await;
                }
                ReceiverInteractionMessage::FileDuplication(file_duplication) => {
                    let confirm_msg = format!("overwrite '{}'?", file_duplication.path);
                    *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Confirm(
                        ConfirmType::FileDuplication(file_duplication.file_id),
                        confirm_msg,
                    );
                    let _ = sender
                        .send((file_duplication.file_id, Progress::None))
                        .await;
                }
                ReceiverInteractionMessage::RecvNewFile(recv_new_file) => {
                    let _ = sender
                        .send((
                            recv_new_file.file_id,
                            Progress::New(
                                recv_new_file.file_id,
                                recv_new_file.filename.clone(),
                                recv_new_file.size,
                            ),
                        ))
                        .await;
                }
                ReceiverInteractionMessage::FileProgress(fp) => {
                    let _ = sender
                        .send((fp.file_id, Progress::Received(fp.position as f32)))
                        .await;
                }
                ReceiverInteractionMessage::FileProgressFinish(file_id) => {
                    let _ = sender.send((file_id, Progress::Finished)).await;
                }
                ReceiverInteractionMessage::OtherClose => {
                    *RECEIVER_NOTIFICATION.write().unwrap() =
                        ReceiverNotification::Errored("The sender stopped sending".to_string());
                    let _ = sender.send((0, Progress::Finished)).await;
                }
                ReceiverInteractionMessage::ReceiveDone => {
                    *RECEIVER_STATE.write().unwrap() = ReceiverState::RecvDone;
                    *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Confirm(
                        ConfirmType::OpenSavePath,
                        "Open the saved directory?".to_string(),
                    );
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
