use anyhow::{Context, Result};
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

use tokio::{fs, io::AsyncWriteExt, sync::mpsc, time::MissedTickBehavior};
use tokio_stream::{wrappers::ReceiverStream as TokioReceiverStream, Stream, StreamExt};
use tonic::transport::Endpoint;

use flash_cat_common::{
    consts::PUBLIC_RELAY,
    crypt::encryptor::Encryptor,
    proto::{
        receiver_update::ReceiverMessage, relay_service_client::RelayServiceClient,
        relay_update::RelayMessage, sender_update::SenderMessage, Character, CloseRequest, Confirm,
        Done, FileConfirm, Join, ReceiverUpdate, RelayUpdate,
    },
    utils::{get_time_ms, net::net_scout::NetScout},
    Shutdown,
};

use crate::{
    FileDuplication, Progress, ReceiverConfirm, ReceiverInteractionMessage, RecvNewFile,
    SendFilesRequest, PING_INTERVAL,
};

static OUT_DIR: LazyLock<RwLock<String>> = LazyLock::new(|| RwLock::new("".to_string()));

/// Receiver stream
pub type ReceiverStream = Pin<Box<dyn Stream<Item = ReceiverInteractionMessage> + Send>>;

#[derive(Clone)]
pub struct FlashCatReceiver {
    encryptor: Arc<Encryptor>,
    specify_relay: Option<String>,
    confirm_tx: async_channel::Sender<ReceiverConfirm>,
    confirm_rx: async_channel::Receiver<ReceiverConfirm>,
    shutdown: Shutdown,
}

impl FlashCatReceiver {
    pub fn new(
        share_code: String,
        specify_relay: Option<String>,
        output: Option<String>,
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
            let endpoint = Endpoint::from_shared(specify_relay_addr)?;
            self.connect_relay(endpoint, receiver_stream_tx.clone(), self.shutdown.clone())
                .await;
        } else {
            // discovery relay addr
            let relay_addr = self.discovery_relay_addr().await;
            if relay_addr.is_some() {
                let relay_addr = relay_addr.unwrap();
                let endpoint = Endpoint::from_shared(format!("http://{relay_addr}"))?;
                self.connect_relay(endpoint, receiver_stream_tx.clone(), self.shutdown.clone())
                    .await;
            } else {
                // public relay
                let endpoint = Endpoint::from_shared(format!("https://{PUBLIC_RELAY}"))?;
                self.connect_relay(endpoint, receiver_stream_tx.clone(), self.shutdown.clone())
                    .await;
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

    pub async fn send_confirm(&self, confirm: ReceiverConfirm) -> Result<()> {
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
        endpoint: Endpoint,
        receiver_stream_tx: mpsc::Sender<ReceiverInteractionMessage>,
        shutdown: Shutdown,
    ) {
        let encryptor = self.encryptor.clone();
        let confirm_rx = self.confirm_rx.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::real_connect_relay(
                encryptor,
                endpoint,
                &receiver_stream_tx,
                confirm_rx,
                shutdown,
            )
            .await
            {
                let _ = &receiver_stream_tx
                    .send(ReceiverInteractionMessage::Error(e.to_string()))
                    .await;
            }
        });
    }

    async fn real_connect_relay(
        encryptor: Arc<Encryptor>,
        endpoint: Endpoint,
        receiver_stream_tx: &mpsc::Sender<ReceiverInteractionMessage>,
        confirm_rx: async_channel::Receiver<ReceiverConfirm>,
        shutdown: Shutdown,
    ) -> Result<()> {
        let mut client = RelayServiceClient::connect(endpoint).await?;

        let (tx, rx) = mpsc::channel(1024);

        let join = RelayMessage::Join(Join {
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

        let mut ping_interval = tokio::time::interval(PING_INTERVAL);
        ping_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            let message = tokio::select! {
                _ = shutdown.wait() => {
                    client.close(CloseRequest {
                        encrypted_share_code: encryptor.encrypt_share_code_bytes(),
                    })
                    .await?;
                    continue;
                }
                // Send periodic pings to the relay.
                _ = ping_interval.tick() => {
                    Self::send_msg_to_relay(&tx, RelayMessage::Ping(get_time_ms())).await?;
                    continue;
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
                                    receiver_message: Some(ReceiverMessage::NewFileConfirm(
                                        FileConfirm {
                                            file_id: file_id,
                                            confirm: Confirm::Accept.into(),
                                        },
                                    )),
                                })
                            } else {
                                RelayMessage::Receiver(ReceiverUpdate {
                                    receiver_message: Some(ReceiverMessage::NewFileConfirm(
                                        FileConfirm {
                                            file_id: file_id,
                                            confirm: Confirm::Reject.into(),
                                        },
                                    )),
                                })
                            };
                            Self::send_msg_to_relay(&tx, file_confirm).await?;
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
                RelayMessage::Join(_) => {
                    receiver_stream_tx
                        .send(ReceiverInteractionMessage::Message(
                            "Invalid join message".to_string(),
                        ))
                        .await?
                }
                RelayMessage::Joined(_) => (),
                RelayMessage::Ready(_) => (),
                RelayMessage::Sender(sender) => {
                    if let Some(sender_message) = sender.sender_message {
                        match sender_message {
                            SenderMessage::SendRequest(send_req) => {
                                Self::send_msg_to_stream(
                                    receiver_stream_tx,
                                    ReceiverInteractionMessage::SendFilesRequest(
                                        SendFilesRequest {
                                            total_size: send_req.total_size,
                                            num_files: send_req.num_files,
                                            num_folders: send_req.num_folders,
                                            max_file_name_length: send_req.max_file_name_length,
                                        },
                                    ),
                                )
                                .await?;
                            }
                            SenderMessage::NewFileRequest(new_file_req) => {
                                let accept_msg = RelayMessage::Receiver(ReceiverUpdate {
                                    receiver_message: Some(ReceiverMessage::NewFileConfirm(
                                        FileConfirm {
                                            file_id: new_file_req.file_id,
                                            confirm: Confirm::Accept.into(),
                                        },
                                    )),
                                });
                                let absolute_path = Path::new(OUT_DIR.read().unwrap().as_str())
                                    .join(new_file_req.relative_path.as_str());
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
                                    let recv_file = {
                                        #[cfg(unix)]
                                        {
                                            RecvFile::new({
                                                let file = fs::File::options()
                                                    .write(true)
                                                    .read(true)
                                                    .open(&absolute_path)
                                                    .await?;
                                                file.set_permissions(
                                                    std::fs::Permissions::from_mode(
                                                        new_file_req.file_mode,
                                                    ),
                                                )
                                                .await?;
                                                file
                                            })
                                        }
                                        #[cfg(windows)]
                                        {
                                            RecvFile::new(
                                                fs::File::options()
                                                    .write(true)
                                                    .read(true)
                                                    .open(&absolute_path)
                                                    .await?,
                                            )
                                        }
                                    };
                                    recv_files.insert(new_file_req.file_id, recv_file);

                                    Self::send_msg_to_stream(
                                        receiver_stream_tx,
                                        ReceiverInteractionMessage::FileDuplication(
                                            FileDuplication {
                                                file_id: new_file_req.file_id,
                                                filename: new_file_req.filename.clone(),
                                                path: absolute_path.to_string_lossy().to_string(),
                                            },
                                        ),
                                    )
                                    .await?;
                                    continue;
                                } else {
                                    let parent = absolute_path.parent().unwrap_or(Path::new(""));
                                    if !parent.exists() && !parent.to_string_lossy().is_empty() {
                                        fs::create_dir_all(parent).await?;
                                    }
                                    let recv_file = {
                                        #[cfg(unix)]
                                        {
                                            RecvFile::new({
                                                let file = fs::File::create(&absolute_path).await?;
                                                file.set_permissions(
                                                    std::fs::Permissions::from_mode(
                                                        new_file_req.file_mode,
                                                    ),
                                                )
                                                .await?;
                                                file
                                            })
                                        }
                                        #[cfg(windows)]
                                        {
                                            RecvFile::new(fs::File::create(&absolute_path).await?)
                                        }
                                    };
                                    recv_files.insert(new_file_req.file_id, recv_file);
                                }

                                Self::send_msg_to_relay(&tx, accept_msg).await?;
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
                                        return Err(anyhow::Error::msg(format!(
                                            "decrypt failed: {e}"
                                        )));
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
                                Self::send_msg_to_stream(
                                    receiver_stream_tx,
                                    ReceiverInteractionMessage::FileProgressFinish(
                                        file_done.file_id,
                                    ),
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
                    Self::send_msg_to_stream(
                        receiver_stream_tx,
                        ReceiverInteractionMessage::ReceiveDone,
                    )
                    .await?;
                }
                RelayMessage::Error(e) => {
                    receiver_stream_tx
                        .send(ReceiverInteractionMessage::Error(e.to_string()))
                        .await?;
                }
                RelayMessage::Terminated(_) => {
                    Self::send_msg_to_stream(
                        receiver_stream_tx,
                        ReceiverInteractionMessage::OtherClose,
                    )
                    .await?;
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

    async fn send_msg_to_relay(tx: &mpsc::Sender<RelayUpdate>, msg: RelayMessage) -> Result<()> {
        let relay_update = RelayUpdate {
            relay_message: Some(msg),
        };
        tx.send(relay_update).await?;
        Ok(())
    }

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
        Self { file, progress: 0 }
    }

    async fn write(&mut self, data: &[u8]) -> Result<()> {
        self.file.write(data).await?;
        let data_len = data.len() as u64;
        self.progress += data_len;
        Ok(())
    }

    fn get_progress(&self) -> u64 {
        self.progress
    }
}
