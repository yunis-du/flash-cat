use std::time::Instant;

use anyhow::Result;
use bytes::Bytes;
use parking_lot::Mutex;

use flash_cat_common::{
    Shutdown,
    proto::{RelayInfo, relay_update::RelayMessage},
};

#[derive(Debug, Clone)]
pub struct Metadata {
    /// Used to validate that clients have the correct encryption share code.
    pub encrypted_share_code: Bytes,
    /// Local relay info for sender.
    pub sender_local_relay: Option<RelayInfo>,
}

#[derive(Debug, Clone)]
pub struct SessionUserPair {
    sharer_update_tx: async_channel::Sender<RelayMessage>,
    sharer_update_rx: async_channel::Receiver<RelayMessage>,
    recipient_update_tx: async_channel::Sender<RelayMessage>,
    recipient_update_rx: async_channel::Receiver<RelayMessage>,
}

impl SessionUserPair {
    pub fn new() -> Self {
        let (sharer_update_tx, sharer_update_rx) = async_channel::bounded(256);
        let (recipient_update_tx, recipient_update_rx) = async_channel::bounded(256);
        Self {
            sharer_update_tx,
            sharer_update_rx,
            recipient_update_tx,
            recipient_update_rx,
        }
    }
}

#[derive(Debug)]
pub struct Session {
    /// Static metadata for this session.
    metadata: Metadata,

    user_pair: SessionUserPair,

    /// Timestamp of the last backend client message from an active connection.
    last_accessed: Mutex<Instant>,

    /// Set when this session has been closed and removed.
    shutdown: Shutdown,
}

impl Session {
    pub fn new(metadata: Metadata) -> Self {
        Session {
            metadata,
            last_accessed: Mutex::new(Instant::now()),
            user_pair: SessionUserPair::new(),
            shutdown: Shutdown::new(),
        }
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn access(&self) {
        *self.last_accessed.lock() = Instant::now();
    }

    pub fn last_accessed(&self) -> Instant {
        *self.last_accessed.lock()
    }

    pub async fn send_to_share(
        &self,
        msg: RelayMessage,
    ) -> Result<()> {
        self.user_pair.sharer_update_tx.send(msg).await?;
        Ok(())
    }

    pub async fn recv_from_share(&self) -> Result<RelayMessage> {
        Ok(self.user_pair.sharer_update_rx.recv().await?)
    }

    pub async fn send_to_recipient(
        &self,
        msg: RelayMessage,
    ) -> Result<()> {
        self.user_pair.recipient_update_tx.send(msg).await?;
        Ok(())
    }

    pub async fn recv_from_recipient(&self) -> Result<RelayMessage> {
        Ok(self.user_pair.recipient_update_rx.recv().await?)
    }

    pub async fn broadcast(
        &self,
        msg: RelayMessage,
    ) -> Result<()> {
        self.user_pair.sharer_update_tx.send(msg.clone()).await?;
        self.user_pair.recipient_update_tx.send(msg).await?;
        Ok(())
    }

    pub fn sharer_update_tx(&self) -> &async_channel::Sender<RelayMessage> {
        &self.user_pair.sharer_update_tx
    }

    pub fn sharer_update_rx(&self) -> &async_channel::Receiver<RelayMessage> {
        &self.user_pair.sharer_update_rx
    }

    pub fn recipient_update_tx(&self) -> &async_channel::Sender<RelayMessage> {
        &self.user_pair.recipient_update_tx
    }

    pub fn recipient_update_rx(&self) -> &async_channel::Receiver<RelayMessage> {
        &self.user_pair.recipient_update_rx
    }

    pub fn shutdown(&self) {
        self.shutdown.shutdown()
    }

    pub async fn terminated(&self) {
        self.shutdown.wait().await
    }
}
