use std::time::Instant;

use anyhow::Result;
use bytes::Bytes;
use parking_lot::Mutex;
use tokio::sync::Mutex as AsyncMutex;

use flash_cat_common::{
    Shutdown,
    consts::RELAY_CHANNEL_CAPACITY,
    proto::{Character, RelayInfo, relay_update::RelayMessage},
};

#[derive(Debug)]
struct ConnectionSlot {
    generation: u64,
    cancel: Shutdown,
    done: Shutdown,
    active: bool,
}

impl ConnectionSlot {
    fn new() -> Self {
        Self {
            generation: 0,
            cancel: Shutdown::new(),
            done: Shutdown::new(),
            active: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionLease {
    generation: u64,
    cancel: Shutdown,
    done: Shutdown,
}

impl ConnectionLease {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub async fn superseded(&self) {
        self.cancel.wait().await;
    }

    pub fn finish(&self) {
        self.done.shutdown();
    }
}

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
        let (sharer_update_tx, sharer_update_rx) = async_channel::bounded(RELAY_CHANNEL_CAPACITY);
        let (recipient_update_tx, recipient_update_rx) = async_channel::bounded(RELAY_CHANNEL_CAPACITY);
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
    /// Session id.
    id: String,
    /// Static metadata for this session.
    metadata: Metadata,
    /// User pair for this session.
    user_pair: SessionUserPair,
    /// Timestamp of the last backend client message from an active connection.
    last_accessed: Mutex<Instant>,
    sharer_connection: AsyncMutex<ConnectionSlot>,
    recipient_connection: AsyncMutex<ConnectionSlot>,
    /// Set when this session has been closed and removed.
    ///
    /// This is used to ensure that we don't send any more messages to the
    /// session after it has been closed.
    shutdown: Shutdown,
}

impl Session {
    pub fn new(metadata: Metadata) -> Self {
        let id = nanoid::nanoid!(10);
        Session {
            id,
            metadata,
            last_accessed: Mutex::new(Instant::now()),
            sharer_connection: AsyncMutex::new(ConnectionSlot::new()),
            recipient_connection: AsyncMutex::new(ConnectionSlot::new()),
            user_pair: SessionUserPair::new(),
            shutdown: Shutdown::new(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
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

    pub async fn register_connection(
        &self,
        character: Character,
    ) -> ConnectionLease {
        let slot = match character {
            Character::Sender => &self.sharer_connection,
            Character::Receiver => &self.recipient_connection,
        };
        let mut slot = slot.lock().await;
        if slot.active {
            slot.cancel.shutdown();
            slot.done.wait().await;
        }

        slot.generation = slot.generation.wrapping_add(1);
        slot.cancel = Shutdown::new();
        slot.done = Shutdown::new();
        slot.active = true;
        ConnectionLease {
            generation: slot.generation,
            cancel: slot.cancel.clone(),
            done: slot.done.clone(),
        }
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
