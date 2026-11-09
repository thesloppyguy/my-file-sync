use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedFile {
    pub root_id: String,
    pub relative_path: String,
    pub size: u64,
    pub mtime_ms: i64,
    pub content_hash: String,
    pub tombstone: bool,
    pub tombstone_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub access_token: String,
    pub user_id: String,
    pub device_id: String,
    pub email: String,
    pub device_name: String,
}

pub struct LocalStore {
    conn: Mutex<Connection>,
}

impl LocalStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).context("open sqlite")?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sync_roots (
              root_id TEXT PRIMARY KEY,
              local_path TEXT NOT NULL,
              enabled INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS files (
              root_id TEXT NOT NULL,
              relative_path TEXT NOT NULL,
              size INTEGER NOT NULL,
              mtime_ms INTEGER NOT NULL,
              content_hash TEXT NOT NULL,
              tombstone INTEGER NOT NULL DEFAULT 0,
              tombstone_at_ms INTEGER,
              PRIMARY KEY (root_id, relative_path)
            );
            CREATE TABLE IF NOT EXISTS session (
              id INTEGER PRIMARY KEY CHECK (id = 1),
              access_token TEXT NOT NULL,
              user_id TEXT NOT NULL,
              device_id TEXT NOT NULL,
              email TEXT NOT NULL,
              device_name TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS settings (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS transfers (
              id TEXT PRIMARY KEY,
              kind TEXT NOT NULL,
              path TEXT NOT NULL,
              peer_id TEXT NOT NULL,
              bytes_done INTEGER NOT NULL,
              bytes_total INTEGER NOT NULL,
              state TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn save_session(&self, session: &AuthSession) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT INTO session (id, access_token, user_id, device_id, email, device_name)
               VALUES (1, ?1, ?2, ?3, ?4, ?5)
               ON CONFLICT(id) DO UPDATE SET
                 access_token=excluded.access_token,
                 user_id=excluded.user_id,
                 device_id=excluded.device_id,
                 email=excluded.email,
                 device_name=excluded.device_name"#,
            params![
                session.access_token,
                session.user_id,
                session.device_id,
                session.email,
                session.device_name
            ],
        )?;
        Ok(())
    }

    pub fn get_session(&self) -> Result<Option<AuthSession>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT access_token, user_id, device_id, email, device_name FROM session WHERE id = 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some(AuthSession {
                access_token: row.get(0)?,
                user_id: row.get(1)?,
                device_id: row.get(2)?,
                email: row.get(3)?,
                device_name: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn clear_session(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM session", [])?;
        Ok(())
    }

    pub fn upsert_sync_root(&self, root_id: &str, local_path: &Path) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT INTO sync_roots (root_id, local_path, enabled)
               VALUES (?1, ?2, 1)
               ON CONFLICT(root_id) DO UPDATE SET local_path=excluded.local_path, enabled=1"#,
            params![root_id, local_path.to_string_lossy()],
        )?;
        Ok(())
    }

    pub fn list_sync_roots(&self) -> Result<Vec<(String, PathBuf, bool)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT root_id, local_path, enabled FROM sync_roots")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                PathBuf::from(row.get::<_, String>(1)?),
                row.get::<_, i64>(2)? != 0,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn upsert_file(&self, file: &IndexedFile) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT INTO files (root_id, relative_path, size, mtime_ms, content_hash, tombstone, tombstone_at_ms)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
               ON CONFLICT(root_id, relative_path) DO UPDATE SET
                 size=excluded.size,
                 mtime_ms=excluded.mtime_ms,
                 content_hash=excluded.content_hash,
                 tombstone=excluded.tombstone,
                 tombstone_at_ms=excluded.tombstone_at_ms"#,
            params![
                file.root_id,
                file.relative_path,
                file.size as i64,
                file.mtime_ms,
                file.content_hash,
                if file.tombstone { 1 } else { 0 },
                file.tombstone_at_ms
            ],
        )?;
        Ok(())
    }

    pub fn list_files(&self, root_id: &str) -> Result<Vec<IndexedFile>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT root_id, relative_path, size, mtime_ms, content_hash, tombstone, tombstone_at_ms FROM files WHERE root_id = ?1",
        )?;
        let rows = stmt.query_map(params![root_id], |row| {
            Ok(IndexedFile {
                root_id: row.get(0)?,
                relative_path: row.get(1)?,
                size: row.get::<_, i64>(2)? as u64,
                mtime_ms: row.get(3)?,
                content_hash: row.get(4)?,
                tombstone: row.get::<_, i64>(5)? != 0,
                tombstone_at_ms: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT INTO settings (key, value) VALUES (?1, ?2)
               ON CONFLICT(key) DO UPDATE SET value=excluded.value"#,
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_transfer(
        &self,
        id: &str,
        kind: &str,
        path: &str,
        peer_id: &str,
        bytes_done: u64,
        bytes_total: u64,
        state: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT INTO transfers (id, kind, path, peer_id, bytes_done, bytes_total, state)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
               ON CONFLICT(id) DO UPDATE SET
                 bytes_done=excluded.bytes_done,
                 bytes_total=excluded.bytes_total,
                 state=excluded.state"#,
            params![
                id,
                kind,
                path,
                peer_id,
                bytes_done as i64,
                bytes_total as i64,
                state
            ],
        )?;
        Ok(())
    }

    pub fn list_transfers(&self) -> Result<Vec<crate::TransferStatus>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, kind, path, peer_id, bytes_done, bytes_total, state FROM transfers ORDER BY rowid DESC LIMIT 50",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::TransferStatus {
                id: row.get(0)?,
                kind: row.get(1)?,
                path: row.get(2)?,
                peer_id: row.get(3)?,
                bytes_done: row.get::<_, i64>(4)? as u64,
                bytes_total: row.get::<_, i64>(5)? as u64,
                state: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}
