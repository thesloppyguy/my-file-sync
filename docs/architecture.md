# Architecture

## Components

- `apps/desktop` — Tauri 2 + React client for macOS, Windows, and Linux
- `crates/sync-core` — shared rust-libp2p swarm, file/sync/clipboard protocols, SQLite index
- `services/relay` — libp2p circuit-relay v2 helper for NAT traversal
- **mysync-registry** (separate repo) — hosted Fastify control plane (email OTP + device directory + website)

## Auth model

No passwords. A device is linked with:

1. `POST /auth/request-otp`
2. `POST /auth/verify-otp` with device fingerprint + PeerId
3. Long-lived bearer token stored locally

Same email = same account = sync group.

## Data vs control plane

- Registry (hosted separately) stores user/device metadata only (email, fingerprint hash, PeerId, multiaddrs).
- File bytes, sync payloads, and clipboard text travel over libp2p (Noise-encrypted transports).
- Desktop clients discover the registry via `MYSYNC_REGISTRY_URL`.

## Protocols

| Protocol | Role |
|----------|------|
| `/mysync/file/1.0.0` | Chunked P2P file transfer |
| `/mysync/sync/1.0.0` | Folder manifest exchange / pull |
| gossipsub `mysync/clipboard/<userId>` | Text clipboard sync |

## Sync semantics

- User selects folders to sync
- Manifest: relative path, size, mtime, content hash, tombstone
- Newer mtime wins
- Equal mtime + different hash → `filename (conflict remote).ext`
- Tombstones retained ~7 days

## Connectivity

1. LAN discovery via mDNS
2. Registry peer directory for WAN dials
3. Circuit relay + DCUtR when direct dial fails

## Future mobile

Keep P2P logic in `sync-core`. Tauri 2 mobile entry stays through `lib.rs`; Android/iOS init is deferred until desktop MVP is stable.
