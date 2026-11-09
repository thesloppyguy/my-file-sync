use crate::protocols::ClipboardMessage;
use crate::sync_engine::hash_bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardPayload {
    pub text: String,
    pub content_hash: String,
    pub origin_peer: String,
    pub sent_at_ms: i64,
}

impl ClipboardPayload {
    pub fn new(text: impl Into<String>, origin_peer: impl Into<String>) -> Self {
        let text = text.into();
        let content_hash = hash_bytes(text.as_bytes());
        Self {
            text,
            content_hash,
            origin_peer: origin_peer.into(),
            sent_at_ms: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn to_message(&self) -> ClipboardMessage {
        ClipboardMessage {
            origin_peer: self.origin_peer.clone(),
            content_hash: self.content_hash.clone(),
            text: self.text.clone(),
            sent_at_ms: self.sent_at_ms,
        }
    }

    pub fn from_message(msg: ClipboardMessage) -> Self {
        Self {
            text: msg.text,
            content_hash: msg.content_hash,
            origin_peer: msg.origin_peer,
            sent_at_ms: msg.sent_at_ms,
        }
    }
}

pub fn clipboard_topic(user_id: &str) -> String {
    format!("mysync/clipboard/{user_id}")
}

pub fn presence_topic(user_id: &str) -> String {
    format!("mysync/presence/{user_id}")
}
