use std::{net::SocketAddr, path::Path, pin::Pin, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use bytes::{Bytes, BytesMut};
use flash_cat_common::{
    compare_versions,
    consts::{DEFAULT_RELAY_PORT, PUBLIC_RELAY},
    crypt::encryptor::Encryptor,
    proto::{
        join_response, receiver_update::ReceiverMessage, relay_service_client::RelayServiceClient,
        relay_update::RelayMessage, sender_update::SenderMessage, Character, ClientType,
        CloseRequest, Confirm, Done, FileConfirm, FileData, FileDone, Id, JoinRequest,
        NewFileRequest, RelayInfo, RelayUpdate, SendRequest, SenderUpdate,
    },
    utils::{
        fs::{collect_files, is_idr, paths_exist, remove_files, zip_folder, FileCollector},
        get_time_ms,
        net::{find_available_port, get_local_ip, net_scout::NetScout},
    },
    Shutdown,
};
use flash_cat_relay::{built_info, relay::Relay};
use tokio::{fs::File, io::AsyncReadExt, sync::mpsc, time::MissedTickBehavior};
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::transport::Endpoint;

use crate::{Progress, RelayType, SenderInteractionMessage, PING_INTERVAL};

/// Broadcast local relay addr timeout.
pub const BROADCAST_TIMEOUT: Duration = Duration::from_secs(60);

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
        let (sender_stream_tx, mut sender_stream_rx) = mpsc::channel(1024);

        if self.specify_relay.is_some() {
            let specify_relay = self.specify_relay.clone().unwrap();
            let specify_relay_addr = match specify_relay.parse() {
                Ok(specify_relay_addr) => {
                    let specify_relay_addr: SocketAddr = specify_relay_addr;
                    format!("http://{specify_relay_addr}")
                }
                Err(_) => specify_relay,
            };
            let endpoint = Endpoint::from_shared(specify_relay_addr)?;
            self.connect_relay(
                RelayType::Specify,
                None,
                endpoint,
                sender_stream_tx.clone(),
                self.shutdown.clone(),
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
            let endpoint = Endpoint::from_shared(format!("http://127.0.0.1:{local_relay_port}"))?;
            self.connect_relay(
                RelayType::Local,
                None,
                endpoint,
                sender_stream_tx.clone(),
                self.local_relay_shutdown.clone(),
            )
            .await?;

            // connect public relay
            let endpoint = Endpoint::from_shared(format!("https://{PUBLIC_RELAY}"))?;
            self.connect_relay(
                RelayType::Public,
                Some(local_relay_port),
                endpoint,
                sender_stream_tx.clone(),
                self.public_relay_shutdown.clone(),
            )
            .await?;

            // broadcast
            self.broadcast_relay_addr(local_relay_port, sender_stream_tx.clone())
                .await;
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
            let relay = match Relay::new(None) {
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
                let _ = &sender_stream_tx
                    .send(SenderInteractionMessage::Error(format!(
                        "broadcast error {}",
                        e.to_string()
                    )))
                    .await;
            }
        });
    }

    async fn connect_relay(
        &self,
        relay_type: RelayType,
        local_relay_port: Option<u16>,
        mut endpoint: Endpoint,
        sender_stream_tx: mpsc::Sender<SenderInteractionMessage>,
        shutdown: Shutdown,
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
                    SenderInteractionMessage::RelayFailed((
                        relay_type,
                        status.message().to_string(),
                    )),
                )
                .await;
                return Ok(());
            }
        };

        let (relay, client_latest_version) =
            if let Some(join_response_message) = resp.into_inner().join_response_message {
                match join_response_message {
                    join_response::JoinResponseMessage::Success(join_success) => {
                        (join_success.relay, join_success.client_latest_version)
                    }
                    join_response::JoinResponseMessage::Failed(join_failed) => {
                        return Err(anyhow::Error::msg(join_failed.error_msg))
                    }
                }
            } else {
                return Err(anyhow::Error::msg("can't get relay info"));
            };

        match self.client_type {
            ClientType::Cli => {
                if compare_versions(client_latest_version.as_str(), built_info::PKG_VERSION)
                    == std::cmp::Ordering::Greater
                {
                    let _ = sender_stream_tx
                        .send(SenderInteractionMessage::Message(format!(
                            "newly cli version[{}] is available",
                            client_latest_version
                        )))
                        .await;
                }
            }
            ClientType::App => {
                if compare_versions(client_latest_version.as_str(), built_info::PKG_VERSION)
                    == std::cmp::Ordering::Greater
                {
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
                endpoint = Endpoint::from_shared(format!(
                    "http://{}:{}",
                    relay_info.relay_ip, relay_info.relay_port
                ))?;
            }
            None => (),
        }

        let encryptor = self.encryptor.clone();
        let file_collector = self.file_collector.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::relay_channel(
                encryptor,
                file_collector.clone(),
                endpoint,
                &sender_stream_tx,
                shutdown,
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

    async fn relay_channel(
        encryptor: Arc<Encryptor>,
        file_collector: Arc<FileCollector>,
        endpoint: Endpoint,
        sender_stream_tx: &mpsc::Sender<SenderInteractionMessage>,
        shutdown: Shutdown,
    ) -> Result<()> {
        let mut client = RelayServiceClient::connect(endpoint).await?;

        let (tx, rx) = mpsc::channel(1024);

        let join = RelayMessage::Join(Id {
            encrypted_share_code: encryptor.encrypt_share_code_bytes(),
            character: Character::Sender.into(),
        });
        tx.send(RelayUpdate {
            relay_message: Some(join),
        })
        .await?;

        let resp = client.channel(ReceiverStream::new(rx)).await?;
        let mut messages = resp.into_inner(); // A stream of relay messages.

        let (confirm_tx, confirm_rx) = async_channel::bounded(10);

        let mut ping_interval = tokio::time::interval(PING_INTERVAL);
        ping_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            let message = tokio::select! {
                _ = shutdown.wait() => {
                    client.close(CloseRequest {
                        encrypted_share_code: encryptor.encrypt_share_code_bytes(),
                    })
                    .await?;
                    return Ok(());
                }
                // Send periodic pings to the relay.
                _ = ping_interval.tick() => {
                    Self::send_msg_to_relay(&tx, RelayMessage::Ping(get_time_ms())).await?;
                    continue;
                }
                item = messages.next() => {
                    item.context("server closed connection")??
                        .relay_message
                        .context("server message is missing")?
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
                RelayMessage::Joined(_) => (),
                RelayMessage::Ready(_) => {
                    Self::send_msg_to_relay(
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
                                            // send files
                                            let encryptor = encryptor.clone();
                                            let file_collector = file_collector.clone();
                                            let tx = tx.clone();
                                            let sender_stream_tx = sender_stream_tx.clone();
                                            let notify_rx = confirm_rx.clone();
                                            tokio::spawn(async move {
                                                if let Err(err) = Self::send_files(
                                                    encryptor,
                                                    tx,
                                                    file_collector,
                                                    notify_rx,
                                                    &sender_stream_tx,
                                                )
                                                .await
                                                {
                                                    let _ = Self::send_msg_to_stream(
                                                        &sender_stream_tx,
                                                        SenderInteractionMessage::Error(format!(
                                                            "send files error {}",
                                                            err.to_string()
                                                        )),
                                                    )
                                                    .await;
                                                }
                                            });
                                        }
                                        Confirm::Reject => {
                                            Self::send_msg_to_relay(
                                                &tx,
                                                RelayMessage::Done(Done {}),
                                            )
                                            .await?;
                                            Self::send_msg_to_stream(
                                                sender_stream_tx,
                                                SenderInteractionMessage::ReceiverReject,
                                            )
                                            .await?;
                                        }
                                    }
                                } else {
                                    Self::send_msg_to_stream(
                                        sender_stream_tx,
                                        SenderInteractionMessage::Error(
                                            "try_from confirm failed".to_string(),
                                        ),
                                    )
                                    .await?;
                                }
                            }
                            ReceiverMessage::NewFileConfirm(new_file_confirm) => {
                                confirm_tx.send(new_file_confirm).await?;
                            }
                        }
                    }
                }
                RelayMessage::Done(_) => {
                    Self::send_msg_to_stream(sender_stream_tx, SenderInteractionMessage::Completed)
                        .await?;
                }
                RelayMessage::Error(e) => {
                    Self::send_msg_to_stream(
                        sender_stream_tx,
                        SenderInteractionMessage::Error(format!("relay error {}", e.to_string())),
                    )
                    .await?;
                }
                RelayMessage::Terminated(_) => {
                    Self::send_msg_to_stream(
                        sender_stream_tx,
                        SenderInteractionMessage::OtherClose,
                    )
                    .await?;
                }
                RelayMessage::Ping(_) => (),
                RelayMessage::Pong(_) => (),
            }
        }
    }

    async fn send_files(
        encryptor: Arc<Encryptor>,
        tx: mpsc::Sender<RelayUpdate>,
        file_collector: Arc<FileCollector>,
        notify: async_channel::Receiver<FileConfirm>,
        sender_stream_tx: &mpsc::Sender<SenderInteractionMessage>,
    ) -> Result<()> {
        for send_file in file_collector.files.iter() {
            Self::send_msg_to_relay(
                &tx,
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

            let file_confirm = notify.recv().await?;

            if file_confirm.confirm == Confirm::Reject.into() {
                Self::send_msg_to_stream(
                    sender_stream_tx,
                    SenderInteractionMessage::ContinueFile(file_confirm.file_id),
                )
                .await?;
                continue;
            }

            if file_confirm.file_id != send_file.file_id {
                Self::send_msg_to_stream(
                    sender_stream_tx,
                    SenderInteractionMessage::Error("File order is wrong".to_string()),
                )
                .await?;
                continue;
            }

            if send_file.empty_dir {
                continue;
            }
            let mut read_file = File::open(send_file.access_path.as_str()).await?;
            let mut position = 0;
            loop {
                let mut buffer = BytesMut::with_capacity(10240); // 10Kib
                let read_length = read_file.read_buf(&mut buffer).await?;
                if read_length == 0 {
                    Self::send_msg_to_relay(
                        &tx,
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
                    break;
                }
                Self::send_msg_to_relay(
                    &tx,
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
        Self::send_msg_to_relay(&tx, RelayMessage::Done(Done {})).await?;
        Self::send_msg_to_stream(sender_stream_tx, SenderInteractionMessage::SendDone).await?;
        Ok(())
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
                let file_name = format!(
                    "{}.zip",
                    Path::new(p)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                );
                let path = p.to_owned();
                let file_name_for_task = file_name.clone();
                let shutdown = shutdown.clone();
                async_task.push(tokio::spawn(async move {
                    zip_folder(file_name_for_task, path, shutdown)
                }));
                zip_files.push(file_name.clone());
                files[i] = file_name;
            }
        }
        for task in async_task {
            task.await??;
        }
        Ok((files, zip_files))
    }

    async fn send_msg_to_relay(tx: &mpsc::Sender<RelayUpdate>, msg: RelayMessage) -> Result<()> {
        let relay_update = RelayUpdate {
            relay_message: Some(msg),
        };
        tx.send(relay_update).await?;
        Ok(())
    }

    async fn send_msg_to_stream(
        tx: &mpsc::Sender<SenderInteractionMessage>,
        msg: SenderInteractionMessage,
    ) -> Result<()> {
        tx.send(msg).await?;
        Ok(())
    }
}
