use std::time::Duration;

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
pub struct RecvNewFile {
    pub file_id: u64,
    pub filename: String,
    pub path: String,
    pub size: u64,
}
