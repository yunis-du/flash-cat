use std::sync::Arc;

use flash_cat_core::sender::FlashCatSender;
use flash_cat_core::{sender::SenderStream, SenderInteractionMessage};
use iced::futures::StreamExt;

use super::{SenderState, SenderStreamState, SENDER_STATE, SENDER_STREAM_STATE};

pub fn send(fcs: Arc<FlashCatSender>) -> iced::Subscription<(u64, Progress)> {
    iced::subscription::unfold(0, State::Ready(fcs), move |state| run(state))
}

pub async fn run(state: State) -> ((u64, Progress), State) {
    match state {
        State::Ready(fcs) => {
            let stream = fcs.clone().start().await.unwrap();

            ((0, Progress::Started), State::Sending(stream))
        }
        State::Sending(mut stream) => {
            if let Some(sender_msg) = stream.next().await {
                match sender_msg {
                    SenderInteractionMessage::Message(msg) => {
                        *SENDER_STREAM_STATE.write().unwrap() = SenderStreamState::Message(msg);
                        ((0, Progress::None), State::Sending(stream))
                    }
                    SenderInteractionMessage::Error(e) => {
                        *SENDER_STREAM_STATE.write().unwrap() = SenderStreamState::Errored(e);
                        ((0, Progress::None), State::Finished)
                    }
                    SenderInteractionMessage::ReceiverReject => {
                        ((0, Progress::Reject), State::Finished)
                    }
                    SenderInteractionMessage::RelayFailed((relay_type, error)) => {
                        *SENDER_STREAM_STATE.write().unwrap() =
                            SenderStreamState::Errored(format!(
                                "connect to {} relay failed: {}",
                                relay_type.to_string(),
                                error
                            ));
                        ((0, Progress::None), State::Finished)
                    }
                    SenderInteractionMessage::ContinueFile(file_id) => {
                        ((file_id, Progress::Skip), State::Sending(stream))
                    }
                    SenderInteractionMessage::FileProgress(file_progress) => (
                        (
                            file_progress.file_id,
                            Progress::Sent(file_progress.position as f32),
                        ),
                        State::Sending(stream),
                    ),
                    SenderInteractionMessage::FileProgressFinish(file_id) => {
                        ((file_id, Progress::Finished), State::Sending(stream))
                    }
                    SenderInteractionMessage::OtherClose => {
                        *SENDER_STREAM_STATE.write().unwrap() = SenderStreamState::Message(
                            "The receive end is interrupted.".to_string(),
                        );
                        ((0, Progress::None), State::Finished)
                    }
                    SenderInteractionMessage::SendDone => {
                        *SENDER_STREAM_STATE.write().unwrap() = SenderStreamState::Message(
                            "Send files done. Waiting for the receiver to receive finish..."
                                .to_string(),
                        );
                        ((0, Progress::None), State::Sending(stream))
                    }
                    SenderInteractionMessage::Completed => {
                        ((0, Progress::Finished), State::Finished)
                    }
                }
            } else {
                *SENDER_STREAM_STATE.write().unwrap() =
                    SenderStreamState::Errored("stream error".to_string());
                ((0, Progress::None), State::Finished)
            }
        }
        State::Finished => {
            *SENDER_STATE.write().unwrap() = SenderState::SendDone;
            iced::futures::future::pending().await
        }
    }
}

#[derive(Debug, Clone)]
pub enum Progress {
    None,
    Started,
    Reject,
    Sent(f32),
    Finished,
    Skip,
    Message(String),
    Errored(String),
}

pub enum State {
    Ready(Arc<FlashCatSender>),
    Sending(SenderStream),
    Finished,
}
