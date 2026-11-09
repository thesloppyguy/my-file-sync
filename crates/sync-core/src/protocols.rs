use serde::{Deserialize, Serialize};

pub const FILE_PROTOCOL: &str = "/mysync/file/1.0.0";
pub const SYNC_PROTOCOL: &str = "/mysync/sync/1.0.0";
pub const CLIPBOARD_PROTOCOL: &str = "/mysync/clipboard/1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileRequest {
    Offer(FileOffer),
    Chunk(FileChunkRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileResponse {
    Accept { transfer_id: String },
    Reject { reason: String },
    Chunk(FileChunkResponse),
    Done { transfer_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOffer {
    pub transfer_id: String,
    pub file_name: String,
    pub relative_path: Option<String>,
    pub size: u64,
    pub content_hash: String,
    pub mtime_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChunkRequest {
    pub transfer_id: String,
    pub offset: u64,
    pub length: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChunkResponse {
    pub transfer_id: String,
    pub offset: u64,
    pub data: Vec<u8>,
    pub eof: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncRequest {
    GetManifest { root_id: String },
    PullFile { root_id: String, relative_path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncResponse {
    Manifest(SyncManifest),
    FileOffer(FileOffer),
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncManifest {
    pub root_id: String,
    pub entries: Vec<SyncManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncManifestEntry {
    pub relative_path: String,
    pub size: u64,
    pub mtime_ms: i64,
    pub content_hash: String,
    pub tombstone: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardMessage {
    pub origin_peer: String,
    pub content_hash: String,
    pub text: String,
    pub sent_at_ms: i64,
}
