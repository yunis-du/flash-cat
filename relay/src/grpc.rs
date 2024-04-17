use std::{sync::Arc, time::Duration};

use log::{debug, error, info, warn};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

use flash_cat_common::{
    proto::{
        relay_service_server::RelayService, relay_update::RelayMessage, Character, CloseRequest,
        CloseResponse, Joined, Ready, RelayUpdate, Terminated,
    },
    utils::get_time_ms,
};

use crate::{
    relay::RelayState,
    session::{Metadata, Session},
};

#[derive(Clone)]
pub struct GrpcServer(Arc<RelayState>);

impl GrpcServer {
    pub fn new(state: Arc<RelayState>) -> Self {
        Self(state)
    }
}

type RR<T> = Result<Response<T>, Status>;

#[tonic::async_trait]
impl RelayService for GrpcServer {
    type ChannelStream = ReceiverStream<Result<RelayUpdate, Status>>;

    async fn channel(&self, request: Request<Streaming<RelayUpdate>>) -> RR<Self::ChannelStream> {
        let mut stream = request.into_inner();
        let first_update = match stream.next().await {
            Some(result) => result?,
            None => return Err(Status::invalid_argument("missing first message")),
        };

        let (tx, rx) = mpsc::channel(16);

        let (session, character) = match first_update.relay_message {
            Some(RelayMessage::Join(join)) => {
                let session_name =
                    String::from_utf8_lossy(join.encrypted_share_code.as_ref()).to_string();
                let character = match Character::try_from(join.character) {
                    Ok(character) => character,
                    Err(_) => return Err(Status::invalid_argument("unknown character")),
                };
                let session = match character {
                    Character::Sender => match self.0.lookup(&session_name) {
                        Some(_) => return Err(Status::already_exists("duplicate session_name")),
                        None => {
                            debug!("new sharer[{session_name}] incoming");
                            let metadata = Metadata {
                                encrypted_share_code: join.encrypted_share_code,
                            };
                            let session = Arc::new(Session::new(metadata));
                            self.0.insert(&session_name, session.clone());
                            send_msg(&tx, RelayMessage::Joined(Joined {})).await;
                            session
                        }
                    },
                    Character::Receiver => match self.0.lookup(&session_name) {
                        None => return Err(Status::not_found("session not found")),
                        Some(session) => session,
                    },
                };
                (session, character)
            }
            _ => return Err(Status::invalid_argument("invalid first message")),
        };

        if let Character::Receiver = character {
            // readly to interaction
            if let Err(e) = session.broadcast(RelayMessage::Ready(Ready {})).await {
                error!("broadcast failed: {e}");
            }
        }

        tokio::spawn(async move {
            if let Err(err) = handle_streaming(&tx, &session, stream, character).await {
                warn!("connection exiting early due to an error {err}");
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn close(&self, request: Request<CloseRequest>) -> RR<CloseResponse> {
        let request = request.into_inner();
        let session_name =
            String::from_utf8_lossy(request.encrypted_share_code.as_ref()).to_string();
        info!("closing session {}", session_name);
        if let Some(session) = self.0.lookup(&session_name) {
            if let Err(e) = session
                .broadcast(RelayMessage::Terminated(Terminated {}))
                .await
            {
                error!("broadcast failed: {e}");
            }
        }
        // wait for broadcast message send to end
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Err(err) = self.0.close_session(&session_name).await {
            error!("failed to close session {}, error: {}", session_name, err);
            return Err(Status::internal(err.to_string()));
        }
        Ok(Response::new(CloseResponse {}))
    }
}

type RelayTx = mpsc::Sender<Result<RelayUpdate, Status>>;

/// Handle bidirectional streaming messages RPC messages.
async fn handle_streaming(
    tx: &RelayTx,
    session: &Session,
    mut stream: Streaming<RelayUpdate>,
    character: Character,
) -> Result<(), &'static str> {
    let (update_tx, update_rx) = match character {
        Character::Sender => (session.recipient_update_tx(), session.sharer_update_rx()),
        Character::Receiver => (session.sharer_update_tx(), session.recipient_update_rx()),
    };
    loop {
        tokio::select! {
            // Send buffered server updates to the client.
            Ok(msg) = update_rx.recv() => {
                if !send_msg(tx, msg).await {
                    return Err("failed to send update message");
                }
            }
            // Handle incoming client messages.
            maybe_update = stream.next() => {
                if let Some(Ok(update)) = maybe_update {
                    if !handle_update(tx, session, update, update_tx).await {
                        return Err("error responding to client update");
                    }
                } else {
                    // The client has hung up on their end.
                    return Ok(());
                }
            }
            // Exit on a session shutdown signal.
            _ = session.terminated() => {
                let msg = String::from("disconnecting because session is closed");
                send_msg(tx, RelayMessage::Error(msg)).await;
                return Ok(());
            }
        }
    }
}

/// Handles a singe update from the client. Returns `true` on success.
async fn handle_update(
    tx: &RelayTx,
    session: &Session,
    update: RelayUpdate,
    update_tx: &async_channel::Sender<RelayMessage>,
) -> bool {
    session.access();
    match update.relay_message {
        Some(relay_message) => {
            if let RelayMessage::Join(_) = relay_message {
                return send_err(tx, "unexpected join".into()).await;
            }
            if let RelayMessage::Ping(ts) = relay_message {
                let now = get_time_ms();
                return send_msg(tx, RelayMessage::Pong(now - ts)).await;
            }
            if let Err(_) = update_tx.send(relay_message).await {
                return false;
            }
        }
        None => (),
    }
    true
}

/// Attempt to send a server message to the client.
async fn send_msg(tx: &RelayTx, message: RelayMessage) -> bool {
    let update = Ok(RelayUpdate {
        relay_message: Some(message),
    });
    tx.send(update).await.is_ok()
}

/// Attempt to send an error string to the client.
async fn send_err(tx: &RelayTx, err: String) -> bool {
    send_msg(tx, RelayMessage::Error(err)).await
}
