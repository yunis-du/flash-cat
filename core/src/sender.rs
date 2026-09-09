use crate::{FileStage, TransferPhase};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
    path::Path,
    pin::Pin,
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::{Result, bail};
use flash_cat_common::utils::fs::identity::{file_identity, prefix_digest};
use tokio::{signal::ctrl_c, sync::mpsc};
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tonic::transport::Endpoint;

use flash_cat_common::{
    Shutdown, compare_versions,
    consts::{DEFAULT_RELAY_PORT, FILE_REQUEST_WINDOW, PUBLIC_RELAY, RELAY_CHANNEL_CAPACITY},
    crypt::encryptor::Encryptor,
    proto::{
        BreakPoint, Character, ClientType, Confirm, Done, FileConfirm, FileData, FileDone, Id, JoinRequest, NewFileRequest, RelayUpdate, SendRequest,
        SenderUpdate, file_confirm::ConfirmMessage, join_response, receiver_update::ReceiverMessage, relay_service_client::RelayServiceClient,
        relay_update::RelayMessage, sender_update::SenderMessage,
    },
    utils::{
        fs::{FileCollector, FileInfo, ScanProgress, collect_files_with_progress, is_idr, paths_exist, zip_folder},
        net::{find_available_port, net_scout::NetScout},
    },
};
use flash_cat_relay::{built_info, relay::Relay};

use crate::{
    PING_INTERVAL, Progress, RelayType, SenderInteractionMessage,
    chunks::{ChunkPool, FileChunks},
    close_relay_session, close_relay_session_at, get_endpoint, normalize_relay_endpoint,
    progress::ProgressThrottle,
    send_msg_to_relay,
};

/// How long the sender waits for receiver-side file confirmation.
pub const FILE_CONFIRM_TIMEOUT: Duration = Duration::from_secs(300);

/// Sender stream
pub type SenderStream = Pin<Box<dyn Stream<Item = SenderInteractionMessage> + Send>>;

#[derive(Debug, Clone)]
struct SenderLifecycle {
    root: CancellationToken,
    local: CancellationToken,
    public: CancellationToken,
    specified: CancellationToken,
    selected_local: Arc<OnceLock<bool>>,
    relay_tasks: TaskTracker,
}

impl SenderLifecycle {
    fn new() -> Self {
        let root = CancellationToken::new();
        Self {
            local: root.child_token(),
            public: root.child_token(),
            specified: root.child_token(),
            selected_local: Arc::new(OnceLock::new()),
            relay_tasks: TaskTracker::new(),
            root,
        }
    }

    fn relay_token(
        &self,
        relay_type: &RelayType,
    ) -> CancellationToken {
        match relay_type {
            RelayType::Local => self.local.clone(),
            RelayType::Public => self.public.clone(),
            RelayType::Specify => self.specified.clone(),
        }
    }

    fn select_path(
        &self,
        local: bool,
    ) -> bool {
        *self.selected_local.get_or_init(|| {
            if local {
                self.public.cancel();
            } else {
                self.local.cancel();
            }
            local
        }) == local
    }

    fn cancel(&self) {
        self.root.cancel();
        self.relay_tasks.close();
    }

    async fn wait(&self) {
        self.relay_tasks.wait().await;
    }

    fn is_selected(
        &self,
        relay_type: &RelayType,
    ) -> bool {
        match relay_type {
            RelayType::Specify => true,
            RelayType::Local => self.selected_local.get().copied() == Some(true),
            RelayType::Public => self.selected_local.get().copied() == Some(false),
        }
    }
}

impl Default for SenderLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct FlashCatSender {
    zip_dirs: Vec<Arc<tempfile::TempDir>>,
    encryptor: Arc<Encryptor>,
    specify_relay: Option<String>,
    file_collector: Arc<FileCollector>,
    lifecycle: SenderLifecycle,
    client_type: ClientType,
    lan_broadcast: bool,
}

impl FlashCatSender {
    pub async fn new(
        share_code: String,
        specify_relay: Option<String>,
        files: Vec<String>,
        zip_floder: bool,
        client_type: ClientType,
        lan_broadcast: bool,
    ) -> Result<Self> {
        Self::new_with_scan_progress(share_code, specify_relay, files, zip_floder, client_type, lan_broadcast, |_| {}).await
    }

    pub async fn new_with_scan_progress(
        share_code: String,
        specify_relay: Option<String>,
        mut files: Vec<String>,
        zip_floder: bool,
        client_type: ClientType,
        lan_broadcast: bool,
        report: impl FnMut(ScanProgress) + Send + 'static,
    ) -> Result<Self> {
        paths_exist(files.as_slice())?;
        let lifecycle = SenderLifecycle::new();
        let mut zip_dirs = vec![];
        if zip_floder {
            let (treated_files, zip) = Self::zip_folder(files, lifecycle.root.clone()).await?;
            files = treated_files;
            zip_dirs = zip;
        }
        let scan_cancel = Shutdown::new();
        let worker_cancel = scan_cancel.clone();
        let _cancel_on_drop = CancelScanOnDrop(scan_cancel.clone());
        let scan = tokio::task::spawn_blocking(move || collect_files_with_progress(&files, &worker_cancel, report));
        let file_collector = tokio::select! {
            result = scan => result??,
            _ = tokio::signal::ctrl_c() => { scan_cancel.shutdown(); bail!("File scan cancelled"); }
        };
        let encryptor = Arc::new(Encryptor::new(share_code)?);
        Ok(Self {
            zip_dirs,
            encryptor,
            specify_relay,
            file_collector: Arc::new(file_collector),
            lifecycle,
            client_type,
            lan_broadcast,
        })
    }

    pub fn new_with_file_collector(
        share_code: String,
        specify_relay: Option<String>,
        file_collector: impl Into<Arc<FileCollector>>,
        client_type: ClientType,
        lan_broadcast: bool,
    ) -> Result<Self> {
        let file_collector = file_collector.into();
        if file_collector.files.is_empty() {
            bail!("No files to send");
        }
        let encryptor = Arc::new(Encryptor::new(share_code)?);
        Ok(Self {
            zip_dirs: vec![],
            encryptor,
            specify_relay,
            file_collector,
            lifecycle: SenderLifecycle::new(),
            client_type,
            lan_broadcast,
        })
    }

    pub async fn start(self: Arc<Self>) -> Result<SenderStream> {
        let (sender_stream_tx, mut sender_stream_rx) = mpsc::channel(128);

        if self.specify_relay.is_some() {
            let specify_relay = self.specify_relay.clone().unwrap();
            let specify_relay_addr = normalize_relay_endpoint(specify_relay);
            let endpoint = get_endpoint(specify_relay_addr)?;
            self.connect_relay(RelayType::Specify, endpoint, sender_stream_tx.clone()).await?;
        } else {
            // start local relay
            let local_relay_port = find_available_port(DEFAULT_RELAY_PORT);
            self.start_local_relay(
                format!("0.0.0.0:{}", local_relay_port).parse().unwrap(),
                sender_stream_tx.clone(),
                self.lifecycle.local.clone(),
            )
            .await;

            // waite for local relay start
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // connect local relay
            let endpoint = get_endpoint(format!("http://127.0.0.1:{local_relay_port}"))?;
            self.connect_relay(RelayType::Local, endpoint, sender_stream_tx.clone()).await?;

            // connect public relay
            let endpoint = get_endpoint(format!("https://{PUBLIC_RELAY}"))?;
            self.connect_relay(RelayType::Public, endpoint, sender_stream_tx.clone()).await?;

            if self.lan_broadcast {
                self.broadcast_relay_addr(local_relay_port, sender_stream_tx.clone(), self.lifecycle.local.clone()).await;
            }
        }
        let root_cancel = self.lifecycle.root.clone();
        Ok(Box::pin(async_stream::stream! {
            loop {
                tokio::select! {
                    biased;
                    message = sender_stream_rx.recv() => {
                        match message {
                            Some(message) => yield message,
                            None => break,
                        }
                    }
                    _ = root_cancel.cancelled() => break,
                }
            }
        }))
    }

    async fn start_local_relay(
        &self,
        local_relay_addr: SocketAddr,
        sender_stream_tx: mpsc::Sender<SenderInteractionMessage>,
        local_cancel: CancellationToken,
    ) {
        tokio::spawn(async move {
            let relay = match Relay::new(None, true) {
                Ok(relay) => relay,
                Err(e) => {
                    let _ = &sender_stream_tx
                        .send(SenderInteractionMessage::Error(format!(
                            "start local relay error {}",
                            e.to_string()
                        )))
                        .await;
                    return;
                }
            };

            let relay_task = async { relay.bind(local_relay_addr).await };

            let signals_task = async {
                local_cancel.cancelled().await;
                // Waiting done message send to the right end.
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                relay.shutdown();
            };

            let _ = tokio::join!(relay_task, signals_task);
        });
    }

    async fn broadcast_relay_addr(
        &self,
        local_relay_port: u16,
        sender_stream_tx: mpsc::Sender<SenderInteractionMessage>,
        local_cancel: CancellationToken,
    ) {
        let scout_shutdown = Shutdown::new();
        let match_content = self.encryptor.encrypt_share_code_bytes().to_vec();
        tokio::spawn(async move {
            let mut net_scout = NetScout::new(match_content, None, scout_shutdown);
            let broadcast_result = tokio::select! {
                result = net_scout.broadcast(local_relay_port) => Some(result),
                _ = local_cancel.cancelled() => None,
            };
            if let Some(Err(e)) = broadcast_result {
                // LAN discovery is an optional optimization. TUN-based VPNs commonly
                // reject broadcast traffic, but the public relay remains usable.
                let _ = &sender_stream_tx
                    .send(SenderInteractionMessage::Message(format!(
                        "LAN discovery unavailable; continuing through relay: {e}"
                    )))
                    .await;
            }
        });
    }

    async fn connect_relay(
        &self,
        relay_type: RelayType,
        mut endpoint: Endpoint,
        sender_stream_tx: mpsc::Sender<SenderInteractionMessage>,
    ) -> Result<()> {
        let cancel = self.lifecycle.relay_token(&relay_type);
        let mut client = tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            result = RelayServiceClient::connect(endpoint.clone()) => result?,
        };

        let join = client.join(JoinRequest {
            id: Some(Id {
                encrypted_share_code: self.encryptor.encrypt_share_code_bytes(),
                character: Character::Sender.into(),
            }),
            client_type: self.client_type.into(),
            // The source address for LAN discovery must come from the
            // interface that received the broadcast. A single route-derived
            // address is ambiguous on multi-homed and TUN-enabled hosts.
            sender_local_relay: None,
        });
        let resp = match tokio::select! {
            _ = cancel.cancelled() => {
                close_relay_session_at(&endpoint, self.encryptor.encrypt_share_code_bytes()).await;
                return Ok(());
            }
            result = join => result,
        } {
            Ok(resp) => resp,
            Err(status) => {
                let _ = Self::send_msg_to_stream(
                    &sender_stream_tx,
                    SenderInteractionMessage::RelayFailed((relay_type, status.message().to_string())),
                )
                .await;
                return Ok(());
            }
        };

        let (relay, client_latest_version) = if let Some(join_response_message) = resp.into_inner().join_response_message {
            match join_response_message {
                join_response::JoinResponseMessage::Success(join_success) => (join_success.relay, join_success.client_latest_version),
                join_response::JoinResponseMessage::Failed(join_failed) => {
                    bail!(join_failed.error_msg);
                }
            }
        } else {
            bail!("can't get relay info");
        };

        match self.client_type {
            ClientType::Cli => {
                if compare_versions(client_latest_version.as_str(), built_info::PKG_VERSION) == std::cmp::Ordering::Greater {
                    let _ = sender_stream_tx
                        .send(SenderInteractionMessage::Message(format!(
                            "newly cli version[{}] is available, use `flash-cat update` to upgrade!",
                            client_latest_version
                        )))
                        .await;
                }
            }
            ClientType::App => {
                if compare_versions(client_latest_version.as_str(), built_info::PKG_VERSION) == std::cmp::Ordering::Greater {
                    let _ = sender_stream_tx
                        .send(SenderInteractionMessage::Message(format!(
                            "newly app version[{}] is available",
                            client_latest_version
                        )))
                        .await;
                }
            }
        }

        match relay {
            Some(relay_info) => {
                // Directly connect to Relay, improve performance
                endpoint = get_endpoint(format!("http://{}:{}", relay_info.relay_ip, relay_info.relay_port))?;
            }
            None => (),
        }

        let encryptor = self.encryptor.clone();
        let file_collector = self.file_collector.clone();
        let lifecycle = self.lifecycle.clone();
        let relay_tasks = lifecycle.relay_tasks.clone();
        relay_tasks.spawn(async move {
            if let Err(e) = Self::relay_channel(
                relay_type.clone(),
                encryptor,
                file_collector.clone(),
                endpoint,
                &sender_stream_tx,
                lifecycle,
            )
            .await
            {
                let _ = Self::send_msg_to_stream(
                    &sender_stream_tx,
                    SenderInteractionMessage::RelayFailed((relay_type, e.to_string())),
                )
                .await;
            }
        });
        Ok(())
    }

    /// Establish a gRPC channel stream connection. Returns the client, tx, messages stream, and confirm channels.
    async fn establish_channel(
        encryptor: &Encryptor,
        endpoint: &Endpoint,
    ) -> Result<(
        RelayServiceClient<tonic::transport::Channel>,
        mpsc::Sender<RelayUpdate>,
        tonic::Streaming<RelayUpdate>,
        async_channel::Sender<FileConfirm>,
        async_channel::Receiver<FileConfirm>,
    )> {
        let mut client = RelayServiceClient::connect(endpoint.clone()).await?;

        let (tx, rx) = mpsc::channel(RELAY_CHANNEL_CAPACITY);

        let join = RelayMessage::Join(Id {
            encrypted_share_code: encryptor.encrypt_share_code_bytes(),
            character: Character::Sender.into(),
        });
        tx.send(RelayUpdate {
            relay_message: Some(join),
        })
        .await?;

        let resp = client.channel(ReceiverStream::new(rx)).await?;
        let messages = resp.into_inner();

        let (confirm_tx, confirm_rx) = async_channel::bounded(10);

        Ok((client, tx, messages, confirm_tx, confirm_rx))
    }

    async fn relay_channel(
        relay_type: RelayType,
        encryptor: Arc<Encryptor>,
        file_collector: Arc<FileCollector>,
        endpoint: Endpoint,
        sender_stream_tx: &mpsc::Sender<SenderInteractionMessage>,
        lifecycle: SenderLifecycle,
    ) -> Result<()> {
        let cancel = lifecycle.relay_token(&relay_type);
        let (mut client, mut tx, mut messages, mut confirm_tx, mut confirm_rx) = tokio::select! {
            _ = cancel.cancelled() => {
                close_relay_session_at(&endpoint, encryptor.encrypt_share_code_bytes()).await;
                return Ok(());
            }
            result = Self::establish_channel(&encryptor, &endpoint) => result?,
        };

        let mut ping_interval = tokio::time::interval(PING_INTERVAL);
        let mut reconnect_attempt = 0u32;
        let mut is_first_connect = true;
        let mut send_files_cancel = cancel.child_token();
        let mut send_task = None;
        let mut request_sent = false;
        let mut share_accepted = false;
        let mut received_file_result = false;
        let mut file_request_window = 1;
        loop {
            let message = tokio::select! {
                _ = cancel.cancelled() => {
                    Self::stop_send_task(&mut send_task, &send_files_cancel).await;
                    close_relay_session(&mut client, encryptor.encrypt_share_code_bytes()).await;
                    return Ok(());
                }
                _ = ping_interval.tick() => {
                    let _ = send_msg_to_relay(&tx, RelayMessage::Ping(0)).await;
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
                            if cancel.is_cancelled() {
                                close_relay_session(&mut client, encryptor.encrypt_share_code_bytes()).await;
                                return Ok(());
                            }
                            Self::stop_send_task(&mut send_task, &send_files_cancel).await;

                            let result = loop {
                                if !crate::should_retry(reconnect_attempt) {
                                    let message = "max reconnect retries exceeded".to_string();
                                    if lifecycle.is_selected(&relay_type) {
                                        Self::send_msg_to_stream(
                                            sender_stream_tx,
                                            SenderInteractionMessage::ReconnectFailed(message),
                                        )
                                        .await?;
                                        lifecycle.cancel();
                                        return Ok(());
                                    }
                                    bail!(message);
                                }
                                let delay = crate::reconnect_delay(reconnect_attempt);
                                let _ = Self::send_msg_to_stream(
                                    sender_stream_tx,
                                    SenderInteractionMessage::Message(format!(
                                        "Connection lost, reconnecting in {}s... (attempt {}/{})",
                                        delay.as_secs(),
                                        reconnect_attempt + 1,
                                        flash_cat_common::consts::MAX_RECONNECT_RETRIES
                                    )),
                                )
                                .await;
                                tokio::select! {
                                    _ = cancel.cancelled() => {
                                        close_relay_session(&mut client, encryptor.encrypt_share_code_bytes()).await;
                                        return Ok(());
                                    }
                                    _ = tokio::time::sleep(delay) => (),
                                }
                                reconnect_attempt += 1;

                                if cancel.is_cancelled() {
                                    close_relay_session(&mut client, encryptor.encrypt_share_code_bytes()).await;
                                    return Ok(());
                                }

                                let reconnect = tokio::select! {
                                    _ = cancel.cancelled() => {
                                        close_relay_session(&mut client, encryptor.encrypt_share_code_bytes()).await;
                                        return Ok(());
                                    }
                                    result = Self::establish_channel(&encryptor, &endpoint) => result,
                                };
                                match reconnect {
                                    Ok(result) => break result,
                                    Err(e) => {
                                        let _ = Self::send_msg_to_stream(
                                            sender_stream_tx,
                                            SenderInteractionMessage::Message(format!(
                                                "Reconnect failed: {e}"
                                            )),
                                        )
                                        .await;
                                    }
                                }
                            };

                            let (new_client, new_tx, new_messages, new_confirm_tx, new_confirm_rx) = result;
                            client = new_client;
                            tx = new_tx;
                            messages = new_messages;
                            confirm_tx = new_confirm_tx;
                            confirm_rx = new_confirm_rx;
                            send_files_cancel = cancel.child_token();
                            is_first_connect = false;
                            reconnect_attempt = 0;
                            ping_interval = tokio::time::interval(PING_INTERVAL);
                            let _ = Self::send_msg_to_stream(
                                sender_stream_tx,
                                SenderInteractionMessage::Message("Reconnected successfully".to_string()),
                            )
                            .await;
                            continue;
                        }
                    }
                }
            };

            match message {
                RelayMessage::Join(_) => {
                    Self::send_msg_to_stream(
                        sender_stream_tx,
                        SenderInteractionMessage::Message("Invalid join message".to_string()),
                    )
                    .await?;
                }
                RelayMessage::Joined(_) => {
                    if is_first_connect {
                        Self::send_msg_to_stream(sender_stream_tx, SenderInteractionMessage::RelayConnected(relay_type.clone())).await?;
                    }
                    // After reconnection, send ResumeRequest instead of waiting for Ready
                    if !is_first_connect && share_accepted {
                        send_msg_to_relay(
                            &tx,
                            RelayMessage::Sender(SenderUpdate {
                                sender_message: Some(SenderMessage::ResumeRequest(flash_cat_common::proto::ResumeRequest {})),
                            }),
                        )
                        .await?;
                    }
                }
                RelayMessage::Ready(ready) => {
                    if relay_type != RelayType::Specify && !lifecycle.select_path(ready.local_relay) {
                        return Ok(());
                    }
                    if !request_sent {
                        request_sent = true;
                        Self::send_msg_to_stream(sender_stream_tx, SenderInteractionMessage::TransferMode(relay_type.clone())).await?;
                        send_msg_to_relay(
                            &tx,
                            RelayMessage::Sender(SenderUpdate {
                                sender_message: Some(SenderMessage::SendRequest(SendRequest {
                                    num_entries: file_collector.files.len() as u64,
                                    total_size: file_collector.total_size,
                                    num_files: file_collector.num_files,
                                    num_folders: file_collector.num_folders,
                                    max_file_name_length: file_collector.max_file_name_length as u64,
                                    file_request_window: FILE_REQUEST_WINDOW as u32,
                                })),
                            }),
                        )
                        .await?;
                    } else if share_accepted {
                        Self::stop_send_task(&mut send_task, &send_files_cancel).await;
                        while confirm_rx.try_recv().is_ok() {}
                        send_files_cancel = cancel.child_token();
                        send_msg_to_relay(
                            &tx,
                            RelayMessage::Sender(SenderUpdate {
                                sender_message: Some(SenderMessage::ResumeRequest(flash_cat_common::proto::ResumeRequest {})),
                            }),
                        )
                        .await?;
                    }
                }
                RelayMessage::Sender(_) => {
                    Self::send_msg_to_stream(
                        sender_stream_tx,
                        SenderInteractionMessage::Message("Invalid sender message".to_string()),
                    )
                    .await?;
                }
                RelayMessage::Receiver(receiver) => {
                    if let Some(receiver_message) = receiver.receiver_message {
                        match receiver_message {
                            ReceiverMessage::ShareConfirm(share_confirm) => {
                                if let Ok(confirm) = Confirm::try_from(share_confirm) {
                                    match confirm {
                                        Confirm::Accept => {
                                            if share_accepted {
                                                continue;
                                            }
                                            share_accepted = true;
                                            Self::stop_send_task(&mut send_task, &send_files_cancel).await;
                                            send_files_cancel = cancel.child_token();
                                            let encryptor = encryptor.clone();
                                            let file_collector = file_collector.clone();
                                            let tx = tx.clone();
                                            let sender_stream_tx = sender_stream_tx.clone();
                                            let notify_rx = confirm_rx.clone();
                                            let cancel = send_files_cancel.clone();
                                            send_task = Some(tokio::spawn(async move {
                                                if let Err(err) = Self::send_files(
                                                    encryptor,
                                                    tx,
                                                    file_collector,
                                                    notify_rx,
                                                    &sender_stream_tx,
                                                    cancel,
                                                    None,
                                                    file_request_window,
                                                )
                                                .await
                                                {
                                                    let _ = Self::send_msg_to_stream(
                                                        &sender_stream_tx,
                                                        SenderInteractionMessage::Error(format!("send files error {}", err)),
                                                    )
                                                    .await;
                                                }
                                            }));
                                        }
                                        Confirm::Reject => {
                                            send_msg_to_relay(&tx, RelayMessage::Done(Done {})).await?;
                                            Self::send_msg_to_stream(sender_stream_tx, SenderInteractionMessage::ReceiverReject).await?;
                                        }
                                    }
                                } else {
                                    Self::send_msg_to_stream(
                                        sender_stream_tx,
                                        SenderInteractionMessage::Error("try_from confirm failed".to_string()),
                                    )
                                    .await?;
                                }
                            }
                            ReceiverMessage::FileRequestWindow(window) => {
                                if !share_accepted {
                                    file_request_window = (window as usize).clamp(1, FILE_REQUEST_WINDOW);
                                }
                            }
                            ReceiverMessage::FileResult(result) => {
                                received_file_result = true;
                                let failure = result.status() == crate::FileStatus::Failed;
                                let error = result.error.clone();
                                Self::send_msg_to_stream(sender_stream_tx, SenderInteractionMessage::FileResult(result)).await?;
                                if failure {
                                    bail!("Receiver could not save file: {error}");
                                }
                            }
                            ReceiverMessage::FileConfirm(file_confirm) => {
                                confirm_tx.send(file_confirm).await?;
                            }
                            ReceiverMessage::ResumeState(resume_state) => {
                                if !share_accepted {
                                    continue;
                                }
                                Self::stop_send_task(&mut send_task, &send_files_cancel).await;
                                while confirm_rx.try_recv().is_ok() {}
                                send_files_cancel = cancel.child_token();
                                let mut resume_progress = HashMap::new();
                                for fp in resume_state.files {
                                    resume_progress.insert(fp.file_id, (fp.received_bytes, fp.completed, fp.source_identity));
                                }
                                let encryptor = encryptor.clone();
                                let file_collector = file_collector.clone();
                                let tx = tx.clone();
                                let sender_stream_tx = sender_stream_tx.clone();
                                let notify_rx = confirm_rx.clone();
                                let cancel = send_files_cancel.clone();
                                send_task = Some(tokio::spawn(async move {
                                    if let Err(err) = Self::send_files(
                                        encryptor,
                                        tx,
                                        file_collector,
                                        notify_rx,
                                        &sender_stream_tx,
                                        cancel,
                                        Some(resume_progress),
                                        file_request_window,
                                    )
                                    .await
                                    {
                                        let _ = Self::send_msg_to_stream(
                                            &sender_stream_tx,
                                            SenderInteractionMessage::Error(format!("send files error {}", err)),
                                        )
                                        .await;
                                    }
                                }));
                            }
                        }
                    }
                }
                RelayMessage::Done(_) => {
                    Self::report_completion(sender_stream_tx, &file_collector.files, share_accepted && !received_file_result).await?;
                }
                RelayMessage::Error(e) => {
                    Self::send_msg_to_stream(
                        sender_stream_tx,
                        SenderInteractionMessage::Error(format!("relay error {}", e.to_string())),
                    )
                    .await?;
                }
                RelayMessage::Terminated(_) => {
                    Self::stop_send_task(&mut send_task, &send_files_cancel).await;
                    Self::send_msg_to_stream(sender_stream_tx, SenderInteractionMessage::OtherClose).await?;
                    return Ok(());
                }
                RelayMessage::Ping(_) => (),
                RelayMessage::Pong(_) => (),
            }
        }
    }

    /// Older receivers only acknowledge the entire transfer. Infer per-file
    /// success only on that acknowledgement and only if no file result arrived.
    async fn report_completion(
        sender_stream_tx: &mpsc::Sender<SenderInteractionMessage>,
        files: &[FileInfo],
        infer_file_success: bool,
    ) -> Result<()> {
        if infer_file_success {
            for file in files {
                Self::send_msg_to_stream(
                    sender_stream_tx,
                    SenderInteractionMessage::FileResult(crate::FileResult {
                        file_id: file.file_id,
                        status: crate::FileStatus::Success as i32,
                        error: String::new(),
                    }),
                )
                .await?;
            }
        }
        Self::send_msg_to_stream(sender_stream_tx, SenderInteractionMessage::Completed).await
    }

    async fn stop_send_task(
        send_task: &mut Option<tokio::task::JoinHandle<()>>,
        cancel: &CancellationToken,
    ) {
        cancel.cancel();
        if let Some(task) = send_task.take() {
            task.abort();
            let _ = task.await;
        }
    }

    async fn send_files(
        encryptor: Arc<Encryptor>,
        tx: mpsc::Sender<RelayUpdate>,
        file_collector: Arc<FileCollector>,
        notify: async_channel::Receiver<FileConfirm>,
        sender_stream_tx: &mpsc::Sender<SenderInteractionMessage>,
        cancel: CancellationToken,
        resume_progress: Option<HashMap<u64, (u64, bool, String)>>,
        file_request_window: usize,
    ) -> Result<()> {
        let transfer = async {
            let mut progress = ProgressThrottle::default();
            let pool = ChunkPool::default();
            let resume_progress = resume_progress.unwrap_or_default();
            for file in &file_collector.files {
                if let Some((position, _, identity)) = resume_progress.get(&file.file_id) {
                    resume_position(file, *position, identity).await?;
                }
            }
            let files = file_collector
                .files
                .iter()
                .enumerate()
                .filter(|(_, file)| !resume_progress.get(&file.file_id).is_some_and(|(_, completed, _)| *completed))
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            let window = file_request_window.clamp(1, FILE_REQUEST_WINDOW);
            let mut confirmations = FileConfirmations::default();
            let mut requested = VecDeque::<usize>::new();
            let mut next = 0;
            let mut sent = 0;
            while sent < files.len() {
                // Metadata lookahead stays bounded and slides after each file;
                // preparing a request never scans file contents.
                while requested.len() < window && next < files.len() {
                    let index = files[next];
                    let file = &file_collector.files[index];
                    if requested.iter().any(|&pending| paths_overlap(&file.relative_path, &file_collector.files[pending].relative_path)) {
                        break;
                    }
                    if !resume_progress.contains_key(&file.file_id) {
                        confirmations.expected.insert(file.file_id);
                        Self::request_file(file, &tx).await?;
                    }
                    requested.push_back(index);
                    next += 1;
                }
                let index = requested.pop_front().ok_or_else(|| anyhow::anyhow!("Missing requested transfer file"))?;
                let file = &file_collector.files[index];
                Self::send_single_file(
                    file,
                    &encryptor,
                    &tx,
                    &notify,
                    sender_stream_tx,
                    &cancel,
                    resume_progress.get(&file.file_id).cloned(),
                    &mut progress,
                    &mut confirmations,
                    &pool,
                )
                .await?;
                sent += 1;
            }
            send_msg_to_relay(&tx, RelayMessage::Done(Done {})).await?;
            Self::send_msg_to_stream(sender_stream_tx, SenderInteractionMessage::SendDone).await?;
            Ok(())
        };
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Ok(()),
            // The relay receive loop owns disconnect/reconnect reporting. A
            // closed upload stream must not race its terminal notification.
            _ = tx.closed() => Ok(()),
            result = transfer => {
                if tx.is_closed() { Ok(()) } else { result }
            },
        }
    }

    async fn request_file(
        send_file: &FileInfo,
        tx: &mpsc::Sender<RelayUpdate>,
    ) -> Result<()> {
        validate_source(send_file).await?;
        send_msg_to_relay(
            tx,
            RelayMessage::Sender(SenderUpdate {
                sender_message: Some(SenderMessage::NewFileRequest(NewFileRequest {
                    file_id: send_file.file_id,
                    filename: send_file.name.clone(),
                    #[cfg(unix)]
                    file_mode: send_file.mode,
                    #[cfg(windows)]
                    file_mode: 0,
                    relative_path: send_file.relative_path.clone(),
                    total_size: send_file.size,
                    is_empty_dir: send_file.empty_dir,
                    source_identity: send_file.source_identity.clone(),
                })),
            }),
        )
        .await
    }

    async fn send_single_file(
        send_file: &FileInfo,
        encryptor: &Encryptor,
        tx: &mpsc::Sender<RelayUpdate>,
        notify: &async_channel::Receiver<FileConfirm>,
        sender_stream_tx: &mpsc::Sender<SenderInteractionMessage>,
        cancel: &CancellationToken,
        file_resume: Option<(u64, bool, String)>,
        progress: &mut ProgressThrottle,
        confirmations: &mut FileConfirmations,
        pool: &ChunkPool,
    ) -> Result<()> {
        // Resume: partial file — send BreakPoint and stream remaining data
        if let Some((received_bytes, _, identity)) = file_resume {
            if !send_file.empty_dir {
                let received_bytes = resume_position(send_file, received_bytes, &identity).await?;
                let _ = Self::send_msg_to_stream(
                    sender_stream_tx,
                    SenderInteractionMessage::Message(format!("Resuming file {} from {}", send_file.name, received_bytes)),
                )
                .await;

                send_msg_to_relay(
                    tx,
                    RelayMessage::Sender(SenderUpdate {
                        sender_message: Some(SenderMessage::BreakPoint(BreakPoint {
                            file_id: send_file.file_id,
                            position: received_bytes,
                        })),
                    }),
                )
                .await?;

                Self::stream_file_data(
                    send_file,
                    encryptor,
                    tx,
                    sender_stream_tx,
                    cancel,
                    received_bytes,
                    progress,
                    pool,
                )
                .await?;
                return Ok(());
            }
        }

        Self::send_msg_to_stream(
            sender_stream_tx,
            SenderInteractionMessage::FileStage(FileStage {
                file_id: send_file.file_id,
                phase: TransferPhase::Waiting,
                position: 0,
            }),
        )
        .await?;
        let file_confirm = tokio::time::timeout(FILE_CONFIRM_TIMEOUT, confirmations.wait(send_file.file_id, notify)).await.map_err(|_| {
            anyhow::anyhow!(
                "timed out waiting for receiver confirmation for file {} after {}s",
                send_file.file_id,
                FILE_CONFIRM_TIMEOUT.as_secs()
            )
        })??;

        let mut position = 0;

        if let Some(confirm_message) = file_confirm.confirm_message {
            match confirm_message {
                ConfirmMessage::NewFileConfirm(new_file_confirm) => {
                    if new_file_confirm.file_id != send_file.file_id {
                        Self::send_msg_to_stream(
                            sender_stream_tx,
                            SenderInteractionMessage::Error("File order is wrong".to_string()),
                        )
                        .await?;
                        return Ok(());
                    }
                    if new_file_confirm.confirm == Confirm::Reject.into() {
                        return Ok(());
                    }
                }
                ConfirmMessage::BreakPointConfirm(break_point_confirm) => {
                    if break_point_confirm.file_id != send_file.file_id {
                        Self::send_msg_to_stream(
                            sender_stream_tx,
                            SenderInteractionMessage::Error("File order is wrong".to_string()),
                        )
                        .await?;
                        return Ok(());
                    }
                    if break_point_confirm.confirm == Confirm::Accept.into() {
                        position = resume_position(send_file, break_point_confirm.position, &break_point_confirm.source_identity).await?;
                        send_msg_to_relay(
                            tx,
                            RelayMessage::Sender(SenderUpdate {
                                sender_message: Some(SenderMessage::BreakPoint(BreakPoint {
                                    file_id: send_file.file_id,
                                    position,
                                })),
                            }),
                        )
                        .await?;
                    }
                }
            }
        }

        if send_file.empty_dir {
            return Ok(());
        }

        Self::stream_file_data(send_file, encryptor, tx, sender_stream_tx, cancel, position, progress, pool).await
    }

    async fn stream_file_data(
        send_file: &FileInfo,
        encryptor: &Encryptor,
        tx: &mpsc::Sender<RelayUpdate>,
        sender_stream_tx: &mpsc::Sender<SenderInteractionMessage>,
        cancel: &CancellationToken,
        start_position: u64,
        progress: &mut ProgressThrottle,
        pool: &ChunkPool,
    ) -> Result<()> {
        validate_source(send_file).await?;
        Self::send_msg_to_stream(
            sender_stream_tx,
            SenderInteractionMessage::FileStage(FileStage {
                file_id: send_file.file_id,
                phase: TransferPhase::Transferring,
                position: start_position,
            }),
        )
        .await?;
        let mut chunks = FileChunks::new(
            send_file.access_path.clone(),
            start_position,
            send_file.size.saturating_sub(start_position),
            pool.clone(),
        );
        let mut position = start_position;
        loop {
            let chunk = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Ok(()),
                result = chunks.next() => result?,
            };
            let Some(mut chunk) = chunk else {
                if position != send_file.size {
                    bail!("Source file size changed: {}", send_file.access_path);
                }
                validate_source(send_file).await?;
                send_msg_to_relay(
                    tx,
                    RelayMessage::Sender(SenderUpdate {
                        sender_message: Some(SenderMessage::FileDone(FileDone {
                            file_id: send_file.file_id,
                        })),
                    }),
                )
                .await?;
                Self::send_msg_to_stream(
                    sender_stream_tx,
                    SenderInteractionMessage::FileProgress(Progress {
                        file_id: send_file.file_id,
                        position,
                    }),
                )
                .await?;
                Self::send_msg_to_stream(
                    sender_stream_tx,
                    SenderInteractionMessage::FileStage(FileStage {
                        file_id: send_file.file_id,
                        phase: TransferPhase::Saving,
                        position,
                    }),
                )
                .await?;
                return Ok(());
            };
            let read_length = chunk.data.len();
            encryptor.encrypt_in_place(&mut chunk.data)?;
            send_msg_to_relay(
                tx,
                RelayMessage::Sender(SenderUpdate {
                    sender_message: Some(SenderMessage::FileData(FileData {
                        file_id: send_file.file_id,
                        data: chunk.into_bytes(),
                    })),
                }),
            )
            .await?;
            position += read_length as u64;
            progress.report(
                sender_stream_tx,
                SenderInteractionMessage::FileProgress(Progress {
                    file_id: send_file.file_id,
                    position,
                }),
            )?;
        }
    }

    pub fn get_file_collector(&self) -> Arc<FileCollector> {
        self.file_collector.clone()
    }

    pub fn shutdown(&self) {
        self.lifecycle.cancel();
        let _ = self.clean_zip_files();
    }

    pub async fn terminated(&self) {
        self.lifecycle.root.cancelled().await
    }

    /// Wait until relay channel tasks have finished their shutdown handshake.
    pub async fn shutdown_complete(&self) {
        self.lifecycle.wait().await;
    }

    fn clean_zip_files(&self) -> Result<()> {
        for dir in &self.zip_dirs {
            match std::fs::remove_dir_all(dir.path()) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    async fn zip_folder(
        mut files: Vec<String>,
        cancel: CancellationToken,
    ) -> Result<(Vec<String>, Vec<Arc<tempfile::TempDir>>)> {
        let mut async_task = vec![];
        let mut zip_dirs = vec![];
        for i in 0..files.len() {
            let p = files[i].as_str();
            if is_idr(p) {
                let source = std::fs::canonicalize(p)?;
                let archive_name = format!("{}.zip", source.file_name().unwrap_or_default().to_string_lossy());
                let zip_dir = Arc::new(tempfile::Builder::new().prefix("flash-cat-zip-").tempdir()?);
                let file_name = zip_dir.path().join(archive_name).to_string_lossy().into_owned();
                let path = p.to_owned();
                let file_name_for_task = file_name.clone();
                let zip_dir_for_task = zip_dir.clone();
                let cancel_clone = cancel.clone();
                async_task.push(tokio::spawn(async move {
                    let shutdown = Shutdown::new();
                    let shutdown_on_cancel = shutdown.clone();
                    let cancel_task = tokio::spawn(async move {
                        cancel_clone.cancelled().await;
                        shutdown_on_cancel.shutdown();
                    });
                    let result = tokio::task::spawn_blocking(move || {
                        // Keep the directory alive even if the awaiting task is cancelled.
                        let _zip_dir = zip_dir_for_task;
                        zip_folder(file_name_for_task, path, shutdown)
                    })
                    .await;
                    cancel_task.abort();
                    result?
                }));
                zip_dirs.push(zip_dir);
                files[i] = file_name;
            }
        }

        let sigint = ctrl_c();
        let signal_cancel = cancel.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = sigint => (),
            }
            signal_cancel.cancel();
        });

        for task in async_task {
            task.await??;
        }
        if cancel.is_cancelled() {
            bail!("folder compression cancelled");
        }
        Ok((files, zip_dirs))
    }

    async fn send_msg_to_stream(
        tx: &mpsc::Sender<SenderInteractionMessage>,
        msg: SenderInteractionMessage,
    ) -> Result<()> {
        tx.send(msg).await?;
        Ok(())
    }
}

/// Normalize components for collision checks without resolving sender-side paths.
fn paths_overlap(
    left: &str,
    right: &str,
) -> bool {
    // Also be conservative for transfers to case-insensitive receivers.
    let left = left.replace('\\', "/").to_lowercase();
    let right = right.replace('\\', "/").to_lowercase();
    let left = Path::new(&left);
    let right = Path::new(&right);
    left.starts_with(right) || right.starts_with(left)
}

#[derive(Default)]
struct FileConfirmations {
    expected: HashSet<u64>,
    buffered: HashMap<u64, FileConfirm>,
}

impl FileConfirmations {
    async fn wait(
        &mut self,
        file_id: u64,
        notify: &async_channel::Receiver<FileConfirm>,
    ) -> Result<FileConfirm> {
        if let Some(confirm) = self.buffered.remove(&file_id) {
            self.expected.remove(&file_id);
            return Ok(confirm);
        }
        loop {
            let confirm = notify.recv().await.map_err(|_| anyhow::anyhow!("confirm channel closed for file {file_id}"))?;
            let id = match &confirm.confirm_message {
                Some(ConfirmMessage::NewFileConfirm(msg)) => msg.file_id,
                Some(ConfirmMessage::BreakPointConfirm(msg)) => msg.file_id,
                None => bail!("empty file confirmation"),
            };
            if !self.expected.contains(&id) {
                bail!("unexpected confirmation for file {id}");
            }
            if id == file_id {
                self.expected.remove(&file_id);
                return Ok(confirm);
            }
            self.buffered.insert(id, confirm);
        }
    }
}

async fn resume_position(
    file: &FileInfo,
    position: u64,
    identity: &str,
) -> Result<u64> {
    if position > file.size {
        bail!("Resume position exceeds source file size");
    }
    let (identity, proof) = identity.split_once(":sha256:").map_or((identity, None), |(identity, proof)| (identity, Some(proof)));
    if identity != file.source_identity {
        bail!("Resume metadata differs from source file: {}", file.name);
    }
    if let Some(proof) = proof {
        validate_source(file).await?;
        let actual = prefix_digest(tokio::fs::File::open(&file.access_path).await?, position).await?;
        if proof != actual {
            bail!(
                "Cannot resume {}: existing content differs from the source. Receive again and choose restart.",
                file.name
            );
        }
        validate_source(file).await?;
    }
    Ok(position)
}

async fn validate_source(file: &FileInfo) -> Result<()> {
    if file.empty_dir {
        return Ok(());
    }
    let metadata = tokio::fs::metadata(&file.access_path).await?;
    if !metadata.is_file() || metadata.len() != file.size || file_identity(&metadata)? != file.source_identity {
        bail!("Source file changed during transfer: {}", file.access_path);
    }
    Ok(())
}

struct CancelScanOnDrop(Shutdown);
impl Drop for CancelScanOnDrop {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}
