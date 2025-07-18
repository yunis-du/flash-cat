#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::Path,
    pin::Pin,
    sync::{Arc, LazyLock, RwLock},
    time::Duration,
};

use anyhow::{Context, Result};
use tokio::{
    fs,
    io::{AsyncSeekExt, AsyncWriteExt, SeekFrom},
    sync::mpsc,
};
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream as TokioReceiverStream};
use tonic::transport::Endpoint;

use flash_cat_common::{
    Shutdown, compare_versions,
    consts::{PUBLIC_RELAY, SEND_BUFF_SIZE},
    crypt::encryptor::Encryptor,
    proto::{
        BreakPointConfirm, Character, ClientType, CloseRequest, Confirm, Done, FileConfirm, Id, JoinRequest, NewFileConfirm, ReceiverUpdate, RelayUpdate,
        file_confirm::ConfirmMessage, join_response, receiver_update::ReceiverMessage, relay_service_client::RelayServiceClient, relay_update::RelayMessage,
        sender_update::SenderMessage,
    },
    utils::{
        fs::{missing_chunks, reset_path},
        net::net_scout::NetScout,
    },
};
use flash_cat_relay::built_info;

use crate::{BreakPoint, FileDuplication, Progress, ReceiverConfirm, ReceiverInteractionMessage, RecvNewFile, RelayType, SendFilesRequest, get_endpoint};

static OUT_DIR: LazyLock<RwLock<String>> = LazyLock::new(|| RwLock::new("".to_string()));

/// Receiver stream
pub type ReceiverStream = Pin<Box<dyn Stream<Item = ReceiverInteractionMessage> + Send>>;

#[derive(Clone)]
pub struct FlashCatReceiver {
    encryptor: Arc<Encryptor>,
    specify_relay: Option<String>,
    confirm_tx: async_channel::Sender<ReceiverConfirm>,
    confirm_rx: async_channel::Receiver<ReceiverConfirm>,
    client_type: ClientType,
    lan: bool,
    shutdown: Shutdown,
}

impl FlashCatReceiver {
    pub fn new(
        share_code: String,
        specify_relay: Option<String>,
        output: Option<String>,
        client_type: ClientType,
        lan: bool,
    ) -> Result<Self> {
        if output.is_some() {
            *OUT_DIR.write().unwrap() = output.unwrap().clone();
        }
        let encryptor = Arc::new(Encryptor::new(share_code)?);
        let (confirm_tx, confirm_rx) = async_channel::bounded(10);
        Ok(Self {
            encryptor,
            specify_relay,
            confirm_tx,
            confirm_rx,
            client_type,
            lan,
            shutdown: Shutdown::new(),
        })
    }

    pub async fn start(self: Arc<Self>) -> Result<ReceiverStream> {
        let (receiver_stream_tx, mut receiver_stream_rx) = mpsc::channel(1024);

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
        // resolve shutdown when receiver_stream_rx is no message will cause panic
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        Ok(Box::pin(async_stream::stream! {
            while !self.shutdown.is_terminated() {
                tokio::select! {
                    Some(sender_stream) = receiver_stream_rx.recv() => {
                        yield sender_stream;
                    }
                    _ = interval.tick() =>(),
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
        let net_scout = NetScout::new(match_content, Duration::from_secs(3), shutdown.clone());
        if let Ok(addr) = net_scout.discovery().await {
            addr
        } else {
            None
        }
    }

    async fn connect_relay(
        &self,
        relay_type: RelayType,
        endpoint: Endpoint,
        receiver_stream_tx: mpsc::Sender<ReceiverInteractionMessage>,
        shutdown: Shutdown,
    ) -> Result<()> {
        let mut client = RelayServiceClient::connect(endpoint.clone()).await?;

        let resp = match client
            .join(JoinRequest {
                id: Some(Id {
                    encrypted_share_code: self.encryptor.encrypt_share_code_bytes(),
                    character: Character::Receiver.into(),
                }),
                client_type: self.client_type.into(),
                sender_local_relay: None,
            })
            .await
        {
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
                    return Err(anyhow::Error::msg(join_failed.error_msg));
                }
            }
        } else {
            return Err(anyhow::Error::msg("can't get relay ip and port"));
        };

        match self.client_type {
            ClientType::Cli => {
                if compare_versions(client_latest_version.as_str(), built_info::PKG_VERSION) == std::cmp::Ordering::Greater {
                    let _ = receiver_stream_tx
                        .send(ReceiverInteractionMessage::Message(format!(
                            "newly cli version[{}] is available",
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
                Some(sender_local_relay_endpoint) => sender_local_relay_endpoint,
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
            endpoint
        };

        let encryptor = self.encryptor.clone();
        let confirm_rx = self.confirm_rx.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::relay_channel(encryptor, endpoint, &receiver_stream_tx, confirm_rx, shutdown).await {
                let _ = &receiver_stream_tx.send(ReceiverInteractionMessage::Error(e.to_string())).await;
            }
        });
        Ok(())
    }

    async fn relay_channel(
        encryptor: Arc<Encryptor>,
        endpoint: Endpoint,
        receiver_stream_tx: &mpsc::Sender<ReceiverInteractionMessage>,
        confirm_rx: async_channel::Receiver<ReceiverConfirm>,
        shutdown: Shutdown,
    ) -> Result<()> {
        let mut client = RelayServiceClient::connect(endpoint).await?;

        let (tx, rx) = mpsc::channel(1024);

        let join = RelayMessage::Join(Id {
            encrypted_share_code: encryptor.encrypt_share_code_bytes(),
            character: Character::Receiver.into(),
        });
        tx.send(RelayUpdate {
            relay_message: Some(join),
        })
        .await?;

        let mut recv_files = HashMap::new();

        let resp = client.channel(TokioReceiverStream::new(rx)).await?;
        let mut messages = resp.into_inner(); // A stream of relay messages.

        loop {
            let message = tokio::select! {
                _ = shutdown.wait() => {
                    client.close(CloseRequest {
                        encrypted_share_code: encryptor.encrypt_share_code_bytes(),
                    })
                    .await?;
                    return Ok(());
                }
                Ok(confirm) = confirm_rx.recv() => {
                    match confirm {
                        ReceiverConfirm::ReceiveConfirm(accept) => {
                            if accept {
                                let share_accept = RelayMessage::Receiver(ReceiverUpdate {
                                    receiver_message: Some(ReceiverMessage::ShareConfirm(
                                        Confirm::Accept.into(),
                                    )),
                                });
                                Self::send_msg_to_relay(&tx, share_accept).await?;
                            } else {
                                let share_reject = RelayMessage::Receiver(ReceiverUpdate {
                                    receiver_message: Some(ReceiverMessage::ShareConfirm(
                                        Confirm::Reject.into(),
                                    )),
                                });
                                Self::send_msg_to_relay(&tx, share_reject).await?;
                            }
                        }
                        ReceiverConfirm::FileConfirm((accept, file_id)) => {
                            let file_confirm = if accept {
                                RelayMessage::Receiver(ReceiverUpdate {
                                    receiver_message: Some(ReceiverMessage::FileConfirm(
                                        FileConfirm {
                                            confirm_message: Some(ConfirmMessage::NewFileConfirm(
                                                NewFileConfirm {
                                                    file_id: file_id,
                                                    confirm: Confirm::Accept.into(),
                                                },
                                            )),
                                        },
                                    )),
                                })
                            } else {
                                RelayMessage::Receiver(ReceiverUpdate {
                                    receiver_message: Some(ReceiverMessage::FileConfirm(
                                        FileConfirm {
                                            confirm_message: Some(ConfirmMessage::NewFileConfirm(
                                                NewFileConfirm {
                                                    file_id: file_id,
                                                    confirm: Confirm::Reject.into(),
                                                },
                                            )),
                                        },
                                    )),
                                })
                            };
                            Self::send_msg_to_relay(&tx, file_confirm).await?;
                        }
                        ReceiverConfirm::BreakPointConfirm((accept, file_id, position)) => {
                            let break_point_confirm = if accept {
                                RelayMessage::Receiver(ReceiverUpdate {
                                    receiver_message: Some(ReceiverMessage::FileConfirm(
                                        FileConfirm {
                                            confirm_message: Some(ConfirmMessage::BreakPointConfirm(
                                                BreakPointConfirm {
                                                    file_id: file_id,
                                                    confirm: Confirm::Accept.into(),
                                                    position: position,
                                                },
                                            )),
                                        },
                                    )),
                                })
                            } else {
                                RelayMessage::Receiver(ReceiverUpdate {
                                    receiver_message: Some(ReceiverMessage::FileConfirm(
                                        FileConfirm {
                                            confirm_message: Some(ConfirmMessage::BreakPointConfirm(
                                                BreakPointConfirm {
                                                    file_id: file_id,
                                                    confirm: Confirm::Reject.into(),
                                                    position: 0,
                                                },
                                            )),
                                        },
                                    )),
                                })
                            };
                            Self::send_msg_to_relay(&tx, break_point_confirm).await?;
                        }
                    }
                    continue;
                }
                item = messages.next() => {
                    item.context("server closed connection")??
                        .relay_message
                        .context("server message is missing")?
                }
            };

            match message {
                RelayMessage::Join(_) => receiver_stream_tx.send(ReceiverInteractionMessage::Message("Invalid join message".to_string())).await?,
                RelayMessage::Joined(_) => (),
                RelayMessage::Ready(_) => (),
                RelayMessage::Sender(sender) => {
                    if let Some(sender_message) = sender.sender_message {
                        match sender_message {
                            SenderMessage::SendRequest(send_req) => {
                                Self::send_msg_to_stream(
                                    receiver_stream_tx,
                                    ReceiverInteractionMessage::SendFilesRequest(SendFilesRequest {
                                        total_size: send_req.total_size,
                                        num_files: send_req.num_files,
                                        num_folders: send_req.num_folders,
                                        max_file_name_length: send_req.max_file_name_length,
                                    }),
                                )
                                .await?;
                            }
                            SenderMessage::NewFileRequest(new_file_req) => {
                                let accept_msg = RelayMessage::Receiver(ReceiverUpdate {
                                    receiver_message: Some(ReceiverMessage::FileConfirm(FileConfirm {
                                        confirm_message: Some(ConfirmMessage::NewFileConfirm(NewFileConfirm {
                                            file_id: new_file_req.file_id,
                                            confirm: Confirm::Accept.into(),
                                        })),
                                    })),
                                });

                                let relative_path = reset_path(new_file_req.relative_path.as_str());

                                let absolute_path = Path::new(OUT_DIR.read().unwrap().as_str()).join(relative_path.as_str());
                                if new_file_req.is_empty_dir {
                                    tokio::fs::create_dir_all(&absolute_path).await?;
                                    Self::send_msg_to_relay(&tx, accept_msg).await?;
                                    continue;
                                }

                                Self::send_msg_to_stream(
                                    receiver_stream_tx,
                                    ReceiverInteractionMessage::RecvNewFile(RecvNewFile {
                                        file_id: new_file_req.file_id,
                                        filename: new_file_req.filename.clone(),
                                        path: absolute_path.to_string_lossy().to_string(),
                                        size: new_file_req.total_size,
                                    }),
                                )
                                .await?;

                                if absolute_path.exists() {
                                    let recv_file = RecvFile::new(fs::File::options().write(true).read(true).open(&absolute_path).await?);

                                    let recv_file_len = recv_file.file.metadata().await?.len();
                                    recv_files.insert(new_file_req.file_id, recv_file);

                                    if recv_file_len == new_file_req.total_size {
                                        // Breakpoint exists, continue receiving
                                        if let Ok((saved_chunks, missing_chunks, percent)) = missing_chunks(&absolute_path, SEND_BUFF_SIZE) {
                                            if missing_chunks > 0 && saved_chunks > 0 && percent > 0.0 {
                                                Self::send_msg_to_stream(
                                                    receiver_stream_tx,
                                                    ReceiverInteractionMessage::BreakPoint(BreakPoint {
                                                        file_id: new_file_req.file_id,
                                                        filename: new_file_req.filename.clone(),
                                                        position: (saved_chunks * SEND_BUFF_SIZE) as u64,
                                                        percent,
                                                    }),
                                                )
                                                .await?;
                                                continue;
                                            }
                                        }
                                    }

                                    Self::send_msg_to_stream(
                                        receiver_stream_tx,
                                        ReceiverInteractionMessage::FileDuplication(FileDuplication {
                                            file_id: new_file_req.file_id,
                                            filename: new_file_req.filename.clone(),
                                            path: absolute_path.to_string_lossy().to_string(),
                                        }),
                                    )
                                    .await?;
                                } else {
                                    let parent = absolute_path.parent().unwrap_or(Path::new(""));
                                    if !parent.exists() && !parent.to_string_lossy().is_empty() {
                                        fs::create_dir_all(parent).await?;
                                    }
                                    let recv_file = RecvFile::new(fs::File::create(&absolute_path).await?);
                                    #[cfg(unix)]
                                    {
                                        recv_file
                                            .file
                                            .set_permissions(if new_file_req.file_mode > 0 {
                                                std::fs::Permissions::from_mode(new_file_req.file_mode)
                                            } else {
                                                // Set as the default permissions of the file
                                                std::fs::Permissions::from_mode(0o644)
                                            })
                                            .await?;
                                    }
                                    recv_file.file.set_len(new_file_req.total_size).await?;
                                    recv_files.insert(new_file_req.file_id, recv_file);

                                    Self::send_msg_to_relay(&tx, accept_msg).await?;
                                }
                            }
                            SenderMessage::BreakPoint(break_point) => {
                                if !recv_files.contains_key(&break_point.file_id) {
                                    return Err(anyhow::Error::msg("receive file failed"));
                                }
                                let recv_file = recv_files.get_mut(&break_point.file_id).unwrap();
                                recv_file.progress = break_point.position;
                                recv_file.file.seek(SeekFrom::Start(break_point.position)).await?;
                            }
                            SenderMessage::FileData(file_data) => {
                                if !recv_files.contains_key(&file_data.file_id) {
                                    return Err(anyhow::Error::msg("receive file failed"));
                                }
                                let recv_file = recv_files.get_mut(&file_data.file_id).unwrap();
                                let encryptor = encryptor.clone();
                                let data = match encryptor.decrypt(file_data.data.as_ref()) {
                                    Ok(data) => data,
                                    Err(e) => {
                                        return Err(anyhow::Error::msg(format!("decrypt failed: {e}")));
                                    }
                                };
                                recv_file.write(&data).await?;
                                Self::send_msg_to_stream(
                                    receiver_stream_tx,
                                    ReceiverInteractionMessage::FileProgress(Progress {
                                        file_id: file_data.file_id,
                                        position: recv_file.get_progress(),
                                    }),
                                )
                                .await?;
                            }
                            SenderMessage::FileDone(file_done) => {
                                recv_files.remove(&file_done.file_id); // drop file recycle file descriptors
                                Self::send_msg_to_stream(
                                    receiver_stream_tx,
                                    ReceiverInteractionMessage::FileProgressFinish(file_done.file_id),
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
                    Self::send_msg_to_relay(&tx, RelayMessage::Done(Done {})).await?;
                    Self::send_msg_to_stream(receiver_stream_tx, ReceiverInteractionMessage::ReceiveDone).await?;
                }
                RelayMessage::Error(e) => {
                    receiver_stream_tx.send(ReceiverInteractionMessage::Error(e.to_string())).await?;
                }
                RelayMessage::Terminated(_) => {
                    Self::send_msg_to_stream(receiver_stream_tx, ReceiverInteractionMessage::OtherClose).await?;
                }
                RelayMessage::Ping(_) => (),
                RelayMessage::Pong(_) => (),
            }
        }
    }

    pub fn shutdown(&self) {
        self.shutdown.shutdown();
    }

    pub async fn terminated(&self) {
        self.shutdown.wait().await
    }

    /// Send message to relay.
    async fn send_msg_to_relay(
        tx: &mpsc::Sender<RelayUpdate>,
        msg: RelayMessage,
    ) -> Result<()> {
        let relay_update = RelayUpdate {
            relay_message: Some(msg),
        };
        tx.send(relay_update).await?;
        Ok(())
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

struct RecvFile {
    file: fs::File,
    progress: u64,
}

impl RecvFile {
    fn new(file: fs::File) -> Self {
        Self {
            file,
            progress: 0,
        }
    }

    async fn write(
        &mut self,
        data: &[u8],
    ) -> Result<()> {
        self.file.write(data).await?;
        let data_len = data.len() as u64;
        self.progress += data_len;
        Ok(())
    }

    fn get_progress(&self) -> u64 {
        self.progress
    }
}
