use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, LazyLock,
};

use flash_cat_core::{
    receiver::{FlashCatReceiver, ReceiverStream},
    ReceiverInteractionMessage,
};
use iced::futures::StreamExt;

use crate::gui::tabs::receiver_tab::ConfirmType;

use super::{ReceiverNotification, ReceiverState, RECEIVER_NOTIFICATION, RECEIVER_STATE};

pub(super) static RECV_NUM_FILES: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));

pub fn recv(fcr: Arc<FlashCatReceiver>) -> iced::Subscription<(u64, Progress)> {
    iced::subscription::unfold(0, State::Ready(fcr), move |state| run(state))
}

pub async fn run(state: State) -> ((u64, Progress), State) {
    match state {
        State::Ready(fcr) => {
            let stream = fcr.clone().start().await.unwrap();
            ((0, Progress::Started), State::Receiving(stream, fcr))
        }
        State::Receiving(mut stream, fcr) => {
            if let Some(receiver_msg) = stream.next().await {
                match receiver_msg {
                    ReceiverInteractionMessage::Message(msg) => {
                        *RECEIVER_NOTIFICATION.write().unwrap() =
                            ReceiverNotification::Message(msg);
                        ((0, Progress::None), State::Receiving(stream, fcr))
                    }
                    ReceiverInteractionMessage::Error(e) => {
                        let err_msg = if e.contains("NotFound") {
                            "Not found, Please check share code.".to_string()
                        } else {
                            format!("An error occurred: {}", e.to_string())
                        };
                        *RECEIVER_NOTIFICATION.write().unwrap() =
                            ReceiverNotification::Errored(err_msg);
                        ((0, Progress::None), State::Finished)
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
                        ((0, Progress::None), State::Receiving(stream, fcr))
                    }
                    ReceiverInteractionMessage::FileDuplication(file_duplication) => {
                        let confirm_msg = format!("overwrite '{}'?", file_duplication.path);
                        *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Confirm(
                            ConfirmType::FileDuplication(file_duplication.file_id),
                            confirm_msg,
                        );
                        (
                            (file_duplication.file_id, Progress::None),
                            State::Receiving(stream, fcr),
                        )
                    }
                    ReceiverInteractionMessage::RecvNewFile(recv_new_file) => (
                        (
                            recv_new_file.file_id,
                            Progress::New(
                                recv_new_file.file_id,
                                recv_new_file.filename.clone(),
                                recv_new_file.size,
                            ),
                        ),
                        State::Receiving(stream, fcr),
                    ),
                    ReceiverInteractionMessage::FileProgress(fp) => (
                        (fp.file_id, Progress::Received(fp.position as f32)),
                        State::Receiving(stream, fcr),
                    ),
                    ReceiverInteractionMessage::FileProgressFinish(file_id) => {
                        ((file_id, Progress::Finished), State::Receiving(stream, fcr))
                    }
                    ReceiverInteractionMessage::OtherClose => {
                        *RECEIVER_NOTIFICATION.write().unwrap() =
                            ReceiverNotification::Errored("The sender stopped sending".to_string());
                        ((0, Progress::Finished), State::Finished)
                    }
                    ReceiverInteractionMessage::ReceiveDone => {
                        *RECEIVER_STATE.write().unwrap() = ReceiverState::RecvDone;
                        *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Confirm(
                            ConfirmType::OpenSavePath,
                            "Open the saved directory?".to_string(),
                        );
                        ((0, Progress::Finished), State::Finished)
                    }
                }
            } else {
                *RECEIVER_NOTIFICATION.write().unwrap() =
                    ReceiverNotification::Errored("stream error".to_string());
                ((0, Progress::None), State::Finished)
            }
        }
        State::Finished => iced::futures::future::pending().await,
    }
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

pub enum State {
    Ready(Arc<FlashCatReceiver>),
    Receiving(ReceiverStream, Arc<FlashCatReceiver>),
    Finished,
}
