use crate::protocols::{SyncManifest, SyncManifestEntry};
use crate::store::{IndexedFile, LocalStore};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use uuid::Uuid;

const TOMBSTONE_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPath {
    pub root_id: String,
    pub local_path: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ConflictPolicy {
    LastWriteWins,
}

pub struct SyncEngine {
    store: std::sync::Arc<LocalStore>,
}

impl SyncEngine {
    pub fn new(store: std::sync::Arc<LocalStore>) -> Self {
        Self { store }
    }

    pub fn add_path(&self, local_path: &Path) -> Result<SyncPath> {
        let root_id = Uuid::new_v4().to_string();
        self.store.upsert_sync_root(&root_id, local_path)?;
        self.scan_root(&root_id, local_path)?;
        Ok(SyncPath {
            root_id,
            local_path: local_path.to_string_lossy().to_string(),
            enabled: true,
        })
    }

    pub fn list_paths(&self) -> Result<Vec<SyncPath>> {
        Ok(self
            .store
            .list_sync_roots()?
            .into_iter()
            .map(|(root_id, path, enabled)| SyncPath {
                root_id,
                local_path: path.to_string_lossy().to_string(),
                enabled,
            })
            .collect())
    }

    pub fn build_manifest(&self, root_id: &str) -> Result<SyncManifest> {
        let files = self.store.list_files(root_id)?;
        Ok(SyncManifest {
            root_id: root_id.to_string(),
            entries: files
                .into_iter()
                .map(|f| SyncManifestEntry {
                    relative_path: f.relative_path,
                    size: f.size,
                    mtime_ms: f.mtime_ms,
                    content_hash: f.content_hash,
                    tombstone: f.tombstone,
                })
                .collect(),
        })
    }

    pub fn scan_root(&self, root_id: &str, root: &Path) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        if root.is_dir() {
            visit_dir(root, root, &mut |rel, path, meta| {
                seen.insert(rel.clone());
                let hash = hash_file(path)?;
                let mtime_ms = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                self.store.upsert_file(&IndexedFile {
                    root_id: root_id.to_string(),
                    relative_path: rel,
                    size: meta.len(),
                    mtime_ms,
                    content_hash: hash,
                    tombstone: false,
                    tombstone_at_ms: None,
                })?;
                Ok(())
            })?;
        }

        let now = chrono::Utc::now().timestamp_millis();
        for existing in self.store.list_files(root_id)? {
            if !seen.contains(&existing.relative_path) && !existing.tombstone {
                self.store.upsert_file(&IndexedFile {
                    tombstone: true,
                    tombstone_at_ms: Some(now),
                    ..existing
                })?;
            } else if existing.tombstone {
                if let Some(at) = existing.tombstone_at_ms {
                    if now - at > TOMBSTONE_TTL_MS {
                        // Leave row; purge can be a later pass. Mark as expired via hash.
                        let mut expired = existing;
                        expired.content_hash = "expired".into();
                        self.store.upsert_file(&expired)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Decide which remote entries should be pulled locally (LWW).
    pub fn entries_to_pull(
        &self,
        local: &SyncManifest,
        remote: &SyncManifest,
    ) -> Vec<SyncManifestEntry> {
        let local_map: std::collections::HashMap<_, _> = local
            .entries
            .iter()
            .map(|e| (e.relative_path.as_str(), e))
            .collect();

        let mut pull = Vec::new();
        for remote_entry in &remote.entries {
            match local_map.get(remote_entry.relative_path.as_str()) {
                None => pull.push(remote_entry.clone()),
                Some(local_entry) => {
                    if remote_entry.tombstone && !local_entry.tombstone {
                        if remote_entry.mtime_ms >= local_entry.mtime_ms {
                            pull.push(remote_entry.clone());
                        }
                    } else if !remote_entry.tombstone
                        && remote_entry.content_hash != local_entry.content_hash
                    {
                        if remote_entry.mtime_ms > local_entry.mtime_ms {
                            pull.push(remote_entry.clone());
                        } else if remote_entry.mtime_ms == local_entry.mtime_ms
                            && remote_entry.content_hash != local_entry.content_hash
                        {
                            // Conflict — caller writes conflict copy from remote.
                            let mut conflict = remote_entry.clone();
                            conflict.relative_path = conflict_path(&conflict.relative_path);
                            pull.push(conflict);
                        }
                    }
                }
            }
        }
        pull
    }

    pub fn resolve_local_path(&self, root_id: &str) -> Result<PathBuf> {
        let roots = self.store.list_sync_roots()?;
        roots
            .into_iter()
            .find(|(id, _, _)| id == root_id)
            .map(|(_, path, _)| path)
            .context("unknown sync root")
    }
}

fn conflict_path(relative: &str) -> String {
    let path = Path::new(relative);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let parent = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .filter(|p| !p.is_empty())
        .map(|p| format!("{p}/"))
        .unwrap_or_default();
    format!("{parent}{stem} (conflict remote){ext}")
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn visit_dir(
    root: &Path,
    dir: &Path,
    on_file: &mut dyn FnMut(String, &Path, fs::Metadata) -> Result<()>,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            visit_dir(root, &path, on_file)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            let meta = entry.metadata()?;
            on_file(rel, &path, meta)?;
        }
    }
    Ok(())
}

pub fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}
