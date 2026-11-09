//! Shared P2P sync core for My Share.
//!
//! Provides libp2p swarm setup, custom file/sync/clipboard protocols,
//! local SQLite indexing, and folder sync with last-write-wins semantics.

pub mod clipboard;
pub mod identity;
pub mod protocols;
pub mod store;
pub mod swarm;
pub mod sync_engine;

pub use clipboard::ClipboardPayload;
pub use identity::{load_or_create_identity, Identity};
pub use protocols::{
    FileChunkRequest, FileChunkResponse, FileOffer, SyncManifest, SyncManifestEntry,
};
pub use store::{AuthSession, LocalStore};
pub use swarm::{NodeEvent, SyncNode, SyncNodeConfig};
pub use sync_engine::{ConflictPolicy, SyncEngine, SyncPath};

use serde::{Deserialize, Serialize};

/// Snapshot of transfer progress for the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferStatus {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub peer_id: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub state: String,
}
