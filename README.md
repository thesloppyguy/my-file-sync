# my-file-sync

Desktop P2P file share, folder sync, and clipboard sync across your devices.

- **Client:** Tauri 2 + React (macOS / Windows / Linux)
- **P2P:** rust-libp2p (no iroh)
- **Registry:** Node/TS Fastify service — email OTP device linking, no passwords

## Workspace

```
apps/desktop          Tauri + React UI
crates/sync-core      libp2p swarm + sync engine
services/registry     OTP + device directory API
services/relay        circuit-relay v2 node
docs/                 architecture notes
```

## Quick start

### 1. Registry

```bash
cd services/registry
npm install
npm run dev
```

OTPs print to the registry console when SMTP is unset. Dev helper: `GET http://127.0.0.1:8787/debug/otps`

### 2. Relay (optional, for WAN / NAT)

```bash
cargo run -p mysync-relay
```

Copy the printed peer multiaddr into `RELAY_MULTIADDR`, then restart the registry.

### 3. Desktop app

```bash
cd apps/desktop
npm install
npm run tauri dev
```

## Packaging

From `apps/desktop`:

```bash
npm run tauri build
```

Bundle targets are configured for deb/rpm/AppImage, MSI/NSIS, and macOS app/dmg.

## Environment

Copy `.env.example`. Important vars:

- `MYSYNC_REGISTRY_URL` — desktop → registry (default `http://127.0.0.1:8787`)
- `RELAY_MULTIADDR` — advertised to clients via `/config`
- `SMTP_*` — optional email delivery for OTPs

Postgres via `docker compose up -d` is available for a future registry backend; the MVP registry uses SQLite locally (`services/registry/data/registry.sqlite`).
