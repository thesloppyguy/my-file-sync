use crate::clipboard::{clipboard_topic, presence_topic, ClipboardPayload};
use crate::identity::Identity;
use crate::protocols::{
    ClipboardMessage, FileChunkRequest, FileChunkResponse, FileOffer, FileRequest, FileResponse,
    SyncManifest, SyncRequest, SyncResponse, FILE_PROTOCOL, SYNC_PROTOCOL,
};
use crate::store::LocalStore;
use crate::sync_engine::SyncEngine;
use crate::TransferStatus;
use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic, MessageAuthenticity};
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, ProtocolSupport, ResponseChannel};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{
    dcutr, identify, mdns, noise, ping, relay, tcp, yamux, Multiaddr, PeerId, StreamProtocol,
    Swarm, SwarmBuilder,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{info, warn};
use uuid::Uuid;

const CHUNK_SIZE: u32 = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncNodeConfig {
    pub app_dir: PathBuf,
    pub listen_port: u16,
    pub relay_addr: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeEvent {
    PeerDiscovered { peer_id: String },
    PeerConnected { peer_id: String },
    PeerDisconnected { peer_id: String },
    ClipboardReceived { text: String, origin_peer: String },
    TransferProgress(TransferStatus),
    Error { message: String },
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    mdns: mdns::tokio::Behaviour,
    gossipsub: gossipsub::Behaviour,
    relay_client: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,
    file_rr: request_response::cbor::Behaviour<FileRequest, FileResponse>,
    sync_rr: request_response::cbor::Behaviour<SyncRequest, SyncResponse>,
}

enum Command {
    Dial {
        peer_id: PeerId,
        addrs: Vec<Multiaddr>,
        respond: oneshot::Sender<Result<()>>,
    },
    ShareFile {
        peer_id: PeerId,
        path: PathBuf,
        respond: oneshot::Sender<Result<String>>,
    },
    PublishClipboard {
        text: String,
        respond: oneshot::Sender<Result<()>>,
    },
    SetUserId {
        user_id: String,
        respond: oneshot::Sender<Result<()>>,
    },
    ListenAddrs {
        respond: oneshot::Sender<Vec<String>>,
    },
    ConnectedPeers {
        respond: oneshot::Sender<Vec<String>>,
    },
    Shutdown,
}

pub struct SyncNode {
    cmd_tx: mpsc::UnboundedSender<Command>,
    events: broadcast::Sender<NodeEvent>,
    store: Arc<LocalStore>,
    sync_engine: Arc<SyncEngine>,
    peer_id: PeerId,
}

impl SyncNode {
    pub async fn start(
        identity: Identity,
        config: SyncNodeConfig,
        store: Arc<LocalStore>,
    ) -> Result<Self> {
        let sync_engine = Arc::new(SyncEngine::new(store.clone()));
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(256);
        let pending_outgoing = Arc::new(Mutex::new(HashMap::new()));

        let peer_id = identity.peer_id;
        let events_clone = events.clone();
        let store_clone = store.clone();
        let engine_clone = sync_engine.clone();
        let pending_clone = pending_outgoing.clone();

        tokio::spawn(async move {
            if let Err(err) = run_swarm(
                identity,
                config,
                cmd_rx,
                events_clone.clone(),
                store_clone,
                engine_clone,
                pending_clone,
            )
            .await
            {
                let _ = events_clone.send(NodeEvent::Error {
                    message: err.to_string(),
                });
            }
        });

        Ok(Self {
            cmd_tx,
            events,
            store,
            sync_engine,
            peer_id,
        })
    }

    pub fn peer_id(&self) -> String {
        self.peer_id.to_string()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NodeEvent> {
        self.events.subscribe()
    }

    pub fn store(&self) -> Arc<LocalStore> {
        self.store.clone()
    }

    pub fn sync_engine(&self) -> Arc<SyncEngine> {
        self.sync_engine.clone()
    }

    pub async fn dial(&self, peer_id: &str, addrs: Vec<String>) -> Result<()> {
        let peer_id: PeerId = peer_id.parse().context("invalid peer id")?;
        let addrs = addrs
            .into_iter()
            .map(|a| a.parse::<Multiaddr>())
            .collect::<Result<Vec<_>, _>>()
            .context("invalid multiaddr")?;
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Dial {
                peer_id,
                addrs,
                respond: tx,
            })
            .map_err(|_| anyhow!("node stopped"))?;
        rx.await?
    }

    pub async fn share_file(&self, peer_id: &str, path: PathBuf) -> Result<String> {
        let peer_id: PeerId = peer_id.parse().context("invalid peer id")?;
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::ShareFile {
                peer_id,
                path,
                respond: tx,
            })
            .map_err(|_| anyhow!("node stopped"))?;
        rx.await?
    }

    pub async fn publish_clipboard(&self, text: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::PublishClipboard {
                text,
                respond: tx,
            })
            .map_err(|_| anyhow!("node stopped"))?;
        rx.await?
    }

    pub async fn set_user_id(&self, user_id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::SetUserId {
                user_id,
                respond: tx,
            })
            .map_err(|_| anyhow!("node stopped"))?;
        rx.await?
    }

    pub async fn listen_addrs(&self) -> Result<Vec<String>> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::ListenAddrs { respond: tx })
            .map_err(|_| anyhow!("node stopped"))?;
        Ok(rx.await?)
    }

    pub async fn connected_peers(&self) -> Result<Vec<String>> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::ConnectedPeers { respond: tx })
            .map_err(|_| anyhow!("node stopped"))?;
        Ok(rx.await?)
    }

    pub fn shutdown(&self) {
        let _ = self.cmd_tx.send(Command::Shutdown);
    }
}

async fn run_swarm(
    identity: Identity,
    config: SyncNodeConfig,
    mut cmd_rx: mpsc::UnboundedReceiver<Command>,
    events: broadcast::Sender<NodeEvent>,
    store: Arc<LocalStore>,
    sync_engine: Arc<SyncEngine>,
    pending_outgoing: Arc<Mutex<HashMap<String, PathBuf>>>,
) -> Result<()> {
    let mut swarm = build_swarm(&identity)?;

    let listen: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", config.listen_port).parse()?;
    swarm.listen_on(listen)?;
    let listen_quic: Multiaddr =
        format!("/ip4/0.0.0.0/udp/{}/quic-v1", config.listen_port).parse()?;
    let _ = swarm.listen_on(listen_quic);

    if let Some(relay) = &config.relay_addr {
        if let Ok(addr) = relay.parse::<Multiaddr>() {
            let _ = swarm.dial(addr.clone());
            // Reserve a relay circuit once connected; listen via relay if possible.
            if let Some(peer) = peer_id_from_multiaddr(&addr) {
                let circuit = addr
                    .clone()
                    .with(Protocol::P2pCircuit)
                    .with(Protocol::P2p(identity.peer_id));
                info!("relay circuit listen candidate via {peer}: {circuit}");
                let _ = swarm.listen_on(circuit);
            }
        }
    }

    let mut known_peers: HashSet<PeerId> = HashSet::new();
    let mut incoming_files: HashMap<String, IncomingTransfer> = HashMap::new();
    let mut user_id = config.user_id.clone();

    if let Some(uid) = &user_id {
        subscribe_topics(&mut swarm, uid)?;
    }

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(Command::Dial { peer_id, addrs, respond }) => {
                        let mut ok = Ok(());
                        for addr in addrs {
                            if let Err(err) = swarm.dial(addr.with(Protocol::P2p(peer_id))) {
                                ok = Err(anyhow!(err));
                            } else {
                                ok = Ok(());
                                break;
                            }
                        }
                        if ok.is_err() {
                            // Try dialing by peer id alone (may use known addresses).
                            ok = swarm.dial(peer_id).map_err(|e| anyhow!(e));
                        }
                        let _ = respond.send(ok);
                    }
                    Some(Command::ShareFile { peer_id, path, respond }) => {
                        let result = offer_file(&mut swarm, &store, &pending_outgoing, peer_id, path);
                        let _ = respond.send(result);
                    }
                    Some(Command::PublishClipboard { text, respond }) => {
                        let result = publish_clipboard_msg(&mut swarm, &identity, &user_id, text);
                        let _ = respond.send(result);
                    }
                    Some(Command::SetUserId { user_id: uid, respond }) => {
                        let result = subscribe_topics(&mut swarm, &uid);
                        if result.is_ok() {
                            user_id = Some(uid);
                        }
                        let _ = respond.send(result);
                    }
                    Some(Command::ListenAddrs { respond }) => {
                        let addrs = swarm.listeners().map(|a| a.to_string()).collect();
                        let _ = respond.send(addrs);
                    }
                    Some(Command::ConnectedPeers { respond }) => {
                        let peers = swarm.connected_peers().map(|p| p.to_string()).collect();
                        let _ = respond.send(peers);
                    }
                    Some(Command::Shutdown) | None => break,
                }
            }
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("listening on {address}");
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        known_peers.insert(peer_id);
                        let _ = events.send(NodeEvent::PeerConnected {
                            peer_id: peer_id.to_string(),
                        });
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } => {
                        let _ = events.send(NodeEvent::PeerDisconnected {
                            peer_id: peer_id.to_string(),
                        });
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                        for (peer, addr) in list {
                            known_peers.insert(peer);
                            let _ = swarm.dial(addr.with(Protocol::P2p(peer)));
                            let _ = events.send(NodeEvent::PeerDiscovered {
                                peer_id: peer.to_string(),
                            });
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                        propagation_source,
                        message,
                        ..
                    })) => {
                        if let Ok(msg) = serde_json::from_slice::<ClipboardMessage>(&message.data) {
                            if msg.origin_peer != identity.peer_id.to_string() {
                                let payload = ClipboardPayload::from_message(msg);
                                let _ = events.send(NodeEvent::ClipboardReceived {
                                    text: payload.text,
                                    origin_peer: payload.origin_peer,
                                });
                            }
                        } else {
                            let _ = propagation_source;
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::FileRr(ev)) => {
                        handle_file_rr(
                            &mut swarm,
                            ev,
                            &store,
                            &sync_engine,
                            &pending_outgoing,
                            &mut incoming_files,
                            &events,
                            &identity,
                        );
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::SyncRr(ev)) => {
                        handle_sync_rr(&mut swarm, ev, &sync_engine);
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn build_swarm(identity: &Identity) -> Result<Swarm<Behaviour>> {
    let keypair = identity.keypair.clone();

    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_relay_client(noise::Config::new, yamux::Config::default)?
        .with_behaviour(|key, relay_client| {
            let peer_id = key.public().to_peer_id();
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(10))
                .validation_mode(gossipsub::ValidationMode::Permissive)
                .build()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

            let gossipsub = gossipsub::Behaviour::new(
                MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

            let file_rr = request_response::cbor::Behaviour::new(
                [(StreamProtocol::new(FILE_PROTOCOL), ProtocolSupport::Full)],
                request_response::Config::default(),
            );
            let sync_rr = request_response::cbor::Behaviour::new(
                [(StreamProtocol::new(SYNC_PROTOCOL), ProtocolSupport::Full)],
                request_response::Config::default(),
            );

            Ok(Behaviour {
                identify: identify::Behaviour::new(identify::Config::new(
                    "mysync/1.0.0".into(),
                    key.public(),
                )),
                ping: ping::Behaviour::default(),
                mdns: mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?,
                gossipsub,
                relay_client,
                dcutr: dcutr::Behaviour::new(peer_id),
                file_rr,
                sync_rr,
            })
        })?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    Ok(swarm)
}

fn subscribe_topics(swarm: &mut Swarm<Behaviour>, user_id: &str) -> Result<()> {
    let clip = IdentTopic::new(clipboard_topic(user_id));
    let presence = IdentTopic::new(presence_topic(user_id));
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&clip)
        .map_err(|e| anyhow!(e))?;
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&presence)
        .map_err(|e| anyhow!(e))?;
    Ok(())
}

fn publish_clipboard_msg(
    swarm: &mut Swarm<Behaviour>,
    identity: &Identity,
    user_id: &Option<String>,
    text: String,
) -> Result<()> {
    let uid = user_id.as_ref().context("user id not set")?;
    let payload = ClipboardPayload::new(text, identity.peer_id.to_string());
    let topic = IdentTopic::new(clipboard_topic(uid));
    let data = serde_json::to_vec(&payload.to_message())?;
    swarm
        .behaviour_mut()
        .gossipsub
        .publish(topic, data)
        .map_err(|e| anyhow!(e))?;
    Ok(())
}

fn offer_file(
    swarm: &mut Swarm<Behaviour>,
    store: &LocalStore,
    pending: &Mutex<HashMap<String, PathBuf>>,
    peer_id: PeerId,
    path: PathBuf,
) -> Result<String> {
    let meta = fs::metadata(&path)?;
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let transfer_id = Uuid::new_v4().to_string();
    let content_hash = crate::sync_engine::hash_bytes(&fs::read(&path)?);
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    pending.lock().insert(transfer_id.clone(), path.clone());
    store.upsert_transfer(
        &transfer_id,
        "share",
        &path.to_string_lossy(),
        &peer_id.to_string(),
        0,
        meta.len(),
        "offered",
    )?;

    swarm.behaviour_mut().file_rr.send_request(
        &peer_id,
        FileRequest::Offer(FileOffer {
            transfer_id: transfer_id.clone(),
            file_name,
            relative_path: None,
            size: meta.len(),
            content_hash,
            mtime_ms,
        }),
    );
    Ok(transfer_id)
}

struct IncomingTransfer {
    path: PathBuf,
    size: u64,
    file: File,
    bytes_done: u64,
}

fn handle_file_rr(
    swarm: &mut Swarm<Behaviour>,
    event: request_response::Event<FileRequest, FileResponse>,
    store: &LocalStore,
    sync_engine: &SyncEngine,
    pending: &Mutex<HashMap<String, PathBuf>>,
    incoming: &mut HashMap<String, IncomingTransfer>,
    events: &broadcast::Sender<NodeEvent>,
    identity: &Identity,
) {
    match event {
        request_response::Event::Message { peer, message, .. } => match message {
            request_response::Message::Request {
                request, channel, ..
            } => match request {
                FileRequest::Offer(offer) => {
                    let dest_dir = dirs::download_dir()
                        .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
                        .unwrap_or_else(|| PathBuf::from("."));
                    let dest = dest_dir.join(&offer.file_name);
                    match prepare_incoming(&offer, &dest, peer) {
                        Ok(xfer) => {
                            incoming.insert(offer.transfer_id.clone(), xfer);
                            let _ = store.upsert_transfer(
                                &offer.transfer_id,
                                "receive",
                                &dest.to_string_lossy(),
                                &peer.to_string(),
                                0,
                                offer.size,
                                "receiving",
                            );
                            let _ = swarm.behaviour_mut().file_rr.send_response(
                                channel,
                                FileResponse::Accept {
                                    transfer_id: offer.transfer_id.clone(),
                                },
                            );
                            // Ask for first chunk
                            swarm.behaviour_mut().file_rr.send_request(
                                &peer,
                                FileRequest::Chunk(FileChunkRequest {
                                    transfer_id: offer.transfer_id,
                                    offset: 0,
                                    length: CHUNK_SIZE,
                                }),
                            );
                        }
                        Err(err) => {
                            let _ = swarm.behaviour_mut().file_rr.send_response(
                                channel,
                                FileResponse::Reject {
                                    reason: err.to_string(),
                                },
                            );
                        }
                    }
                }
                FileRequest::Chunk(req) => {
                    let response = read_chunk(pending, &req);
                    let _ = swarm.behaviour_mut().file_rr.send_response(channel, response);
                }
            },
            request_response::Message::Response { response, .. } => match response {
                FileResponse::Accept { transfer_id } => {
                    let _ = store.upsert_transfer(
                        &transfer_id,
                        "share",
                        "",
                        &peer.to_string(),
                        0,
                        0,
                        "accepted",
                    );
                }
                FileResponse::Reject { reason } => {
                    warn!("file rejected: {reason}");
                }
                FileResponse::Chunk(chunk) => {
                    if let Some(xfer) = incoming.get_mut(&chunk.transfer_id) {
                        if let Err(err) = xfer.file.write_all(&chunk.data) {
                            warn!("write chunk failed: {err}");
                            return;
                        }
                        xfer.bytes_done = chunk.offset + chunk.data.len() as u64;
                        let status = TransferStatus {
                            id: chunk.transfer_id.clone(),
                            kind: "receive".into(),
                            path: xfer.path.to_string_lossy().to_string(),
                            peer_id: peer.to_string(),
                            bytes_done: xfer.bytes_done,
                            bytes_total: xfer.size,
                            state: if chunk.eof {
                                "done".into()
                            } else {
                                "receiving".into()
                            },
                        };
                        let _ = store.upsert_transfer(
                            &status.id,
                            &status.kind,
                            &status.path,
                            &status.peer_id,
                            status.bytes_done,
                            status.bytes_total,
                            &status.state,
                        );
                        let _ = events.send(NodeEvent::TransferProgress(status));
                        if chunk.eof {
                            incoming.remove(&chunk.transfer_id);
                        } else {
                            swarm.behaviour_mut().file_rr.send_request(
                                &peer,
                                FileRequest::Chunk(FileChunkRequest {
                                    transfer_id: chunk.transfer_id,
                                    offset: chunk.offset + chunk.data.len() as u64,
                                    length: CHUNK_SIZE,
                                }),
                            );
                        }
                    }
                }
                FileResponse::Done { .. } => {}
            },
        },
        request_response::Event::OutboundFailure { error, .. } => {
            warn!("file outbound failure: {error}");
            let _ = identity;
            let _ = sync_engine;
        }
        request_response::Event::InboundFailure { error, .. } => {
            warn!("file inbound failure: {error}");
        }
        request_response::Event::ResponseSent { .. } => {}
    }
}

fn prepare_incoming(offer: &FileOffer, dest: &Path, peer: PeerId) -> Result<IncomingTransfer> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(dest)?;
    let _ = peer;
    Ok(IncomingTransfer {
        path: dest.to_path_buf(),
        size: offer.size,
        file,
        bytes_done: 0,
    })
}

fn read_chunk(
    pending: &Mutex<HashMap<String, PathBuf>>,
    req: &FileChunkRequest,
) -> FileResponse {
    let path = match pending.lock().get(&req.transfer_id).cloned() {
        Some(p) => p,
        None => {
            return FileResponse::Reject {
                reason: "unknown transfer".into(),
            }
        }
    };
    match read_file_chunk(&path, req.offset, req.length) {
        Ok((data, eof)) => FileResponse::Chunk(FileChunkResponse {
            transfer_id: req.transfer_id.clone(),
            offset: req.offset,
            data,
            eof,
        }),
        Err(err) => FileResponse::Reject {
            reason: err.to_string(),
        },
    }
}

fn read_file_chunk(path: &Path, offset: u64, length: u32) -> Result<(Vec<u8>, bool)> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; length as usize];
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    let eof = n == 0 || (offset + n as u64) >= fs::metadata(path)?.len();
    Ok((buf, eof))
}

fn handle_sync_rr(
    swarm: &mut Swarm<Behaviour>,
    event: request_response::Event<SyncRequest, SyncResponse>,
    sync_engine: &SyncEngine,
) {
    match event {
        request_response::Event::Message { message, .. } => {
            if let request_response::Message::Request {
                request, channel, ..
            } = message
            {
                let response = match request {
                    SyncRequest::GetManifest { root_id } => {
                        match sync_engine.build_manifest(&root_id) {
                            Ok(manifest) => SyncResponse::Manifest(manifest),
                            Err(err) => SyncResponse::Error {
                                message: err.to_string(),
                            },
                        }
                    }
                    SyncRequest::PullFile {
                        root_id,
                        relative_path,
                    } => match build_sync_offer(sync_engine, &root_id, &relative_path) {
                        Ok(offer) => SyncResponse::FileOffer(offer),
                        Err(err) => SyncResponse::Error {
                            message: err.to_string(),
                        },
                    },
                };
                let _ = swarm
                    .behaviour_mut()
                    .sync_rr
                    .send_response(channel, response);
            }
        }
        _ => {}
    }
}

fn build_sync_offer(
    sync_engine: &SyncEngine,
    root_id: &str,
    relative_path: &str,
) -> Result<FileOffer> {
    let root = sync_engine.resolve_local_path(root_id)?;
    let path = root.join(relative_path);
    let meta = fs::metadata(&path)?;
    let content_hash = crate::sync_engine::hash_bytes(&fs::read(&path)?);
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Ok(FileOffer {
        transfer_id: Uuid::new_v4().to_string(),
        file_name: Path::new(relative_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string(),
        relative_path: Some(relative_path.to_string()),
        size: meta.len(),
        content_hash,
        mtime_ms,
    })
}

fn peer_id_from_multiaddr(addr: &Multiaddr) -> Option<PeerId> {
    addr.iter().find_map(|p| match p {
        Protocol::P2p(peer) => Some(peer),
        _ => None,
    })
}

// Silence unused import in some feature combinations.
#[allow(dead_code)]
fn _unused(_ch: ResponseChannel<FileResponse>, _m: SyncManifest) {}
