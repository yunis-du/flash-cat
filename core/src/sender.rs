use std::{collections::HashMap, net::SocketAddr, path::Path, pin::Pin, sync::Arc, time::Duration};

use anyhow::{Result, bail};
use bytes::{Bytes, BytesMut};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, SeekFrom},
    signal::ctrl_c,
    sync::{mpsc, oneshot, Semaphore},
};
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::transport::Endpoint;

use flash_cat_common::{
    Shutdown, compare_versions,
    consts::{DEFAULT_RELAY_PORT, PUBLIC_RELAY, SEND_BUFF_SIZE},
    crypt::encryptor::Encryptor,
    proto::{
        BreakPoint, Character, ClientType, CloseRequest, Confirm, Done, FileConfirm, FileData, FileDone, Id, JoinRequest, NewFileRequest, RelayInfo,
        RelayUpdate, SendRequest, SenderUpdate, file_confirm::ConfirmMessage, join_response, receiver_update::ReceiverMessage,
        relay_service_client::RelayServiceClient, relay_update::RelayMessage, sender_update::SenderMessage,
    },
    utils::{
        fs::{FileCollector, FileInfo, collect_files, is_idr, paths_exist, remove_files, zip_folder},
        net::{find_available_port, get_local_ip, net_scout::NetScout},
    },
};
use flash_cat_relay::{built_info, relay::Relay};

use crate::{PING_INTERVAL, Progress, RelayType, SenderInteractionMessage, get_endpoint, send_msg_to_relay};

/// Broadcast local relay addr timeout.
pub const BROADCAST_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum number of files to transfer concurrently.
pub const MAX_CONCURRENT_FILES: usize = 3;

/// Sender stream
pub type SenderStream = Pin<Box<dyn Stream<Item = SenderInteractionMessage> + Send>>;

#[derive(Debug, Clone)]
pub struct FlashCatSender {
    zip_files: Vec<String>,
    encryptor: Arc<Encryptor>,
    specify_relay: Option<String>,
    file_collector: Arc<FileCollector>,
    local_relay_shutdown: Shutdown,
    public_relay_shutdown: Shutdown,
    client_type: ClientType,
    shutdown: Shutdown,
}

impl FlashCatSender {
    pub async fn new(
        share_code: String,
        specify_relay: Option<String>,
        mut files: Vec<String>,
        zip_floder: bool,
        client_type: ClientType,
    ) -> Result<Self> {
        paths_exist(files.as_slice())?;
        let shutdown = Shutdown::new();
        let mut zip_files = vec![];
        if zip_floder {
            let (treated_files, zip) = Self::zip_folder(files, shutdown.clone()).await?;
            files = treated_files;
            zip_files = zip;
        }
        let file_collector = collect_files(files.as_slice());
        let encryptor = Arc::new(Encryptor::new(share_code)?);
        Ok(Self {
            zip_files,
            encryptor,
            specify_relay,
            file_collector: Arc::new(file_collector),
            local_relay_shutdown: Shutdown::new(),
            public_relay_shutdown: Shutdown::new(),
            client_type,
            shutdown,
        })
    }

    pub fn new_with_file_collector(
        share_code: String,
        specify_relay: Option<String>,
        file_collector: FileCollector,
        client_type: ClientType,
    ) -> Result<Self> {
        let shutdown = Shutdown::new();
        let encryptor = Arc::new(Encryptor::new(share_code)?);
        Ok(Self {
            zip_files: vec![],
            encryptor,
            specify_relay,
            file_collector: Arc::new(file_collector),
            local_relay_shutdown: Shutdown::new(),
            public_relay_shutdown: Shutdown::new(),
            client_type,
            shutdown,
        })
    }

    pub async fn start(self: Arc<Self>) -> Result<SenderStream> {
        let (sender_stream_tx, mut sender_stream_rx) = mpsc::channel(128);

        if self.specify_relay.is_some() {
            let specify_relay = self.specify_relay.clone().unwrap();
            let specify_relay_addr = match specify_relay.parse() {
                Ok(specify_relay_addr) => {
                    let specify_relay_addr: SocketAddr = specify_relay_addr;
                    format!("http://{specify_relay_addr}")
                }
                Err(_) => specify_relay,
            };
            let endpoint = get_endpoint(specify_relay_addr)?;
            self.connect_relay(
                RelayType::Specify,
                None,
                endpoint,
                sender_stream_tx.clone(),
                self.shutdown.clone(),
                self.local_relay_shutdown.clone(),
            )
            .await?;
        } else {
            // start local relay
            let local_relay_port = find_available_port(DEFAULT_RELAY_PORT);
            self.start_loacl_relay(
                format!("0.0.0.0:{}", local_relay_port).parse().unwrap(),
                sender_stream_tx.clone(),
                self.local_relay_shutdown.clone(),
            )
            .await;

            // waite for local relay start
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // connect local relay
            let endpoint = get_endpoint(format!("http://127.0.0.1:{local_relay_port}"))?;
            self.connect_relay(
                RelayType::Local,
                None,
                endpoint,
                sender_stream_tx.clone(),
                self.public_relay_shutdown.clone(),
                self.local_relay_shutdown.clone(),
            )
            .await?;

            // connect public relay
            let endpoint = get_endpoint(format!("https://{PUBLIC_RELAY}"))?;
            self.connect_relay(
                RelayType::Public,
                Some(local_relay_port),
                endpoint,
                sender_stream_tx.clone(),
                self.public_relay_shutdown.clone(),
                self.local_relay_shutdown.clone(),
            )
            .await?;

            // broadcast
            self.broadcast_relay_addr(local_relay_port, sender_stream_tx.clone()).await;
        }
        // resolve shutdown when sender_stream_rx is no message will cause panic
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        Ok(Box::pin(async_stream::stream! {
            while !self.shutdown.is_terminated() {
                tokio::select! {
                    Some(sender_stream) = sender_stream_rx.recv() => {
                        yield sender_stream;
                    }
                    _ = interval.tick() =>(),
                }
            }
        }))
    }

    async fn start_loacl_relay(
        &self,
        local_relay_addr: SocketAddr,
        sender_stream_tx: mpsc::Sender<SenderInteractionMessage>,
        local_relay_shutdown: Shutdown,
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
                tokio::select! {
                    () = local_relay_shutdown.wait() => (),
                    else => return,
                }
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
    ) {
        let shutdown = Shutdown::new();
        let match_content = self.encryptor.encrypt_share_code_bytes().to_vec();
        tokio::spawn(async move {
            let mut net_scout = NetScout::new(match_content, BROADCAST_TIMEOUT, shutdown.clone());
            if let Err(e) = net_scout.broadcast(local_relay_port).await {
                let _ = &sender_stream_tx.send(SenderInteractionMessage::Error(format!("broadcast error {}", e.to_string()))).await;
            }
        });
    }

    async fn connect_relay(
        &self,
        relay_type: RelayType,
        local_relay_port: Option<u16>,
        mut endpoint: Endpoint,
        sender_stream_tx: mpsc::Sender<SenderInteractionMessage>,
        public_or_specify_shutdown: Shutdown,
        local_relay_shutdown: Shutdown,
    ) -> Result<()> {
        let mut client = RelayServiceClient::connect(endpoint.clone()).await?;

        let sender_local_relay = if relay_type == RelayType::Public && local_relay_port.is_some() {
            match get_local_ip() {
                Some(ip) => Some(RelayInfo {
                    relay_ip: ip.to_string(),
                    relay_port: local_relay_port.unwrap() as u32,
                }),
                None => None,
            }
        } else {
            None
        };

        let resp = match client
            .join(JoinRequest {
                id: Some(Id {
                    encrypted_share_code: self.encryptor.encrypt_share_code_bytes(),
                    character: Character::Sender.into(),
                }),
                client_type: self.client_type.into(),
                sender_local_relay,
            })
            .await
        {
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
        tokio::spawn(async move {
            if let Err(e) = Self::relay_channel(
                relay_type.clone(),
                encryptor,
                file_collector.clone(),
                endpoint,
                &sender_stream_tx,
                public_or_specify_shutdown,
                local_relay_shutdown,
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

        let (tx, rx) = mpsc::channel(32);

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
        public_or_specify_shutdown: Shutdown,
        local_relay_shutdown: Shutdown,
    ) -> Result<()> {
        let (mut client, mut tx, mut messages, mut confirm_tx, mut confirm_rx) = Self::establish_channel(&encryptor, &endpoint).await?;

        let shutdown = match relay_type {
            RelayType::Local => local_relay_shutdown.clone(),
            _ => public_or_specify_shutdown.clone(),
        };

        let mut ping_interval = tokio::time::interval(PING_INTERVAL);
        let mut reconnect_attempt = 0u32;
        let mut is_first_connect = true;
        let mut send_files_shutdown = Shutdown::new();
        loop {
            let message = tokio::select! {
                _ = shutdown.wait() => {
                    send_files_shutdown.shutdown();
                    let _ = client.close(CloseRequest {
                        encrypted_share_code: encryptor.encrypt_share_code_bytes(),
                    })
                    .await;
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
                            if shutdown.is_terminated() {
                                return Ok(());
                            }
                            send_files_shutdown.shutdown();

                            let result = loop {
                                if !crate::should_retry(reconnect_attempt) {
                                    bail!("max reconnect retries exceeded");
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
                                tokio::time::sleep(delay).await;
                                reconnect_attempt += 1;

                                if shutdown.is_terminated() {
                                    return Ok(());
                                }

                                match Self::establish_channel(&encryptor, &endpoint).await {
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
                            send_files_shutdown = Shutdown::new();
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
                    // After reconnection, send ResumeRequest instead of waiting for Ready
                    if !is_first_connect {
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
                    if ready.local_relay {
                        public_or_specify_shutdown.shutdown();
                    } else {
                        local_relay_shutdown.shutdown();
                    }
                    send_msg_to_relay(
                        &tx,
                        RelayMessage::Sender(SenderUpdate {
                            sender_message: Some(SenderMessage::SendRequest(SendRequest {
                                total_size: file_collector.total_size,
                                num_files: file_collector.num_files,
                                num_folders: file_collector.num_folders,
                                max_file_name_length: file_collector.max_file_name_length as u64,
                            })),
                        }),
                    )
                    .await?;
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
                                            let encryptor = encryptor.clone();
                                            let file_collector = file_collector.clone();
                                            let tx = tx.clone();
                                            let sender_stream_tx = sender_stream_tx.clone();
                                            let notify_rx = confirm_rx.clone();
                                            let cancel = send_files_shutdown.clone();
                                            tokio::spawn(async move {
                                                if let Err(err) =
                                                    Self::send_files(encryptor, tx, file_collector, notify_rx, &sender_stream_tx, cancel, None).await
                                                {
                                                    let _ = Self::send_msg_to_stream(
                                                        &sender_stream_tx,
                                                        SenderInteractionMessage::Error(format!("send files error {}", err)),
                                                    )
                                                    .await;
                                                }
                                            });
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
                            ReceiverMessage::FileConfirm(file_confirm) => {
                                confirm_tx.send(file_confirm).await?;
                            }
                            ReceiverMessage::ResumeState(resume_state) => {
                                let mut resume_progress = HashMap::new();
                                for fp in resume_state.files {
                                    resume_progress.insert(fp.file_id, (fp.received_bytes, fp.completed));
                                }
                                let encryptor = encryptor.clone();
                                let file_collector = file_collector.clone();
                                let tx = tx.clone();
                                let sender_stream_tx = sender_stream_tx.clone();
                                let notify_rx = confirm_rx.clone();
                                let cancel = send_files_shutdown.clone();
                                tokio::spawn(async move {
                                    if let Err(err) = Self::send_files(
                                        encryptor,
                                        tx,
                                        file_collector,
                                        notify_rx,
                                        &sender_stream_tx,
                                        cancel,
                                        Some(resume_progress),
                                    )
                                    .await
                                    {
                                        let _ = Self::send_msg_to_stream(
                                            &sender_stream_tx,
                                            SenderInteractionMessage::Error(format!("send files error {}", err)),
                                        )
                                        .await;
                                    }
                                });
                            }
                        }
                    }
                }
                RelayMessage::Done(_) => {
                    Self::send_msg_to_stream(sender_stream_tx, SenderInteractionMessage::Completed).await?;
                }
                RelayMessage::Error(e) => {
                    Self::send_msg_to_stream(
                        sender_stream_tx,
                        SenderInteractionMessage::Error(format!("relay error {}", e.to_string())),
                    )
                    .await?;
                }
                RelayMessage::Terminated(_) => {
                    Self::send_msg_to_stream(sender_stream_tx, SenderInteractionMessage::OtherClose).await?;
                }
                RelayMessage::Ping(_) => (),
                RelayMessage::Pong(_) => (),
            }
        }
    }

    fn extract_confirm_file_id(confirm: &FileConfirm) -> Option<u64> {
        confirm.confirm_message.as_ref().map(|msg| match msg {
            ConfirmMessage::NewFileConfirm(c) => c.file_id,
            ConfirmMessage::BreakPointConfirm(c) => c.file_id,
        })
    }

    async fn send_files(
        encryptor: Arc<Encryptor>,
        tx: mpsc::Sender<RelayUpdate>,
        file_collector: Arc<FileCollector>,
        notify: async_channel::Receiver<FileConfirm>,
        sender_stream_tx: &mpsc::Sender<SenderInteractionMessage>,
        cancel: Shutdown,
        resume_progress: Option<HashMap<u64, (u64, bool)>>,
    ) -> Result<()> {
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_FILES));
        let confirm_waiters: Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<FileConfirm>>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));

        let confirm_waiters_ref = confirm_waiters.clone();
        let cancel_ref = cancel.clone();
        let dispatcher = tokio::spawn(async move {
            loop {
                if cancel_ref.is_terminated() {
                    return;
                }
                match notify.recv().await {
                    Ok(file_confirm) => {
                        if let Some(file_id) = Self::extract_confirm_file_id(&file_confirm) {
                            let sender = confirm_waiters_ref.lock().unwrap().remove(&file_id);
                            if let Some(sender) = sender {
                                let _ = sender.send(file_confirm);
                            }
                        }
                    }
                    Err(_) => return,
                }
            }
        });

        let mut tasks = Vec::new();

        for send_file in file_collector.files.iter() {
            if cancel.is_terminated() {
                break;
            }

            if let Some(ref progress) = resume_progress {
                if let Some(&(_, completed)) = progress.get(&send_file.file_id) {
                    if completed {
                        continue;
                    }
                }
            }

            let file_resume = resume_progress
                .as_ref()
                .and_then(|p| p.get(&send_file.file_id).copied());

            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| anyhow::anyhow!("semaphore closed: {}", e))?;

            let send_file = send_file.clone();
            let encryptor = encryptor.clone();
            let tx = tx.clone();
            let sender_stream_tx = sender_stream_tx.clone();
            let cancel = cancel.clone();
            let confirm_waiters = confirm_waiters.clone();

            let task = tokio::spawn(async move {
                let result = Self::send_single_file(
                    &send_file,
                    &encryptor,
                    &tx,
                    &sender_stream_tx,
                    &cancel,
                    &confirm_waiters,
                    file_resume,
                )
                .await;
                drop(permit);
                result
            });
            tasks.push(task);
        }

        let mut first_error = None;
        for task in tasks {
            match task.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
                Err(e) => {
                    if first_error.is_none() {
                        first_error = Some(anyhow::anyhow!("task panicked: {}", e));
                    }
                }
            }
        }

        dispatcher.abort();

        if let Some(e) = first_error {
            return Err(e);
        }

        send_msg_to_relay(&tx, RelayMessage::Done(Done {})).await?;
        Self::send_msg_to_stream(sender_stream_tx, SenderInteractionMessage::SendDone).await?;
        Ok(())
    }

    async fn send_single_file(
        send_file: &FileInfo,
        encryptor: &Encryptor,
        tx: &mpsc::Sender<RelayUpdate>,
        sender_stream_tx: &mpsc::Sender<SenderInteractionMessage>,
        cancel: &Shutdown,
        confirm_waiters: &std::sync::Mutex<HashMap<u64, oneshot::Sender<FileConfirm>>>,
        file_resume: Option<(u64, bool)>,
    ) -> Result<()> {
        // Resume: partial file — send BreakPoint and stream remaining data
        if let Some((received_bytes, _)) = file_resume {
            if received_bytes > 0 && !send_file.empty_dir {
                let _ = Self::send_msg_to_stream(
                    sender_stream_tx,
                    SenderInteractionMessage::Message(format!(
                        "Resuming file {} from {}",
                        send_file.name, received_bytes
                    )),
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

                Self::stream_file_data(send_file, encryptor, tx, sender_stream_tx, cancel, received_bytes).await?;
                return Ok(());
            }
        }

        // Normal flow: send NewFileRequest, wait for confirm, then stream data
        let (confirm_tx, confirm_rx) = oneshot::channel();
        confirm_waiters
            .lock()
            .unwrap()
            .insert(send_file.file_id, confirm_tx);

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
                })),
            }),
        )
        .await?;

        let file_confirm = confirm_rx
            .await
            .map_err(|_| anyhow::anyhow!("confirm channel closed for file {}", send_file.file_id))?;

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
                        Self::send_msg_to_stream(
                            sender_stream_tx,
                            SenderInteractionMessage::ContinueFile(new_file_confirm.file_id),
                        )
                        .await?;
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
                        position = break_point_confirm.position;
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

        Self::stream_file_data(send_file, encryptor, tx, sender_stream_tx, cancel, position).await
    }

    async fn stream_file_data(
        send_file: &FileInfo,
        encryptor: &Encryptor,
        tx: &mpsc::Sender<RelayUpdate>,
        sender_stream_tx: &mpsc::Sender<SenderInteractionMessage>,
        cancel: &Shutdown,
        start_position: u64,
    ) -> Result<()> {
        let mut read_file = File::open(send_file.access_path.as_str()).await?;
        read_file.seek(SeekFrom::Start(start_position)).await?;
        let mut position = start_position;
        loop {
            if cancel.is_terminated() {
                return Ok(());
            }
            let mut buffer = BytesMut::with_capacity(SEND_BUFF_SIZE);
            let read_length = read_file.read_buf(&mut buffer).await?;
            if read_length == 0 {
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
                    SenderInteractionMessage::FileProgressFinish(send_file.file_id),
                )
                .await?;
                return Ok(());
            }
            send_msg_to_relay(
                tx,
                RelayMessage::Sender(SenderUpdate {
                    sender_message: Some(SenderMessage::FileData(FileData {
                        file_id: send_file.file_id,
                        data: Bytes::from(encryptor.encrypt(buffer.as_ref())?),
                    })),
                }),
            )
            .await?;
            position += read_length as u64;
            Self::send_msg_to_stream(
                sender_stream_tx,
                SenderInteractionMessage::FileProgress(Progress {
                    file_id: send_file.file_id,
                    position,
                }),
            )
            .await?;
        }
    }

    pub fn get_file_collector(&self) -> Arc<FileCollector> {
        self.file_collector.clone()
    }

    pub fn shutdown(&self) {
        let _ = self.clean_zip_files();
        self.local_relay_shutdown.shutdown();
        self.public_relay_shutdown.shutdown();
        self.shutdown.shutdown();
    }

    pub async fn terminated(&self) {
        self.shutdown.wait().await
    }

    fn clean_zip_files(&self) -> Result<()> {
        remove_files(self.zip_files.as_slice())
    }

    async fn zip_folder(
        mut files: Vec<String>,
        shutdown: Shutdown,
    ) -> Result<(Vec<String>, Vec<String>)> {
        let mut async_task = vec![];
        let mut zip_files = vec![];
        for i in 0..files.len() {
            let p = files[i].as_str();
            if is_idr(p) {
                let file_name = format!("{}.zip", Path::new(p).file_name().unwrap_or_default().to_string_lossy());
                let path = p.to_owned();
                let file_name_for_task = file_name.clone();
                let shutdown_clone = shutdown.clone();
                async_task.push(tokio::spawn(
                    async move { zip_folder(file_name_for_task, path, shutdown_clone) },
                ));
                zip_files.push(file_name.clone());
                files[i] = file_name;
            }
        }

        let sigint = ctrl_c();
        tokio::spawn(async move {
            tokio::select! {
                _ = sigint => (),
            }
            shutdown.shutdown();
        });

        for task in async_task {
            task.await??;
        }
        Ok((files, zip_files))
    }

    async fn send_msg_to_stream(
        tx: &mpsc::Sender<SenderInteractionMessage>,
        msg: SenderInteractionMessage,
    ) -> Result<()> {
        tx.send(msg).await?;
        Ok(())
    }
}
