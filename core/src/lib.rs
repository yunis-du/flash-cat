use std::time::Duration;

use anyhow::Result;
use bytes::Bytes;
use tokio::sync::mpsc;
use tonic::transport::Endpoint;

use flash_cat_common::{
    consts::{DEFAULT_HTTP2_KEEPALIVE_INTERVAL, DEFAULT_HTTP2_KEEPALIVE_TIMEOUT, DEFAULT_TCP_KEEPALIVE},
    proto::{RelayUpdate, relay_update::RelayMessage},
};

pub mod receiver;
pub mod sender;

/// Interval for ping.
pub const PING_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub enum SenderInteractionMessage {
    Message(String),
    Error(String),
    ReceiverReject,
    RelayFailed((RelayType, String)),
    ContinueFile(u64),
    FileProgress(Progress),
    FileProgressFinish(u64),
    OtherClose,
    SendDone,
    Completed,
}

#[derive(Debug, Clone)]
pub enum ReceiverInteractionMessage {
    Message(String),
    Error(String),
    SendFilesRequest(SendFilesRequest),
    FileDuplication(FileDuplication),
    RecvNewFile(RecvNewFile),
    BreakPoint(BreakPoint),
    FileProgress(Progress),
    FileProgressFinish(u64),
    OtherClose,
    ReceiveDone,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RelayType {
    Local,
    Public,
    Specify,
}

impl RelayType {
    pub fn to_string(&self) -> String {
        match self {
            RelayType::Local => "Local".to_string(),
            RelayType::Public => "Public".to_string(),
            RelayType::Specify => "Specify".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Progress {
    pub file_id: u64,
    pub position: u64,
}

#[derive(Debug, Clone)]
pub enum ReceiverConfirm {
    ReceiveConfirm(bool),
    FileConfirm((bool, u64)),
    BreakPointConfirm((bool, u64, u64)), // (accept, file_id, start_position)
}

#[derive(Debug, Clone)]
pub struct SendFilesRequest {
    pub total_size: u64,
    pub num_files: u64,
    pub num_folders: u64,
    pub max_file_name_length: u64,
}

#[derive(Debug, Clone)]
pub struct FileDuplication {
    pub file_id: u64,
    pub filename: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct BreakPoint {
    pub file_id: u64,
    pub filename: String,
    pub position: u64,
    pub percent: f64,
}

#[derive(Debug, Clone)]
pub struct RecvNewFile {
    pub file_id: u64,
    pub filename: String,
    pub path: String,
    pub size: u64,
}

fn get_endpoint(s: impl Into<Bytes>) -> Result<Endpoint> {
    let endpoint = Endpoint::from_shared(s.into())?
        .http2_keep_alive_interval(DEFAULT_HTTP2_KEEPALIVE_INTERVAL)
        .keep_alive_timeout(DEFAULT_HTTP2_KEEPALIVE_TIMEOUT)
        .http2_adaptive_window(true) // enable adaptive window size
        .tcp_keepalive(Some(DEFAULT_TCP_KEEPALIVE)); // set TCP keepalive
    Ok(endpoint)
}

/// Send message to relay
pub async fn send_msg_to_relay(
    tx: &mpsc::Sender<RelayUpdate>,
    msg: RelayMessage,
) -> Result<()> {
    let relay_update = RelayUpdate {
        relay_message: Some(msg),
    };
    tx.send(relay_update).await?;
    Ok(())
}
