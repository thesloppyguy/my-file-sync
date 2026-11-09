mod registry_client;

use parking_lot::Mutex;
use registry_client::{DeviceInfo, RegistryClient};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use sync_core::{
    load_or_create_identity, AuthSession, LocalStore, NodeEvent, SyncNode, SyncNodeConfig,
    SyncPath, TransferStatus,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::RwLock;

const DEFAULT_REGISTRY: &str = "http://127.0.0.1:8787";

struct AppState {
    registry: RegistryClient,
    store: Arc<LocalStore>,
    node: RwLock<Option<Arc<SyncNode>>>,
    clipboard_sync: Mutex<bool>,
    last_clipboard_hash: Mutex<Option<String>>,
    app_dir: PathBuf,
}

#[derive(Serialize)]
struct SessionView {
    email: String,
    user_id: String,
    device_id: String,
    device_name: String,
    peer_id: String,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("my_file_sync=info".parse().unwrap())
                .add_directive("sync_core=info".parse().unwrap()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let app_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from(".mysync-data"));
            std::fs::create_dir_all(&app_dir)?;
            let store = Arc::new(LocalStore::open(&app_dir.join("local.sqlite"))?);
            let registry_url = std::env::var("MYSYNC_REGISTRY_URL")
                .unwrap_or_else(|_| DEFAULT_REGISTRY.to_string());

            let state = AppState {
                registry: RegistryClient::new(registry_url),
                store,
                node: RwLock::new(None),
                clipboard_sync: Mutex::new(false),
                last_clipboard_hash: Mutex::new(None),
                app_dir: app_dir.clone(),
            };
            app.manage(state);

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = bootstrap_node(handle.clone()).await {
                    tracing::error!("bootstrap failed: {err:#}");
                }
                spawn_background_tasks(handle);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            request_otp,
            verify_otp,
            get_session,
            list_devices,
            share_files,
            add_sync_path,
            list_sync_paths,
            set_clipboard_sync,
            get_clipboard_sync,
            get_transfer_status,
            get_peer_id,
            get_connected_peers,
            unlink_device,
            logout
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn bootstrap_node(app: AppHandle) -> anyhow::Result<()> {
    let state = app.state::<AppState>();
    let identity = load_or_create_identity(&state.app_dir)?;
    let relay = state.registry.relay_multiaddr().await.ok().flatten();
    let session = state.store.get_session()?;
    let user_id = session.as_ref().map(|s| s.user_id.clone());

    let node = SyncNode::start(
        identity,
        SyncNodeConfig {
            app_dir: state.app_dir.clone(),
            listen_port: 45221,
            relay_addr: relay,
            user_id: user_id.clone(),
        },
        state.store.clone(),
    )
    .await?;

    if let Some(session) = session {
        let _ = node.set_user_id(session.user_id.clone()).await;
        let addrs = node.listen_addrs().await.unwrap_or_default();
        let _ = state
            .registry
            .update_me(
                &session.access_token,
                &node.peer_id(),
                &addrs,
                Some(&session.device_name),
            )
            .await;
    }

    let mut events = node.subscribe();
    let emit_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = events.recv().await {
            match &event {
                NodeEvent::ClipboardReceived { text, .. } => {
                    let state = emit_handle.state::<AppState>();
                    if *state.clipboard_sync.lock() {
                        use tauri_plugin_clipboard_manager::ClipboardExt;
                        let hash = hex::encode(Sha256::digest(text.as_bytes()));
                        *state.last_clipboard_hash.lock() = Some(hash);
                        let _ = emit_handle.clipboard().write_text(text);
                    }
                }
                _ => {}
            }
            let _ = emit_handle.emit("node-event", event);
        }
    });

    *state.node.write().await = Some(Arc::new(node));
    Ok(())
}

fn spawn_background_tasks(app: AppHandle) {
    // Heartbeat + dial peers
    let app_heartbeat = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(20));
        loop {
            interval.tick().await;
            let state = app_heartbeat.state::<AppState>();
            let Ok(Some(session)) = state.store.get_session() else {
                continue;
            };
            let _ = state.registry.heartbeat(&session.access_token).await;
            if let Ok(devices) = state.registry.list_devices(&session.access_token).await {
                let node = state.node.read().await.clone();
                if let Some(node) = node {
                    for device in devices.into_iter().filter(|d| !d.is_self) {
                        if let Some(peer_id) = device.peer_id {
                            let _ = node.dial(&peer_id, device.multiaddrs).await;
                        }
                    }
                }
            }
        }
    });

    let app_clipboard = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            interval.tick().await;
            let state = app_clipboard.state::<AppState>();
            if !*state.clipboard_sync.lock() {
                continue;
            }
            use tauri_plugin_clipboard_manager::ClipboardExt;
            let Ok(text) = app_clipboard.clipboard().read_text() else {
                continue;
            };
            let hash = hex::encode(Sha256::digest(text.as_bytes()));
            {
                let mut last = state.last_clipboard_hash.lock();
                if last.as_deref() == Some(hash.as_str()) {
                    continue;
                }
                *last = Some(hash);
            }
            let node = state.node.read().await.clone();
            if let Some(node) = node {
                let _ = node.publish_clipboard(text).await;
            }
        }
    });
}

fn device_fingerprint(app_dir: &PathBuf) -> String {
    let raw = format!(
        "{}|{}|{}",
        whoami_fallback(),
        std::env::consts::OS,
        app_dir.to_string_lossy()
    );
    hex::encode(Sha256::digest(raw.as_bytes()))
}

fn whoami_fallback() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".into())
}

#[tauri::command]
async fn request_otp(state: State<'_, AppState>, email: String) -> Result<(), String> {
    state
        .registry
        .request_otp(&email)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn verify_otp(
    state: State<'_, AppState>,
    email: String,
    otp: String,
    device_name: String,
) -> Result<SessionView, String> {
    let node = state
        .node
        .read()
        .await
        .clone()
        .ok_or_else(|| "P2P node not ready".to_string())?;
    let peer_id = node.peer_id();
    let addrs = node.listen_addrs().await.map_err(|e| e.to_string())?;
    let fp = device_fingerprint(&state.app_dir);
    let verified = state
        .registry
        .verify_otp(
            &email,
            &otp,
            &device_name,
            &fp,
            &peer_id,
            &addrs,
            std::env::consts::OS,
        )
        .await
        .map_err(|e| e.to_string())?;

    state
        .store
        .save_session(&AuthSession {
            access_token: verified.access_token.clone(),
            user_id: verified.user_id.clone(),
            device_id: verified.device_id.clone(),
            email: verified.email.clone(),
            device_name: device_name.clone(),
        })
        .map_err(|e| e.to_string())?;

    node.set_user_id(verified.user_id.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(SessionView {
        email: verified.email,
        user_id: verified.user_id,
        device_id: verified.device_id,
        device_name,
        peer_id,
    })
}

#[tauri::command]
async fn get_session(state: State<'_, AppState>) -> Result<Option<SessionView>, String> {
    let session = state.store.get_session().map_err(|e| e.to_string())?;
    let Some(session) = session else {
        return Ok(None);
    };
    let peer_id = state
        .node
        .read()
        .await
        .as_ref()
        .map(|n| n.peer_id())
        .unwrap_or_default();
    Ok(Some(SessionView {
        email: session.email,
        user_id: session.user_id,
        device_id: session.device_id,
        device_name: session.device_name,
        peer_id,
    }))
}

#[tauri::command]
async fn list_devices(state: State<'_, AppState>) -> Result<Vec<DeviceInfo>, String> {
    let session = state
        .store
        .get_session()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Not signed in".to_string())?;
    state
        .registry
        .list_devices(&session.access_token)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn share_files(
    state: State<'_, AppState>,
    peer_id: String,
    paths: Vec<String>,
) -> Result<Vec<String>, String> {
    let node = state
        .node
        .read()
        .await
        .clone()
        .ok_or_else(|| "P2P node not ready".to_string())?;
    let mut ids = Vec::new();
    for path in paths {
        let id = node
            .share_file(&peer_id, PathBuf::from(path))
            .await
            .map_err(|e| e.to_string())?;
        ids.push(id);
    }
    Ok(ids)
}

#[tauri::command]
async fn add_sync_path(state: State<'_, AppState>, path: String) -> Result<SyncPath, String> {
    let node = state
        .node
        .read()
        .await
        .clone()
        .ok_or_else(|| "P2P node not ready".to_string())?;
    node.sync_engine()
        .add_path(PathBuf::from(path).as_path())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_sync_paths(state: State<'_, AppState>) -> Result<Vec<SyncPath>, String> {
    let node = state
        .node
        .read()
        .await
        .clone()
        .ok_or_else(|| "P2P node not ready".to_string())?;
    node.sync_engine()
        .list_paths()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_clipboard_sync(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    *state.clipboard_sync.lock() = enabled;
    state
        .store
        .set_setting("clipboard_sync", if enabled { "1" } else { "0" })
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_clipboard_sync(state: State<'_, AppState>) -> Result<bool, String> {
    if let Ok(Some(v)) = state.store.get_setting("clipboard_sync") {
        let enabled = v == "1";
        *state.clipboard_sync.lock() = enabled;
        return Ok(enabled);
    }
    Ok(*state.clipboard_sync.lock())
}

#[tauri::command]
async fn get_transfer_status(state: State<'_, AppState>) -> Result<Vec<TransferStatus>, String> {
    state.store.list_transfers().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_peer_id(state: State<'_, AppState>) -> Result<String, String> {
    let node = state
        .node
        .read()
        .await
        .clone()
        .ok_or_else(|| "P2P node not ready".to_string())?;
    Ok(node.peer_id())
}

#[tauri::command]
async fn get_connected_peers(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let node = state
        .node
        .read()
        .await
        .clone()
        .ok_or_else(|| "P2P node not ready".to_string())?;
    node.connected_peers().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn unlink_device(state: State<'_, AppState>, device_id: String) -> Result<(), String> {
    let session = state
        .store
        .get_session()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Not signed in".to_string())?;
    state
        .registry
        .unlink_device(&session.access_token, &device_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    state.store.clear_session().map_err(|e| e.to_string())
}

