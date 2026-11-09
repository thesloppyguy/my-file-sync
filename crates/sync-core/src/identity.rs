use anyhow::{Context, Result};
use base64::Engine;
use libp2p::identity::{Keypair, PeerId};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Identity {
    pub keypair: Keypair,
    pub peer_id: PeerId,
}

#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    /// Base64-encoded protobuf-encoded keypair.
    keypair_b64: String,
}

pub fn default_identity_path(app_dir: &Path) -> PathBuf {
    app_dir.join("identity.json")
}

pub fn load_or_create_identity(app_dir: &Path) -> Result<Identity> {
    fs::create_dir_all(app_dir).context("create app data dir")?;
    let path = default_identity_path(app_dir);
    if path.exists() {
        let raw = fs::read_to_string(&path).context("read identity")?;
        let stored: StoredIdentity = serde_json::from_str(&raw).context("parse identity")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&stored.keypair_b64)
            .context("decode identity")?;
        let keypair = Keypair::from_protobuf_encoding(&bytes).context("decode keypair")?;
        let peer_id = keypair.public().to_peer_id();
        return Ok(Identity { keypair, peer_id });
    }

    let keypair = Keypair::generate_ed25519();
    let peer_id = keypair.public().to_peer_id();
    let encoded = keypair
        .to_protobuf_encoding()
        .context("encode keypair")?;
    let stored = StoredIdentity {
        keypair_b64: base64::engine::general_purpose::STANDARD.encode(encoded),
    };
    fs::write(&path, serde_json::to_string_pretty(&stored)?)
        .context("write identity")?;
    Ok(Identity { keypair, peer_id })
}
