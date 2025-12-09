use std::{sync::Arc, time::Duration};

use log::{debug, error, info};
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming};

use flash_cat_common::{
    proto::{
        Character, CloseRequest, CloseResponse, JoinFailed, JoinRequest, JoinResponse, JoinSuccess, Joined, Ready, RelayInfo, RelayUpdate, Terminated,
        join_response::JoinResponseMessage, relay_service_server::RelayService, relay_update::RelayMessage,
    },
    utils::net::get_local_ip,
};

use crate::{
    built_info,
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

    async fn join(
        &self,
        request: Request<JoinRequest>,
    ) -> RR<JoinResponse> {
        let relay_port = match request.local_addr() {
            Some(local_addr) => local_addr.port() as u32,
            None => 0,
        };

        let request = request.into_inner();
        match request.id {
            Some(id) => {
                let session_code = String::from_utf8_lossy(id.encrypted_share_code.as_ref()).to_string();
                let character = match Character::try_from(id.character) {
                    Ok(character) => character,
                    Err(_) => return Err(Status::invalid_argument("unknown character")),
                };
                let mut sender_local_relay = None;

                match character {
                    Character::Sender => match self.0.lookup(&session_code) {
                        Some(_) => return Err(Status::already_exists("duplicate session_code")),
                        None => {
                            debug!("new sender({session_code}) incoming");
                            let metadata = Metadata {
                                encrypted_share_code: id.encrypted_share_code,
                                sender_local_relay: request.sender_local_relay,
                            };
                            let session = Arc::new(Session::new(metadata));
                            self.0.insert(&session_code, session.clone());
                        }
                    },
                    Character::Receiver => match self.0.lookup(&session_code) {
                        None => {
                            return Err(Status::not_found("Not found, Please check share code."));
                        }
                        Some(session) => {
                            debug!("new receiver({session_code}) incoming");
                            sender_local_relay = session.metadata().sender_local_relay.clone();
                        }
                    },
                }

                let relay = match self.0.external_ip() {
                    Some(ip) => Some(RelayInfo {
                        relay_ip: ip.to_string(),
                        relay_port,
                    }),
                    None => match get_local_ip() {
                        Some(ip) => Some(RelayInfo {
                            relay_ip: ip.to_string(),
                            relay_port,
                        }),
                        None => None,
                    },
                };

                let client_latest_version = built_info::PKG_VERSION.to_string();

                Ok(Response::new(JoinResponse {
                    join_response_message: Some(JoinResponseMessage::Success(JoinSuccess {
                        relay,
                        sender_local_relay,
                        client_latest_version,
                    })),
                }))
            }
            None => Ok(Response::new(JoinResponse {
                join_response_message: Some(JoinResponseMessage::Failed(JoinFailed {
                    error_msg: "Id is required".to_string(),
                })),
            })),
        }
    }

    async fn channel(
        &self,
        request: Request<Streaming<RelayUpdate>>,
    ) -> RR<Self::ChannelStream> {
        let remote_addr = match request.remote_addr() {
            Some(addr) => addr.to_string(),
            None => "unknown".to_string(),
        };

        let mut stream = request.into_inner();
        let first_update = match stream.next().await {
            Some(result) => result?,
            None => return Err(Status::invalid_argument("missing first message")),
        };

        let (tx, rx) = mpsc::channel(1024);

        let (session, character) = match first_update.relay_message {
            Some(RelayMessage::Join(join)) => {
                let session_code = String::from_utf8_lossy(join.encrypted_share_code.as_ref()).to_string();
                let character = match Character::try_from(join.character) {
                    Ok(character) => character,
                    Err(_) => return Err(Status::invalid_argument("unknown character")),
                };
                let session = match self.0.lookup(&session_code) {
                    None => return Err(Status::not_found("Not found, Please check share code.")),
                    Some(session) => session,
                };
                send_msg(&tx, RelayMessage::Joined(Joined {})).await;
                (session, character)
            }
            _ => return Err(Status::invalid_argument("invalid first message")),
        };

        if let Character::Receiver = character {
            // readly to interaction
            if let Err(e) = session
                .broadcast(RelayMessage::Ready(Ready {
                    local_relay: self.0.is_local_relay(),
                }))
                .await
            {
                error!("broadcast failed: {e}");
            }
            info!("receiver(addr: {remote_addr}, session_id: {}) started channel", session.id());
        } else {
            info!("sender(addr: {remote_addr}, session_id: {}) started channel", session.id());
        }

        tokio::spawn(async move {
            if let Err(err) = handle_streaming(&tx, &session, stream, character).await {
                error!(
                    "connection(addr: {remote_addr}, session_id: {}) exiting early due to an error {err}",
                    session.id()
                );
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn close(
        &self,
        request: Request<CloseRequest>,
    ) -> RR<CloseResponse> {
        let request = request.into_inner();
        let session_code = String::from_utf8_lossy(request.encrypted_share_code.as_ref()).to_string();
        if let Some(session) = self.0.lookup(&session_code) {
            if let Err(e) = session.broadcast(RelayMessage::Terminated(Terminated {})).await {
                error!("broadcast failed: {e}");
            }
        }
        // wait for broadcast message send to end
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.0.close_session(&session_code);

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
                send_err(tx, "disconnecting because session terminated".into()).await;
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
            if let RelayMessage::Ping(_) = relay_message {
                return send_msg(tx, RelayMessage::Pong(0)).await;
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
async fn send_msg(
    tx: &RelayTx,
    message: RelayMessage,
) -> bool {
    let update = Ok(RelayUpdate {
        relay_message: Some(message),
    });
    let max_retries = 3;
    let mut retry_count = 0;
    loop {
        match tx.send(update.clone()).await {
            Ok(_) => return true,
            Err(e) => {
                error!("Failed to send relay update: {}, retry: {}", e, retry_count);
                retry_count += 1;
                if retry_count >= max_retries {
                    error!("Max retries reached, giving up sending update");
                    return false;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

/// Attempt to send an error string to the client.
async fn send_err(
    tx: &RelayTx,
    err: String,
) -> bool {
    send_msg(tx, RelayMessage::Error(err)).await
}
