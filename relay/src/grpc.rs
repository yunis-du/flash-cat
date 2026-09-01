use std::{sync::Arc, time::Duration};

use log::{debug, error, info};
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming, transport::Endpoint};

use flash_cat_common::{
    consts::{DEFAULT_CONNECT_TIMEOUT, RELAY_CHANNEL_CAPACITY},
    proto::{
        Character, CloseRequest, CloseResponse, JoinFailed, JoinRequest, JoinResponse, JoinSuccess, Joined, Ready, RelayInfo, RelayUpdate, Terminated,
        join_response::JoinResponseMessage, relay_service_client::RelayServiceClient, relay_service_server::RelayService, relay_update::RelayMessage,
    },
};

use crate::{
    built_info,
    relay::RelayState,
    session::{ConnectionLease, Metadata, Session},
};

#[derive(Clone)]
pub struct GrpcServer(Arc<RelayState>);

impl GrpcServer {
    pub fn new(state: Arc<RelayState>) -> Self {
        Self(state)
    }
}

type RR<T> = Result<Response<T>, Status>;

async fn forwarded_client(forward: std::net::SocketAddr) -> Result<RelayServiceClient<tonic::transport::Channel>, Status> {
    Endpoint::from_shared(format!("http://{forward}"))
        .map_err(|error| Status::internal(format!("invalid forward relay address: {error}")))?
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .connect()
        .await
        .map(RelayServiceClient::new)
        .map_err(|error| Status::unavailable(format!("failed to connect to forward relay {forward}: {error}")))
}

#[tonic::async_trait]
impl RelayService for GrpcServer {
    type ChannelStream = ReceiverStream<Result<RelayUpdate, Status>>;

    async fn join(
        &self,
        request: Request<JoinRequest>,
    ) -> RR<JoinResponse> {
        if let Some(forward) = self.0.forward() {
            let mut response = forwarded_client(forward).await?.join(request.into_inner()).await?.into_inner();
            if let Some(JoinResponseMessage::Success(success)) = response.join_response_message.as_mut() {
                success.relay = Some(RelayInfo {
                    relay_ip: forward.ip().to_string(),
                    relay_port: forward.port() as u32,
                });
            }
            return Ok(Response::new(response));
        }

        let relay_local_addr = request.local_addr();
        let relay_port = match relay_local_addr {
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
                    Character::Sender => {
                        debug!("new sender({session_code}) incoming");
                        let metadata = Metadata {
                            encrypted_share_code: id.encrypted_share_code,
                            sender_local_relay: request.sender_local_relay,
                        };
                        let session = Arc::new(Session::new(metadata));
                        if !self.0.insert_if_absent(&session_code, session.clone()) {
                            return Ok(Response::new(JoinResponse {
                                join_response_message: Some(JoinResponseMessage::Failed(JoinFailed {
                                    error_msg: "share code already has an active sender session".to_string(),
                                })),
                            }));
                        }
                    }
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
                    None => match relay_local_addr {
                        Some(addr) => Some(RelayInfo {
                            relay_ip: addr.ip().to_string(),
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

        let (tx, rx) = mpsc::channel(RELAY_CHANNEL_CAPACITY);

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
        let connection = session.register_connection(character).await;

        if let Character::Receiver = character {
            // Ready for interaction.
            if let Err(e) = session
                .send_to_share(RelayMessage::Ready(Ready {
                    local_relay: self.0.is_local_relay(),
                }))
                .await
            {
                error!("send ready to sharer failed: {e}");
            }
            info!(
                "receiver(addr: {remote_addr}, session_id: {}, generation: {}) started channel",
                session.id(),
                connection.generation()
            );
        } else {
            info!(
                "sender(addr: {remote_addr}, session_id: {}, generation: {}) started channel",
                session.id(),
                connection.generation()
            );
        }

        tokio::spawn(async move {
            let result = handle_streaming(&tx, &session, stream, character, &connection).await;
            connection.finish();
            if let Err(err) = result {
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
        if let Some(forward) = self.0.forward() {
            return forwarded_client(forward).await?.close(request.into_inner()).await;
        }

        let request = request.into_inner();
        let session_code = String::from_utf8_lossy(request.encrypted_share_code.as_ref()).to_string();
        // Closing must not wait on either participant's bounded update queue. A
        // stalled/disconnected client can fill its queue and used to prevent the
        // other participant from ever being notified.
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
    connection: &ConnectionLease,
) -> Result<(), &'static str> {
    let (update_tx, update_rx) = match character {
        Character::Sender => (session.recipient_update_tx(), session.sharer_update_rx()),
        Character::Receiver => (session.sharer_update_tx(), session.recipient_update_rx()),
    };
    loop {
        tokio::select! {
            biased;
            _ = connection.superseded() => return Ok(()),
            _ = session.terminated() => {
                send_msg(tx, RelayMessage::Terminated(Terminated {})).await;
                return Ok(());
            }
            // Send buffered server updates to the client.
            Ok(msg) = update_rx.recv() => {
                tokio::select! {
                    biased;
                    _ = connection.superseded() => return Ok(()),
                    _ = session.terminated() => return Ok(()),
                    sent = send_msg(tx, msg) => {
                        if !sent {
                            return Err("failed to send update message");
                        }
                    }
                }
            }
            // Handle incoming client messages.
            maybe_update = stream.next() => {
                if let Some(Ok(update)) = maybe_update {
                    if !handle_update(tx, session, update, update_tx, connection).await {
                        return Err("error responding to client update");
                    }
                } else {
                    // The client has hung up on their end.
                    return Ok(());
                }
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
    connection: &ConnectionLease,
) -> bool {
    session.access();
    match update.relay_message {
        Some(relay_message) => {
            if let RelayMessage::Join(_) = relay_message {
                return tokio::select! {
                    biased;
                    _ = connection.superseded() => false,
                    sent = send_err(tx, "unexpected join".into()) => sent,
                };
            }
            if let RelayMessage::Ping(_) = relay_message {
                return tokio::select! {
                    biased;
                    _ = connection.superseded() => false,
                    sent = send_msg(tx, RelayMessage::Pong(0)) => sent,
                };
            }
            tokio::select! {
                biased;
                _ = connection.superseded() => return false,
                result = update_tx.send(relay_message) => {
                    if result.is_err() {
                        return false;
                    }
                }
                _ = session.terminated() => {
                    // Forwarding can be blocked by a full peer queue. Notify this
                    // client directly instead of waiting for that queue to drain.
                    send_msg(tx, RelayMessage::Terminated(Terminated {})).await;
                    return false;
                }
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

#[cfg(test)]
mod tests {
    use std::{net::TcpListener, sync::Arc, time::Duration};

    use bytes::Bytes;
    use flash_cat_common::proto::{ClientType, Id};

    use super::*;
    use crate::relay::Relay;

    fn unused_local_addr() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    }

    #[tokio::test]
    async fn forwarded_join_and_close_use_target_session_store() {
        let target_addr = unused_local_addr();
        let target = Arc::new(Relay::new(None, false).unwrap());
        let running_target = Arc::clone(&target);
        let target_task = tokio::spawn(async move { running_target.bind(target_addr).await });

        let target_endpoint = format!("http://{target_addr}");
        for _ in 0..50 {
            if RelayServiceClient::connect(target_endpoint.clone()).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let entry = GrpcServer::new(Arc::new(RelayState::new_with_forward(None, false, Some(target_addr)).unwrap()));
        let session_code = Bytes::from_static(b"forwarded-session");
        let response = entry
            .join(Request::new(JoinRequest {
                id: Some(Id {
                    encrypted_share_code: session_code.clone(),
                    character: Character::Sender.into(),
                }),
                client_type: ClientType::Cli.into(),
                sender_local_relay: None,
            }))
            .await
            .unwrap()
            .into_inner();

        let Some(JoinResponseMessage::Success(success)) = response.join_response_message else {
            panic!("forwarded join did not succeed");
        };
        let advertised_relay = success.relay.unwrap();
        assert_eq!(advertised_relay.relay_ip, target_addr.ip().to_string());
        assert_eq!(advertised_relay.relay_port, target_addr.port() as u32);
        assert!(target.state().lookup("forwarded-session").is_some());

        entry
            .close(Request::new(CloseRequest {
                encrypted_share_code: session_code,
            }))
            .await
            .unwrap();
        assert!(target.state().lookup("forwarded-session").is_none());

        target.shutdown();
        target_task.await.unwrap().unwrap();
    }
}
