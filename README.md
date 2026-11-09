# My Share

Desktop P2P file share, folder sync, and clipboard sync across your devices.

- **Client:** Tauri 2 + React (macOS / Windows / Linux)
- **P2P:** rust-libp2p (no iroh)
- **Registry:** hosted separately — see sibling [`mysync-registry`](../mysync-registry) (email OTP device linking, no passwords)

## Workspace

```
apps/desktop          Tauri + React UI
crates/sync-core      libp2p swarm + sync engine
services/relay        circuit-relay v2 node
docs/                 architecture notes
```

Control-plane (OTP, device directory DB, public website/API) lives in **`mysync-registry`**, not this repo.

## Quick start

### 1. Registry (separate repo)

```bash
cd ../mysync-registry
npm install
npm run dev
```

OTPs print to the registry console when SMTP is unset. Dev helper: `GET http://127.0.0.1:8787/debug/otps`

### 2. Relay (optional, for WAN / NAT)

```bash
cargo run -p mysync-relay
```

Copy the printed peer multiaddr into `RELAY_MULTIADDR` in the registry repo, then restart the registry.

### 3. Desktop app

```bash
cd apps/desktop
npm install
npm run tauri dev
```

Point the app at your registry:

```bash
# from repo root
cp .env.example .env
# MYSYNC_REGISTRY_URL=http://127.0.0.1:8787
```

## Packaging

From `apps/desktop`:

```bash
npm run tauri build
```

Bundle targets are configured for deb/rpm/AppImage, MSI/NSIS, and macOS app/dmg.

## Environment

- `MYSYNC_REGISTRY_URL` — desktop → registry (default `http://127.0.0.1:8787`)

Registry SMTP, SQLite/Postgres, and `RELAY_MULTIADDR` are configured in the **mysync-registry** repo.
