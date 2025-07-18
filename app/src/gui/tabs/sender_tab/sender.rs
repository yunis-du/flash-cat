use std::sync::Arc;

use iced::futures::{SinkExt, Stream, StreamExt};
use iced::stream::try_channel;

use flash_cat_core::{SenderInteractionMessage, sender::FlashCatSender};

use super::{SENDER_NOTIFICATION, SenderNotification};

pub fn send(fcs: Arc<FlashCatSender>) -> iced::Subscription<Result<(u64, Progress), Error>> {
    iced::Subscription::run_with_id(0, run(fcs).map(move |progress| progress))
}

pub fn run(fcs: Arc<FlashCatSender>) -> impl Stream<Item = Result<(u64, Progress), Error>> {
    try_channel(1, move |mut sender| async move {
        let mut stream = fcs.start().await.unwrap();
        while let Some(sender_msg) = stream.next().await {
            match sender_msg {
                SenderInteractionMessage::Message(msg) => {
                    *SENDER_NOTIFICATION.write().unwrap() = SenderNotification::Message(msg);
                }
                SenderInteractionMessage::Error(e) => {
                    return Err(Error::Errored(e));
                }
                SenderInteractionMessage::ReceiverReject => {
                    return Err(Error::Reject);
                }
                SenderInteractionMessage::RelayFailed((relay_type, error)) => {
                    return Err(Error::RelayFailed(format!(
                        "connect to {} relay failed: {}",
                        relay_type.to_string(),
                        error
                    )));
                }
                SenderInteractionMessage::ContinueFile(file_id) => {
                    let _ = sender.send((file_id, Progress::Skip)).await;
                }
                SenderInteractionMessage::FileProgress(file_progress) => {
                    let _ = sender.send((file_progress.file_id, Progress::Sent(file_progress.position as f32))).await;
                }
                SenderInteractionMessage::FileProgressFinish(file_id) => {
                    let _ = sender.send((file_id, Progress::Finished)).await;
                }
                SenderInteractionMessage::OtherClose => {
                    return Err(Error::OtherClose);
                }
                SenderInteractionMessage::SendDone => {
                    *SENDER_NOTIFICATION.write().unwrap() =
                        SenderNotification::Message("Send files done. Waiting for the receiver to receive finish...".to_string());
                    let _ = sender.send((0, Progress::Done)).await;
                }
                SenderInteractionMessage::Completed => {
                    let _ = sender.send((0, Progress::Done)).await;
                }
            }
        }
        Ok(())
    })
}

#[derive(Debug, Clone)]
pub enum Progress {
    None,
    Started,
    Sent(f32),
    Finished,
    Skip,
    Done,
}

#[derive(Debug, Clone)]
pub enum Error {
    Reject,
    OtherClose,
    Errored(String),
    RelayFailed(String),
}
