import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

type Tab = "devices" | "share" | "sync" | "clipboard" | "settings";

type Session = {
  email: string;
  user_id: string;
  device_id: string;
  device_name: string;
  peer_id: string;
};

type Device = {
  id: string;
  name: string;
  fingerprintHash: string;
  peerId?: string | null;
  multiaddrs: string[];
  os?: string | null;
  lastSeenAt?: string | null;
  online: boolean;
  isSelf: boolean;
};

type SyncPath = {
  root_id: string;
  local_path: string;
  enabled: boolean;
};

type Transfer = {
  id: string;
  kind: string;
  path: string;
  peer_id: string;
  bytes_done: number;
  bytes_total: number;
  state: string;
};

export default function App() {
  const [session, setSession] = useState<Session | null>(null);
  const [booting, setBooting] = useState(true);
  const [tab, setTab] = useState<Tab>("devices");
  const [error, setError] = useState<string | null>(null);

  const refreshSession = useCallback(async () => {
    const s = await invoke<Session | null>("get_session");
    setSession(s);
  }, []);

  useEffect(() => {
    (async () => {
      try {
        await refreshSession();
      } catch (e) {
        setError(String(e));
      } finally {
        setBooting(false);
      }
    })();
  }, [refreshSession]);

  if (booting) {
    return (
      <div className="shell center">
        <p className="muted">Starting my-file-sync…</p>
      </div>
    );
  }

  if (!session) {
    return (
      <Onboarding
        onDone={async () => {
          await refreshSession();
        }}
        error={error}
        setError={setError}
      />
    );
  }

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">mfs</div>
          <div>
            <div className="brand-title">my-file-sync</div>
            <div className="muted tiny">{session.email}</div>
          </div>
        </div>
        <nav>
          {(
            [
              ["devices", "Devices"],
              ["share", "Share"],
              ["sync", "Sync"],
              ["clipboard", "Clipboard"],
              ["settings", "Settings"],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              className={tab === id ? "nav active" : "nav"}
              onClick={() => setTab(id)}
            >
              {label}
            </button>
          ))}
        </nav>
        <div className="sidebar-foot muted tiny">
          Peer {session.peer_id.slice(0, 12)}…
        </div>
      </aside>
      <main className="content">
        {error && <div className="banner error">{error}</div>}
        {tab === "devices" && <DevicesPanel setError={setError} />}
        {tab === "share" && <SharePanel setError={setError} />}
        {tab === "sync" && <SyncPanel setError={setError} />}
        {tab === "clipboard" && <ClipboardPanel setError={setError} />}
        {tab === "settings" && (
          <SettingsPanel
            session={session}
            setError={setError}
            onLogout={async () => {
              await invoke("logout");
              setSession(null);
            }}
          />
        )}
      </main>
    </div>
  );
}

function Onboarding({
  onDone,
  error,
  setError,
}: {
  onDone: () => Promise<void>;
  error: string | null;
  setError: (v: string | null) => void;
}) {
  const [step, setStep] = useState<"email" | "otp">("email");
  const [email, setEmail] = useState("");
  const [otp, setOtp] = useState("");
  const [deviceName, setDeviceName] = useState(
    `${navigator.platform || "Device"}`,
  );
  const [busy, setBusy] = useState(false);

  async function requestOtp() {
    setBusy(true);
    setError(null);
    try {
      await invoke("request_otp", { email });
      setStep("otp");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function verify() {
    setBusy(true);
    setError(null);
    try {
      await invoke("verify_otp", { email, otp, deviceName });
      await onDone();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="shell center">
      <div className="card onboarding">
        <h1>Link this device</h1>
        <p className="muted">
          No passwords. Enter your email, verify a one-time code, and this
          device joins your sync group.
        </p>
        {error && <div className="banner error">{error}</div>}
        {step === "email" ? (
          <>
            <label>
              Email
              <input
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="you@example.com"
              />
            </label>
            <button disabled={busy || !email.includes("@")} onClick={requestOtp}>
              {busy ? "Sending…" : "Send OTP"}
            </button>
          </>
        ) : (
          <>
            <label>
              OTP code
              <input
                value={otp}
                onChange={(e) => setOtp(e.target.value)}
                placeholder="6-digit code"
              />
            </label>
            <label>
              Device name
              <input
                value={deviceName}
                onChange={(e) => setDeviceName(e.target.value)}
              />
            </label>
            <div className="row">
              <button className="ghost" onClick={() => setStep("email")}>
                Back
              </button>
              <button disabled={busy || otp.length < 4} onClick={verify}>
                {busy ? "Verifying…" : "Link device"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function DevicesPanel({ setError }: { setError: (v: string | null) => void }) {
  const [devices, setDevices] = useState<Device[]>([]);
  const [peers, setPeers] = useState<string[]>([]);

  const load = useCallback(async () => {
    try {
      const [d, p] = await Promise.all([
        invoke<Device[]>("list_devices"),
        invoke<string[]>("get_connected_peers"),
      ]);
      setDevices(d);
      setPeers(p);
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  useEffect(() => {
    load();
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, [load]);

  return (
    <section>
      <header className="page-head">
        <h2>Devices</h2>
        <button className="ghost" onClick={load}>
          Refresh
        </button>
      </header>
      <div className="list">
        {devices.map((d) => {
          const connected = d.peerId ? peers.includes(d.peerId) : false;
          return (
            <div className="list-item" key={d.id}>
              <div>
                <div className="title">
                  {d.name} {d.isSelf && <span className="pill">this device</span>}
                </div>
                <div className="muted tiny">
                  {d.os || "unknown os"} · {d.peerId?.slice(0, 16) || "no peer yet"}
                </div>
              </div>
              <div className="status">
                <span className={connected || d.online ? "dot on" : "dot"} />
                {connected ? "P2P" : d.online ? "online" : "offline"}
              </div>
            </div>
          );
        })}
        {devices.length === 0 && <p className="muted">No devices yet.</p>}
      </div>
    </section>
  );
}

function SharePanel({ setError }: { setError: (v: string | null) => void }) {
  const [devices, setDevices] = useState<Device[]>([]);
  const [peerId, setPeerId] = useState("");
  const [transfers, setTransfers] = useState<Transfer[]>([]);

  useEffect(() => {
    (async () => {
      try {
        setDevices(await invoke<Device[]>("list_devices"));
        setTransfers(await invoke<Transfer[]>("get_transfer_status"));
      } catch (e) {
        setError(String(e));
      }
    })();
    const unlisten = listen("node-event", () => {
      invoke<Transfer[]>("get_transfer_status").then(setTransfers).catch(() => {});
    });
    return () => {
      unlisten.then((u) => u());
    };
  }, [setError]);

  const targets = useMemo(
    () => devices.filter((d) => !d.isSelf && d.peerId),
    [devices],
  );

  async function pickAndSend() {
    setError(null);
    try {
      const selected = await open({ multiple: true, directory: false });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      await invoke("share_files", { peerId, paths });
      setTransfers(await invoke<Transfer[]>("get_transfer_status"));
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section>
      <header className="page-head">
        <h2>Share files</h2>
      </header>
      <label>
        Destination device
        <select value={peerId} onChange={(e) => setPeerId(e.target.value)}>
          <option value="">Select a device</option>
          {targets.map((d) => (
            <option key={d.id} value={d.peerId!}>
              {d.name}
            </option>
          ))}
        </select>
      </label>
      <button disabled={!peerId} onClick={pickAndSend}>
        Choose files & send
      </button>
      <h3>Transfers</h3>
      <div className="list">
        {transfers.map((t) => (
          <div className="list-item" key={t.id}>
            <div>
              <div className="title">{t.path || t.id}</div>
              <div className="muted tiny">
                {t.kind} · {t.state} · {t.bytes_done}/{t.bytes_total}
              </div>
            </div>
          </div>
        ))}
        {transfers.length === 0 && <p className="muted">No transfers yet.</p>}
      </div>
    </section>
  );
}

function SyncPanel({ setError }: { setError: (v: string | null) => void }) {
  const [paths, setPaths] = useState<SyncPath[]>([]);

  const load = useCallback(async () => {
    try {
      setPaths(await invoke<SyncPath[]>("list_sync_paths"));
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  useEffect(() => {
    load();
  }, [load]);

  async function addPath() {
    setError(null);
    try {
      const selected = await open({ directory: true, multiple: false });
      if (!selected || Array.isArray(selected)) return;
      await invoke("add_sync_path", { path: selected });
      await load();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section>
      <header className="page-head">
        <h2>Folder sync</h2>
        <button onClick={addPath}>Add folder</button>
      </header>
      <p className="muted">
        Selected folders sync to your other devices over libp2p using
        last-write-wins. Conflicts create a local conflict copy.
      </p>
      <div className="list">
        {paths.map((p) => (
          <div className="list-item" key={p.root_id}>
            <div>
              <div className="title">{p.local_path}</div>
              <div className="muted tiny">{p.root_id}</div>
            </div>
            <span className="pill">{p.enabled ? "enabled" : "paused"}</span>
          </div>
        ))}
        {paths.length === 0 && <p className="muted">No sync folders yet.</p>}
      </div>
    </section>
  );
}

function ClipboardPanel({ setError }: { setError: (v: string | null) => void }) {
  const [enabled, setEnabled] = useState(false);
  const [lastRemote, setLastRemote] = useState<string | null>(null);

  useEffect(() => {
    invoke<boolean>("get_clipboard_sync")
      .then(setEnabled)
      .catch((e) => setError(String(e)));
    const unlisten = listen<{ type?: string; text?: string; origin_peer?: string }>(
      "node-event",
      (event) => {
        const payload = event.payload as {
          type?: string;
          text?: string;
          origin_peer?: string;
        };
        if (payload?.type === "clipboard_received" && payload.text) {
          setLastRemote(
            `From ${payload.origin_peer?.slice(0, 12) ?? "peer"}: ${payload.text.slice(0, 120)}`,
          );
        }
      },
    );
    return () => {
      unlisten.then((u) => u());
    };
  }, [setError]);

  async function toggle(next: boolean) {
    try {
      await invoke("set_clipboard_sync", { enabled: next });
      setEnabled(next);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section>
      <header className="page-head">
        <h2>Clipboard sync</h2>
      </header>
      <p className="muted">UTF-8 text only. Echoes are ignored via content hash.</p>
      <label className="switch">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => toggle(e.target.checked)}
        />
        Sync clipboard across devices
      </label>
      {lastRemote && <div className="banner">{lastRemote}</div>}
    </section>
  );
}

function SettingsPanel({
  session,
  setError,
  onLogout,
}: {
  session: Session;
  setError: (v: string | null) => void;
  onLogout: () => Promise<void>;
}) {
  const [devices, setDevices] = useState<Device[]>([]);

  useEffect(() => {
    invoke<Device[]>("list_devices")
      .then(setDevices)
      .catch((e) => setError(String(e)));
  }, [setError]);

  return (
    <section>
      <header className="page-head">
        <h2>Settings</h2>
      </header>
      <div className="card">
        <div className="kv">
          <span>Email</span>
          <strong>{session.email}</strong>
        </div>
        <div className="kv">
          <span>Device</span>
          <strong>{session.device_name}</strong>
        </div>
        <div className="kv">
          <span>Peer ID</span>
          <strong className="mono">{session.peer_id}</strong>
        </div>
      </div>
      <h3>Unlink a device</h3>
      <div className="list">
        {devices
          .filter((d) => !d.isSelf)
          .map((d) => (
            <div className="list-item" key={d.id}>
              <div className="title">{d.name}</div>
              <button
                className="ghost danger"
                onClick={async () => {
                  try {
                    await invoke("unlink_device", { deviceId: d.id });
                    setDevices(await invoke<Device[]>("list_devices"));
                  } catch (e) {
                    setError(String(e));
                  }
                }}
              >
                Unlink
              </button>
            </div>
          ))}
      </div>
      <button className="danger" onClick={() => onLogout().catch((e) => setError(String(e)))}>
        Sign out on this device
      </button>
    </section>
  );
}
