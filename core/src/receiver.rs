use crate::{FileStage, TransferPhase};
use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use anyhow::{Result, anyhow, bail};
use bytes::Bytes;
use tokio::{
    fs,
    io::{AsyncSeekExt, AsyncWriteExt, BufWriter, SeekFrom},
    sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot},
};
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream as TokioReceiverStream};
use tokio_util::task::TaskTracker;
use tonic::transport::Endpoint;

use flash_cat_common::{
    Shutdown, compare_versions,
    consts::{FILE_REQUEST_WINDOW, PUBLIC_RELAY, RELAY_CHANNEL_CAPACITY, TRANSFER_QUEUE_BYTES},
    crypt::encryptor::Encryptor,
    proto::{
        BreakPointConfirm, Character, ClientType, Confirm, Done, FileConfirm, FileResumeProgress, Id, JoinRequest, NewFileConfirm, NewFileRequest,
        ReceiverUpdate, RelayUpdate, ResumeState, SenderUpdate, file_confirm::ConfirmMessage, join_response, receiver_update::ReceiverMessage,
        relay_service_client::RelayServiceClient, relay_update::RelayMessage, sender_update::SenderMessage,
    },
    utils::{
        fs::{identity::prefix_digest, receive_file::ReceiveFile, safe_join_relative_path},
        net::net_scout::NetScout,
    },
};
use flash_cat_relay::built_info;

use crate::{
    BreakPoint, FileDuplication, FileResult, FileStatus, PING_INTERVAL, Progress, ReceiverConfirm, ReceiverInteractionMessage, RecvNewFile, RelayType,
    SendFilesRequest, close_relay_session, close_relay_session_at, get_endpoint, normalize_relay_endpoint, progress::ProgressThrottle, send_msg_to_relay,
};

/// Receiver stream
pub type ReceiverStream = Pin<Box<dyn Stream<Item = ReceiverInteractionMessage> + Send>>;

#[derive(Clone)]
pub struct FlashCatReceiver {
    encryptor: Arc<Encryptor>,
    specify_relay: Option<String>,
    confirm_tx: async_channel::Sender<ReceiverConfirm>,
    confirm_rx: async_channel::Receiver<ReceiverConfirm>,
    output_dir: PathBuf,
    client_type: ClientType,
    lan: bool,
    shutdown: Shutdown,
    tasks: TaskTracker,
}

impl FlashCatReceiver {
    pub fn new(
        share_code: String,
        specify_relay: Option<String>,
        output: Option<String>,
        client_type: ClientType,
        lan: bool,
    ) -> Result<Self> {
        let encryptor = Arc::new(Encryptor::new(share_code)?);
        let (confirm_tx, confirm_rx) = async_channel::bounded(10);
        Ok(Self {
            encryptor,
            specify_relay,
            confirm_tx,
            confirm_rx,
            output_dir: output.map(PathBuf::from).unwrap_or_default(),
            client_type,
            lan,
            shutdown: Shutdown::new(),
            tasks: TaskTracker::new(),
        })
    }

    pub async fn start(self: Arc<Self>) -> Result<ReceiverStream> {
        let (receiver_stream_tx, mut receiver_stream_rx) = mpsc::channel(128);

        if self.specify_relay.is_some() {
            let specify_relay = self.specify_relay.clone().unwrap();
            let specify_relay_addr = normalize_relay_endpoint(specify_relay);
            let endpoint = get_endpoint(specify_relay_addr)?;
            self.connect_relay(RelayType::Specify, endpoint, receiver_stream_tx.clone(), self.shutdown.clone()).await?;
        } else {
            // discovery relay addr
            let relay_addr = self.discovery_relay_addr().await;
            if relay_addr.is_some() {
                let relay_addr = relay_addr.unwrap();
                let endpoint = get_endpoint(format!("http://{relay_addr}"))?;
                self.connect_relay(RelayType::Local, endpoint, receiver_stream_tx.clone(), self.shutdown.clone()).await?;
            } else {
                // public relay
                let endpoint = get_endpoint(format!("https://{PUBLIC_RELAY}"))?;
                self.connect_relay(RelayType::Public, endpoint, receiver_stream_tx.clone(), self.shutdown.clone()).await?;
            }
        }
        Ok(Box::pin(async_stream::stream! {
            loop {
                tokio::select! {
                    biased;
                    message = receiver_stream_rx.recv() => {
                        match message {
                            Some(message) => yield message,
                            None => break,
                        }
                    }
                    _ = self.shutdown.wait() => break,
                }
            }
        }))
    }

    pub async fn send_confirm(
        &self,
        confirm: ReceiverConfirm,
    ) -> Result<()> {
        self.confirm_tx.send(confirm).await?;
        Ok(())
    }

    async fn discovery_relay_addr(&self) -> Option<SocketAddr> {
        let match_content = self.encryptor.encrypt_share_code_bytes().to_vec();
        let shutdown = Shutdown::new();
        let net_scout = NetScout::new(match_content, Some(Duration::from_millis(1500)), shutdown.clone());
        if let Ok(addr) = net_scout.discovery().await {
            addr
        } else {
            None
        }
    }

    async fn connect_relay(
        &self,
        mut relay_type: RelayType,
        endpoint: Endpoint,
        receiver_stream_tx: mpsc::Sender<ReceiverInteractionMessage>,
        shutdown: Shutdown,
    ) -> Result<()> {
        let mut client = tokio::select! {
            _ = shutdown.wait() => return Ok(()),
            result = RelayServiceClient::connect(endpoint.clone()) => result?,
        };

        let join = client.join(JoinRequest {
            id: Some(Id {
                encrypted_share_code: self.encryptor.encrypt_share_code_bytes(),
                character: Character::Receiver.into(),
            }),
            client_type: self.client_type.into(),
            sender_local_relay: None,
        });
        let resp = match tokio::select! {
            _ = shutdown.wait() => {
                close_relay_session_at(&endpoint, self.encryptor.encrypt_share_code_bytes()).await;
                return Ok(());
            }
            result = join => result,
        } {
            Ok(resp) => resp,
            Err(status) => {
                let _ = Self::send_msg_to_stream(
                    &receiver_stream_tx,
                    ReceiverInteractionMessage::Error(status.message().to_string()),
                )
                .await;
                return Ok(());
            }
        };

        let (relay, sender_local_relay, client_latest_version) = if let Some(join_response_message) = resp.into_inner().join_response_message {
            match join_response_message {
                join_response::JoinResponseMessage::Success(join_success) => (
                    join_success.relay,
                    join_success.sender_local_relay,
                    join_success.client_latest_version,
                ),
                join_response::JoinResponseMessage::Failed(join_failed) => {
                    bail!(join_failed.error_msg);
                }
            }
        } else {
            bail!("can't get relay ip and port");
        };

        match self.client_type {
            ClientType::Cli => {
                if compare_versions(client_latest_version.as_str(), built_info::PKG_VERSION) == std::cmp::Ordering::Greater {
                    let _ = receiver_stream_tx
                        .send(ReceiverInteractionMessage::Message(format!(
                            "newly cli version[{}] is available, use `flash-cat update` to upgrade!",
                            client_latest_version
                        )))
                        .await;
                }
            }
            ClientType::App => {
                if compare_versions(client_latest_version.as_str(), built_info::PKG_VERSION) == std::cmp::Ordering::Greater {
                    let _ = receiver_stream_tx
                        .send(ReceiverInteractionMessage::Message(format!(
                            "newly app version[{}] is available",
                            client_latest_version
                        )))
                        .await;
                }
            }
        }

        let endpoint = if relay_type == RelayType::Public && self.lan {
            let sender_local_relay_endpoint = if sender_local_relay.is_some() {
                let sender_local_relay = sender_local_relay.unwrap();
                let sender_local_relay_endpoint = get_endpoint(format!(
                    "http://{}:{}",
                    sender_local_relay.relay_ip, sender_local_relay.relay_port
                ))?;

                match tokio::time::timeout(Duration::from_secs(1), async move {
                    if RelayServiceClient::connect(sender_local_relay_endpoint.clone()).await.is_ok() {
                        Some(sender_local_relay_endpoint)
                    } else {
                        None
                    }
                })
                .await
                {
                    Ok(sender_local_relay_endpoint) => sender_local_relay_endpoint,
                    Err(_) => None,
                }
            } else {
                None
            };

            match sender_local_relay_endpoint {
                Some(sender_local_relay_endpoint) => {
                    relay_type = RelayType::Local;
                    sender_local_relay_endpoint
                }
                None => {
                    if relay.is_some() {
                        let relay = relay.unwrap();
                        get_endpoint(format!("http://{}:{}", relay.relay_ip, relay.relay_port))?
                    } else {
                        endpoint
                    }
                }
            }
        } else {
            if relay.is_some() {
                let relay = relay.unwrap();
                get_endpoint(format!("http://{}:{}", relay.relay_ip, relay.relay_port))?
            } else {
                endpoint
            }
        };

        let encryptor = self.encryptor.clone();
        let confirm_rx = self.confirm_rx.clone();
        let output_dir = self.output_dir.clone();
        let tasks = self.tasks.clone();
        self.tasks.spawn(async move {
            // Cancellation must also interrupt a blocked write or progress update.
            // Close the session on every exit, including errors from those operations.
            let result = tokio::select! {
                biased;
                _ = shutdown.wait() => Ok(()),
                result = Self::relay_channel(
                    encryptor.clone(),
                    endpoint.clone(),
                    relay_type,
                    &receiver_stream_tx,
                    confirm_rx,
                    output_dir,
                    shutdown.clone(),
                    tasks,
                ) => result,
            };
            close_relay_session_at(&endpoint, encryptor.encrypt_share_code_bytes()).await;
            if let Err(e) = result {
                let _ = &receiver_stream_tx.send(ReceiverInteractionMessage::Error(e.to_string())).await;
            }
        });
        Ok(())
    }

    /// Establish a gRPC channel stream connection for receiver.
    async fn establish_channel(
        encryptor: &Encryptor,
        endpoint: &Endpoint,
    ) -> Result<(
        RelayServiceClient<tonic::transport::Channel>,
        mpsc::Sender<RelayUpdate>,
        tonic::Streaming<RelayUpdate>,
    )> {
        let mut client = RelayServiceClient::connect(endpoint.clone()).await?;

        let (tx, rx) = mpsc::channel(RELAY_CHANNEL_CAPACITY);

        let join = RelayMessage::Join(Id {
            encrypted_share_code: encryptor.encrypt_share_code_bytes(),
            character: Character::Receiver.into(),
        });
        tx.send(RelayUpdate {
            relay_message: Some(join),
        })
        .await?;

        let resp = client.channel(TokioReceiverStream::new(rx)).await?;
        let messages = resp.into_inner();

        Ok((client, tx, messages))
    }

    async fn relay_channel(
        encryptor: Arc<Encryptor>,
        endpoint: Endpoint,
        relay_type: RelayType,
        receiver_stream_tx: &mpsc::Sender<ReceiverInteractionMessage>,
        confirm_rx: async_channel::Receiver<ReceiverConfirm>,
        output_dir: PathBuf,
        shutdown: Shutdown,
        tasks: TaskTracker,
    ) -> Result<()> {
        let (mut client, mut tx, mut messages) = tokio::select! {
            _ = shutdown.wait() => {
                close_relay_session_at(&endpoint, encryptor.encrypt_share_code_bytes()).await;
                return Ok(());
            }
            result = Self::establish_channel(&encryptor, &endpoint) => result?,
        };

        let mut expected_entries = None;
        let mut file_states: HashMap<u64, ReceiveFileState> = HashMap::new();
        let mut share_confirm = None;
        let mut transfer_mode_reported = false;
        let mut progress = ProgressThrottle::default();
        let mut file_request_window = 1;
        let mut pending_requests = PendingFileRequests::default();

        let mut ping_interval = tokio::time::interval(PING_INTERVAL);
        let mut reconnect_attempt = 0u32;
        loop {
            let message = if let Some(request) = pending_requests.next_ready() {
                RelayMessage::Sender(SenderUpdate {
                    sender_message: Some(SenderMessage::NewFileRequest(request)),
                })
            } else {
                tokio::select! {
                    _ = shutdown.wait() => {
                        close_relay_session(&mut client, encryptor.encrypt_share_code_bytes()).await;
                        return Ok(());
                    }
                    _ = ping_interval.tick() => {
                        let _ = send_msg_to_relay(&tx, RelayMessage::Ping(0)).await;
                        continue;
                    }
                    Ok(confirm) = confirm_rx.recv() => {
                        match confirm {
                            ReceiverConfirm::ReceiveConfirm(accept) => {
                                share_confirm = Some(accept);
                                if accept {
                                    if file_request_window > 1 {
                                        send_msg_to_relay(&tx, RelayMessage::Receiver(ReceiverUpdate {
                                            receiver_message: Some(ReceiverMessage::FileRequestWindow(file_request_window)),
                                        })).await?;
                                    }
                                    let share_accept = RelayMessage::Receiver(ReceiverUpdate {
                                        receiver_message: Some(ReceiverMessage::ShareConfirm(
                                            Confirm::Accept.into(),
                                        )),
                                    });
                                    send_msg_to_relay(&tx, share_accept).await?;
                                } else {
                                    let share_reject = RelayMessage::Receiver(ReceiverUpdate {
                                        receiver_message: Some(ReceiverMessage::ShareConfirm(
                                            Confirm::Reject.into(),
                                        )),
                                    });
                                    send_msg_to_relay(&tx, share_reject).await?;
                                }
                            }
                            ReceiverConfirm::RenameFile(file_id) => {
                                if pending_requests.waiting_for != Some(file_id) { continue; }
                                let state = file_states.get_mut(&file_id).ok_or_else(|| anyhow!("missing receive file"))?;
                                state.received_bytes = 0;
                                let destination = state.destination.as_mut().ok_or_else(|| anyhow!("missing destination file"))?;
                                destination.keep_both()?;
                                state.file = Some(RecvFile::new(destination.file()?, 0, &tasks).await?);
                                Self::send_msg_to_stream(receiver_stream_tx, ReceiverInteractionMessage::FileRenamed((file_id, destination.target.to_string_lossy().into_owned()))).await?;
                                pending_requests.waiting_for = None;
                                send_file_confirmation(&tx, file_id, true, None).await?;
                            }
                            ReceiverConfirm::FileConfirm((accept, file_id)) => {
                                if pending_requests.waiting_for != Some(file_id) { continue; }
                                let state = file_states.get_mut(&file_id).ok_or_else(|| anyhow!("missing receive file"))?;
                                if accept {
                                    let destination = state.destination.as_mut().ok_or_else(|| anyhow!("missing destination file"))?;
                                    destination.open(true)?;
                                    state.file = Some(RecvFile::new(destination.file()?, 0, &tasks).await?);
                                    state.received_bytes = 0;
                                } else {
                                    if let Some(mut file) = state.file.take() { file.finish().await?; }
                                    state.destination = None;
                                    state.completed = true;
                                    let result = FileResult { file_id, status: FileStatus::Skipped as i32, error: String::new() };
                                    state.result = Some(result.clone());
                                    report_file_result(&tx, receiver_stream_tx, result).await?;
                                }
                                pending_requests.waiting_for = None;
                                send_file_confirmation(&tx, file_id, accept, None).await?;
                            }
                            ReceiverConfirm::BreakPointConfirm((accept, file_id, position)) => {
                                if pending_requests.waiting_for != Some(file_id) { continue; }
                                let state = file_states.get_mut(&file_id).ok_or_else(|| anyhow!("missing receive file"))?;
                                let destination = state.destination.as_mut().ok_or_else(|| anyhow!("missing destination file"))?;
                                if accept {
                                    destination.open_resume(position)?;
                                    let digest = prefix_digest(destination.file()?, position).await?;
                                    state.resume_proof = Some(format!("{}:sha256:{digest}", state.source_identity));
                                    state.file = Some(RecvFile::new(destination.file()?, position, &tasks).await?);
                                    state.received_bytes = position;
                                } else {
                                    destination.open(true)?;
                                    state.file = Some(RecvFile::new(destination.file()?, 0, &tasks).await?);
                                    state.received_bytes = 0;
                                }
                                pending_requests.waiting_for = None;
                                send_file_confirmation(&tx, file_id, true, Some((state.received_bytes, state.resume_identity()))).await?;
                            }
                        }
                        continue;
                    }
                    item = messages.next() => {
                        match item {
                            Some(Ok(update)) => {
                                match update.relay_message {
                                    Some(msg) => {
                                        reconnect_attempt = 0;
                                        msg
                                    }
                                    None => continue,
                                }
                            }
                            Some(Err(_)) | None => {
                                if shutdown.is_terminated() {
                                    close_relay_session(&mut client, encryptor.encrypt_share_code_bytes()).await;
                                    return Ok(());
                                }

                                let result = loop {
                                    if !crate::should_retry(reconnect_attempt) {
                                        let message = "max reconnect retries exceeded".to_string();
                                        Self::send_msg_to_stream(
                                            receiver_stream_tx,
                                            ReceiverInteractionMessage::ReconnectFailed(message),
                                        )
                                        .await?;
                                        shutdown.shutdown();
                                        return Ok(());
                                    }
                                    let delay = crate::reconnect_delay(reconnect_attempt);
                                    let _ = Self::send_msg_to_stream(
                                        receiver_stream_tx,
                                        ReceiverInteractionMessage::Message(format!(
                                            "Connection lost, reconnecting in {}s... (attempt {}/{})",
                                            delay.as_secs(),
                                            reconnect_attempt + 1,
                                            flash_cat_common::consts::MAX_RECONNECT_RETRIES
                                        )),
                                    )
                                    .await;
                                    tokio::select! {
                                        _ = shutdown.wait() => {
                                            close_relay_session(&mut client, encryptor.encrypt_share_code_bytes()).await;
                                            return Ok(());
                                        }
                                        _ = tokio::time::sleep(delay) => (),
                                    }
                                    reconnect_attempt += 1;

                                    if shutdown.is_terminated() {
                                        close_relay_session(&mut client, encryptor.encrypt_share_code_bytes()).await;
                                        return Ok(());
                                    }

                                    let reconnect = tokio::select! {
                                        _ = shutdown.wait() => {
                                            close_relay_session(&mut client, encryptor.encrypt_share_code_bytes()).await;
                                            return Ok(());
                                        }
                                        result = Self::establish_channel(&encryptor, &endpoint) => result,
                                    };
                                    match reconnect {
                                        Ok(result) => break result,
                                        Err(e) => {
                                            let _ = Self::send_msg_to_stream(
                                                receiver_stream_tx,
                                                ReceiverInteractionMessage::Message(format!(
                                                    "Reconnect failed: {e}"
                                                )),
                                            )
                                            .await;
                                        }
                                    }
                                };

                                let (new_client, new_tx, new_messages) = result;
                                client = new_client;
                                tx = new_tx;
                                messages = new_messages;
                                if let Some(accept) = share_confirm {
                                    send_msg_to_relay(
                                        &tx,
                                        RelayMessage::Receiver(ReceiverUpdate {
                                            receiver_message: Some(ReceiverMessage::ShareConfirm(if accept {
                                                Confirm::Accept.into()
                                            } else {
                                                Confirm::Reject.into()
                                            })),
                                        }),
                                    )
                                    .await?;
                                }
                                reconnect_attempt = 0;
                                ping_interval = tokio::time::interval(PING_INTERVAL);
                                let _ = Self::send_msg_to_stream(
                                    receiver_stream_tx,
                                    ReceiverInteractionMessage::Message("Reconnected successfully".to_string()),
                                )
                                .await;
                                continue;
                            }
                        }
                    }
                }
            };

            match message {
                RelayMessage::Join(_) => receiver_stream_tx.send(ReceiverInteractionMessage::Message("Invalid join message".to_string())).await?,
                RelayMessage::Joined(_) => {
                    if !transfer_mode_reported {
                        Self::send_msg_to_stream(receiver_stream_tx, ReceiverInteractionMessage::TransferMode(relay_type.clone())).await?;
                        transfer_mode_reported = true;
                    }
                }
                RelayMessage::Ready(_) => (),
                RelayMessage::Sender(sender) => {
                    if let Some(sender_message) = sender.sender_message {
                        match sender_message {
                            SenderMessage::SendRequest(send_req) => {
                                expected_entries = Some(send_req.num_entries);
                                file_request_window = send_req.file_request_window.clamp(1, FILE_REQUEST_WINDOW as u32);
                                Self::send_msg_to_stream(
                                    receiver_stream_tx,
                                    ReceiverInteractionMessage::SendFilesRequest(SendFilesRequest {
                                        num_entries: send_req.num_entries,
                                        total_size: send_req.total_size,
                                        num_files: send_req.num_files,
                                        num_folders: send_req.num_folders,
                                        max_file_name_length: send_req.max_file_name_length,
                                    }),
                                )
                                .await?;
                            }
                            SenderMessage::NewFileRequest(new_file_req) => {
                                if pending_requests.waiting_for.is_some() {
                                    pending_requests.defer(new_file_req)?;
                                    continue;
                                }
                                // A confirmation may cross a reconnect snapshot. Reuse
                                // accepted state instead of reopening/truncating its file.
                                if let Some(state) = file_states.get_mut(&new_file_req.file_id) {
                                    if state.source_identity != new_file_req.source_identity {
                                        bail!("Source metadata changed while reconnecting");
                                    }
                                    if state.completed {
                                        send_file_confirmation(&tx, new_file_req.file_id, false, None).await?;
                                    } else {
                                        let position = state.file.as_mut().ok_or_else(|| anyhow!("file is not open"))?.checkpoint().await?;
                                        send_file_confirmation(&tx, new_file_req.file_id, true, Some((position, state.resume_identity()))).await?;
                                    }
                                    continue;
                                }
                                let absolute_path = safe_join_relative_path(&output_dir, &new_file_req.relative_path)?;
                                if new_file_req.is_empty_dir {
                                    fs::create_dir_all(&absolute_path).await?;
                                    // Empty directories count as transfer entries on both clients.
                                    Self::send_msg_to_stream(
                                        receiver_stream_tx,
                                        ReceiverInteractionMessage::RecvNewFile(RecvNewFile {
                                            file_id: new_file_req.file_id,
                                            filename: new_file_req.filename.clone(),
                                            path: absolute_path.to_string_lossy().into_owned(),
                                            size: 0,
                                        }),
                                    )
                                    .await?;
                                    let result = FileResult {
                                        file_id: new_file_req.file_id,
                                        status: FileStatus::Success as i32,
                                        error: String::new(),
                                    };
                                    let mut state = ReceiveFileState::completed(0);
                                    state.result = Some(result.clone());
                                    file_states.insert(new_file_req.file_id, state);
                                    report_file_result(&tx, receiver_stream_tx, result).await?;
                                    send_file_confirmation(&tx, new_file_req.file_id, true, None).await?;
                                    continue;
                                }
                                let target_exists = absolute_path.try_exists()?;
                                let existing_size =
                                    std::fs::symlink_metadata(&absolute_path).ok().filter(|info| info.is_file()).map(|info| info.len()).unwrap_or(0);
                                let resumable = target_exists && existing_size > 0 && existing_size < new_file_req.total_size;
                                let mut destination = ReceiveFile::new(
                                    absolute_path.clone(),
                                    new_file_req.total_size,
                                    new_file_req.file_mode,
                                    shutdown.clone(),
                                );
                                let file = if target_exists {
                                    None
                                } else {
                                    destination.open(false)?;
                                    Some(RecvFile::new(destination.file()?, 0, &tasks).await?)
                                };
                                let mut state = ReceiveFileState::active(file, 0, new_file_req.source_identity.clone());
                                state.destination = Some(destination);
                                file_states.insert(new_file_req.file_id, state);
                                Self::send_msg_to_stream(
                                    receiver_stream_tx,
                                    ReceiverInteractionMessage::RecvNewFile(RecvNewFile {
                                        file_id: new_file_req.file_id,
                                        filename: new_file_req.filename.clone(),
                                        path: absolute_path.to_string_lossy().into_owned(),
                                        size: new_file_req.total_size,
                                    }),
                                )
                                .await?;
                                if resumable {
                                    pending_requests.waiting_for = Some(new_file_req.file_id);
                                    Self::send_msg_to_stream(
                                        receiver_stream_tx,
                                        ReceiverInteractionMessage::BreakPoint(BreakPoint {
                                            file_id: new_file_req.file_id,
                                            filename: new_file_req.filename,
                                            position: existing_size,
                                            percent: existing_size as f64 / new_file_req.total_size as f64 * 100.0,
                                        }),
                                    )
                                    .await?;
                                } else if target_exists {
                                    pending_requests.waiting_for = Some(new_file_req.file_id);
                                    Self::send_msg_to_stream(
                                        receiver_stream_tx,
                                        ReceiverInteractionMessage::FileDuplication(FileDuplication {
                                            file_id: new_file_req.file_id,
                                            filename: new_file_req.filename,
                                            path: absolute_path.to_string_lossy().into_owned(),
                                        }),
                                    )
                                    .await?;
                                } else {
                                    send_file_confirmation(&tx, new_file_req.file_id, true, None).await?;
                                }
                            }
                            SenderMessage::BreakPoint(break_point) => {
                                let state = file_states.get_mut(&break_point.file_id).ok_or_else(|| anyhow!("receive file failed"))?;
                                let recv_file = state.file.as_mut().ok_or_else(|| anyhow!("receive file is not open"))?;
                                if break_point.position == 0 {
                                    recv_file.restart().await?;
                                } else {
                                    if break_point.position > recv_file.checkpoint().await? {
                                        bail!("Resume position exceeds saved data");
                                    }
                                    recv_file.seek(break_point.position).await?;
                                }
                                state.resume_proof = None;
                                state.received_bytes = break_point.position;
                                state.started = true;
                                Self::send_msg_to_stream(
                                    receiver_stream_tx,
                                    ReceiverInteractionMessage::FileStage(FileStage {
                                        file_id: break_point.file_id,
                                        phase: TransferPhase::Transferring,
                                        position: break_point.position,
                                    }),
                                )
                                .await?;
                            }
                            SenderMessage::FileData(file_data) => {
                                let state = file_states.get_mut(&file_data.file_id).ok_or_else(|| anyhow!("receive file failed"))?;
                                if !state.started {
                                    Self::send_msg_to_stream(
                                        receiver_stream_tx,
                                        ReceiverInteractionMessage::FileStage(FileStage {
                                            file_id: file_data.file_id,
                                            phase: TransferPhase::Transferring,
                                            position: state.received_bytes,
                                        }),
                                    )
                                    .await?;
                                    state.started = true;
                                }
                                let recv_file = state.file.as_mut().ok_or_else(|| anyhow!("receive file is not open"))?;
                                let data = match encryptor.decrypt_owned(file_data.data) {
                                    Ok(data) => data,
                                    Err(e) => {
                                        bail!(format!("decrypt failed: {e}"));
                                    }
                                };
                                let expected = state.destination.as_ref().ok_or_else(|| anyhow!("missing destination file"))?.expected_size;
                                if state.received_bytes.saturating_add(data.len() as u64) > expected {
                                    bail!("Received data exceeds declared file size");
                                }
                                recv_file.write(data).await?;
                                state.received_bytes = recv_file.get_progress();
                                progress.report(
                                    receiver_stream_tx,
                                    ReceiverInteractionMessage::FileProgress(Progress {
                                        file_id: file_data.file_id,
                                        position: state.received_bytes,
                                    }),
                                )?;
                            }
                            SenderMessage::FileDone(file_done) => {
                                let state = file_states.get_mut(&file_done.file_id).ok_or_else(|| anyhow!("receive file failed"))?;
                                Self::send_msg_to_stream(
                                    receiver_stream_tx,
                                    ReceiverInteractionMessage::FileStage(FileStage {
                                        file_id: file_done.file_id,
                                        phase: TransferPhase::Saving,
                                        position: state.received_bytes,
                                    }),
                                )
                                .await?;
                                let completion = async {
                                    let mut recv_file = state.file.take().ok_or_else(|| anyhow!("receive file is not open"))?;
                                    recv_file.finish().await?;
                                    state.received_bytes = recv_file.get_progress();
                                    let destination = state.destination.take().ok_or_else(|| anyhow!("missing destination file"))?;
                                    tokio::task::spawn_blocking(move || destination.finish()).await??;
                                    Result::<()>::Ok(())
                                }
                                .await;
                                let result = FileResult {
                                    file_id: file_done.file_id,
                                    status: if completion.is_ok() {
                                        FileStatus::Success
                                    } else {
                                        FileStatus::Failed
                                    } as i32,
                                    error: completion.as_ref().err().map(|e| format!("{e:#}")).unwrap_or_default(),
                                };
                                state.completed = completion.is_ok();
                                state.result = Some(result.clone());
                                report_file_result(&tx, receiver_stream_tx, result).await?;
                                completion?;
                            }
                            SenderMessage::ResumeRequest(_) => {
                                // Sender reconnected and asks for current file progress
                                pending_requests.queued.clear();
                                let mut files = Vec::new();
                                for (&file_id, state) in file_states.iter_mut() {
                                    // Unconfirmed existing files must still require consent
                                    // after reconnection; they are not resume checkpoints.
                                    if pending_requests.waiting_for == Some(file_id) {
                                        continue;
                                    }
                                    if let Some(file) = state.file.as_mut() {
                                        state.received_bytes = file.checkpoint().await?;
                                    }
                                    if let Some(result) = state.result.clone() {
                                        report_file_result(&tx, receiver_stream_tx, result).await?;
                                    }
                                    files.push(FileResumeProgress {
                                        file_id,
                                        received_bytes: state.received_bytes,
                                        completed: state.completed,
                                        source_identity: state.resume_identity(),
                                    });
                                }
                                // Reply with ResumeState
                                send_msg_to_relay(
                                    &tx,
                                    RelayMessage::Receiver(ReceiverUpdate {
                                        receiver_message: Some(ReceiverMessage::ResumeState(ResumeState {
                                            files,
                                        })),
                                    }),
                                )
                                .await?;
                            }
                        }
                    }
                }
                RelayMessage::Receiver(_) => {
                    Self::send_msg_to_stream(
                        receiver_stream_tx,
                        ReceiverInteractionMessage::Message("Invalid receiver message".to_string()),
                    )
                    .await?;
                }
                RelayMessage::Done(_) => {
                    if expected_entries != Some(file_states.len() as u64)
                        || file_states.values().any(|state| !state.completed)
                        || pending_requests.waiting_for.is_some()
                        || !pending_requests.queued.is_empty()
                    {
                        bail!("Transfer ended before all files were finalized");
                    }
                    send_msg_to_relay(&tx, RelayMessage::Done(Done {})).await?;
                    Self::send_msg_to_stream(receiver_stream_tx, ReceiverInteractionMessage::ReceiveDone).await?;
                }
                RelayMessage::Error(e) => {
                    receiver_stream_tx.send(ReceiverInteractionMessage::Error(e.to_string())).await?;
                }
                RelayMessage::Terminated(_) => {
                    Self::send_msg_to_stream(receiver_stream_tx, ReceiverInteractionMessage::OtherClose).await?;
                    return Ok(());
                }
                RelayMessage::Ping(_) => (),
                RelayMessage::Pong(_) => (),
            }
        }
    }

    pub fn shutdown(&self) {
        self.shutdown.shutdown();
        self.tasks.close();
    }

    /// Wait for the relay close handshake and queued file writes to finish.
    pub async fn shutdown_complete(&self) {
        self.tasks.wait().await;
    }

    pub async fn terminated(&self) {
        self.shutdown.wait().await
    }

    /// Send message to receiver. cli | app
    async fn send_msg_to_stream(
        tx: &mpsc::Sender<ReceiverInteractionMessage>,
        msg: ReceiverInteractionMessage,
    ) -> Result<()> {
        tx.send(msg).await?;
        Ok(())
    }
}

/// Only one overwrite/resume prompt may be visible in CLI or GUI at a time.
#[derive(Default)]
struct PendingFileRequests {
    waiting_for: Option<u64>,
    queued: VecDeque<NewFileRequest>,
}

impl PendingFileRequests {
    fn defer(
        &mut self,
        request: NewFileRequest,
    ) -> Result<()> {
        if self.waiting_for == Some(request.file_id) || self.queued.iter().any(|r| r.file_id == request.file_id) {
            return Ok(());
        }
        if self.queued.len() >= FILE_REQUEST_WINDOW {
            bail!("too many pending file requests");
        }
        self.queued.push_back(request);
        Ok(())
    }

    fn next_ready(&mut self) -> Option<NewFileRequest> {
        if self.waiting_for.is_none() {
            self.queued.pop_front()
        } else {
            None
        }
    }
}

enum FileWriteCommand {
    Write(Bytes, OwnedSemaphorePermit),
    Checkpoint(oneshot::Sender<Result<u64, String>>),
    Seek(u64, oneshot::Sender<Result<u64, String>>),
    Restart(oneshot::Sender<Result<u64, String>>),
    Finish,
}

struct ReceiveFileState {
    file: Option<RecvFile>,
    destination: Option<ReceiveFile>,
    source_identity: String,
    resume_proof: Option<String>,
    received_bytes: u64,
    completed: bool,
    result: Option<FileResult>,
    started: bool,
}

impl ReceiveFileState {
    fn resume_identity(&self) -> String {
        self.resume_proof.as_ref().unwrap_or(&self.source_identity).clone()
    }

    fn active(
        file: Option<RecvFile>,
        received_bytes: u64,
        source_identity: String,
    ) -> Self {
        Self {
            file,
            destination: None,
            source_identity,
            resume_proof: None,
            received_bytes,
            completed: false,
            result: None,
            started: false,
        }
    }

    fn completed(received_bytes: u64) -> Self {
        Self {
            file: None,
            destination: None,
            source_identity: String::new(),
            resume_proof: None,
            received_bytes,
            completed: true,
            result: None,
            started: false,
        }
    }
}

struct RecvFile {
    tx: tokio::sync::mpsc::Sender<FileWriteCommand>,
    writer_handle: Option<tokio::task::JoinHandle<Result<()>>>,
    progress: u64,
    budget: Arc<Semaphore>,
}

impl RecvFile {
    async fn new(
        mut file: fs::File,
        position: u64,
        tasks: &TaskTracker,
    ) -> Result<Self> {
        // Bound queued data independently of the file size. Writes are acknowledged
        // only at barriers, allowing reception and filesystem I/O to overlap.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<FileWriteCommand>(64);

        let writer_handle = tasks.spawn(async move {
            let mut progress = position;
            file.seek(SeekFrom::Start(position)).await?;
            let mut file = BufWriter::with_capacity(1024 * 1024, file);
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    FileWriteCommand::Write(data, _permit) => {
                        file.write_all(&data).await?;
                        progress += data.len() as u64;
                    }
                    FileWriteCommand::Checkpoint(ack) => {
                        let result = file.flush().await.map(|()| progress);
                        let failed = result.is_err();
                        let _ = ack.send(result.map_err(|e| e.to_string()));
                        if failed {
                            bail!("failed to flush receive file");
                        }
                    }
                    FileWriteCommand::Seek(position, ack) => {
                        let result: Result<u64> = async {
                            file.flush().await?;
                            file.seek(SeekFrom::Start(position)).await?;
                            progress = position;
                            Ok(progress)
                        }
                        .await;
                        let failed = result.is_err();
                        let _ = ack.send(result.map_err(|e| e.to_string()));
                        if failed {
                            bail!("failed to seek receive file");
                        }
                    }
                    FileWriteCommand::Restart(ack) => {
                        let result: Result<u64> = async {
                            file.flush().await?;
                            file.get_mut().set_len(0).await?;
                            file.seek(SeekFrom::Start(0)).await?;
                            progress = 0;
                            Ok(progress)
                        }
                        .await;
                        let failed = result.is_err();
                        let _ = ack.send(result.map_err(|e| e.to_string()));
                        if failed {
                            bail!("failed to restart receive file");
                        }
                    }
                    FileWriteCommand::Finish => break,
                }
            }
            // Also drain and flush if the producer disappears during cancellation.
            file.flush().await?;
            Ok(())
        });

        Ok(Self {
            tx,
            writer_handle: Some(writer_handle),
            progress: position,
            budget: Arc::new(Semaphore::new(TRANSFER_QUEUE_BYTES)),
        })
    }

    async fn write(
        &mut self,
        data: Bytes,
    ) -> Result<()> {
        if data.len() > TRANSFER_QUEUE_BYTES {
            bail!("file data exceeds the receive queue budget");
        }
        let len = data.len() as u64;
        let permit = self.budget.clone().acquire_many_owned(data.len() as u32).await?;
        self.send_command(FileWriteCommand::Write(data, permit)).await?;
        // UI progress counts accepted data; resume uses checkpoint() instead.
        self.progress += len;
        Ok(())
    }

    async fn send_command(
        &mut self,
        command: FileWriteCommand,
    ) -> Result<()> {
        if self.tx.send(command).await.is_err() {
            self.join_writer().await?;
            bail!("writer task stopped");
        }
        Ok(())
    }

    async fn join_writer(&mut self) -> Result<()> {
        if let Some(handle) = self.writer_handle.take() {
            handle.await.map_err(|e| anyhow!("writer task failed: {e}"))??;
        }
        Ok(())
    }

    async fn checkpoint(&mut self) -> Result<u64> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.send_command(FileWriteCommand::Checkpoint(ack_tx)).await?;
        match ack_rx.await {
            Ok(result) => result.map_err(anyhow::Error::msg),
            Err(_) => {
                self.join_writer().await?;
                bail!("writer task stopped");
            }
        }
    }

    async fn seek(
        &mut self,
        position: u64,
    ) -> Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.send_command(FileWriteCommand::Seek(position, ack_tx)).await?;
        self.progress = ack_rx.await.map_err(|_| anyhow!("writer task stopped"))?.map_err(anyhow::Error::msg)?;
        Ok(())
    }

    async fn restart(&mut self) -> Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.send_command(FileWriteCommand::Restart(ack_tx)).await?;
        self.progress = ack_rx.await.map_err(|_| anyhow!("writer task stopped"))?.map_err(anyhow::Error::msg)?;
        Ok(())
    }

    async fn finish(&mut self) -> Result<()> {
        self.send_command(FileWriteCommand::Finish).await?;
        self.join_writer().await
    }

    fn get_progress(&self) -> u64 {
        self.progress
    }
}

async fn send_file_confirmation(
    tx: &mpsc::Sender<RelayUpdate>,
    file_id: u64,
    accept: bool,
    resume: Option<(u64, String)>,
) -> Result<()> {
    let confirm = if accept {
        Confirm::Accept
    } else {
        Confirm::Reject
    } as i32;
    let confirm_message = match resume {
        Some((position, source_identity)) => ConfirmMessage::BreakPointConfirm(BreakPointConfirm {
            file_id,
            position,
            confirm,
            source_identity,
        }),
        None => ConfirmMessage::NewFileConfirm(NewFileConfirm {
            file_id,
            confirm,
        }),
    };
    send_msg_to_relay(
        tx,
        RelayMessage::Receiver(ReceiverUpdate {
            receiver_message: Some(ReceiverMessage::FileConfirm(FileConfirm {
                confirm_message: Some(confirm_message),
            })),
        }),
    )
    .await
}

async fn report_file_result(
    tx: &mpsc::Sender<RelayUpdate>,
    stream: &mpsc::Sender<ReceiverInteractionMessage>,
    result: FileResult,
) -> Result<()> {
    stream.send(ReceiverInteractionMessage::FileResult(result.clone())).await?;
    send_msg_to_relay(
        tx,
        RelayMessage::Receiver(ReceiverUpdate {
            receiver_message: Some(ReceiverMessage::FileResult(result)),
        }),
    )
    .await
}
