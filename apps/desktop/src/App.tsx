import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import "./App.css";

const TIP_KEY = "myshare_seen_tip";

async function applyWindowChrome(mode: "auth" | "app") {
  const win = getCurrentWindow();
  if (mode === "app") {
    await win.setTitle("My Share");
    await win.setResizable(true);
    await win.setMinSize(new LogicalSize(900, 600));
    await win.setSize(new LogicalSize(1040, 700));
  } else {
    await win.setTitle("Get Started");
    await win.setResizable(false);
    await win.setMinSize(new LogicalSize(380, 420));
    await win.setSize(new LogicalSize(440, 520));
  }
  await win.center();
}

function BrandMark() {
  return <span className="brand-badge">M</span>;
}

function TitleBar({
  title,
  showMaximize = false,
}: {
  title: string;
  showMaximize?: boolean;
}) {
  const win = getCurrentWindow();
  return (
    <div className="titlebar" data-tauri-drag-region>
      <div className="titlebar-brand" data-tauri-drag-region>
        <BrandMark />
        <div className="titlebar-title">{title}</div>
      </div>
      <div className="titlebar-controls">
        <button
          type="button"
          className="titlebar-btn"
          aria-label="Minimize"
          onClick={() => win.minimize()}
        >
          <span aria-hidden="true">─</span>
        </button>
        {showMaximize && (
          <button
            type="button"
            className="titlebar-btn"
            aria-label="Maximize"
            onClick={() => win.toggleMaximize()}
          >
            <span aria-hidden="true">□</span>
          </button>
        )}
        <button
          type="button"
          className="titlebar-btn titlebar-close"
          aria-label="Close"
          onClick={() => win.close()}
        >
          <span aria-hidden="true">✕</span>
        </button>
      </div>
    </div>
  );
}

function KeyIcon() {
  return (
    <svg viewBox="0 0 64 64" fill="none" stroke="currentColor" strokeWidth="2.4">
      <circle cx="24" cy="24" r="12" />
      <path d="M34 30l22 22" strokeLinecap="round" />
      <path d="M48 44l6 0M52 48l6 0" strokeLinecap="round" />
      <circle cx="24" cy="24" r="3.5" fill="currentColor" stroke="none" />
    </svg>
  );
}

function MailIcon() {
  return (
    <svg viewBox="0 0 64 64" fill="none" stroke="currentColor" strokeWidth="2.4">
      <rect x="8" y="16" width="48" height="34" rx="6" />
      <path d="M10 20l22 18L54 20" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function TabletIcon() {
  return (
    <svg viewBox="0 0 64 64" fill="none" stroke="currentColor" strokeWidth="2.6">
      <rect x="16" y="8" width="32" height="48" rx="5" />
      <circle cx="32" cy="48" r="2" fill="currentColor" stroke="none" />
    </svg>
  );
}

function LaptopIcon() {
  return (
    <svg viewBox="0 0 64 64" fill="none" stroke="currentColor" strokeWidth="2.6">
      <rect x="12" y="14" width="40" height="28" rx="3" />
      <path d="M8 46h48" strokeLinecap="round" />
    </svg>
  );
}

function PhoneIcon() {
  return (
    <svg viewBox="0 0 64 64" fill="none" stroke="currentColor" strokeWidth="2.6">
      <rect x="20" y="8" width="24" height="48" rx="5" />
      <circle cx="32" cy="48" r="2" fill="currentColor" stroke="none" />
    </svg>
  );
}

function PasteIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
      <rect x="8" y="4" width="10" height="14" rx="2" />
      <rect x="5" y="7" width="10" height="14" rx="2" />
    </svg>
  );
}

function FileIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M7 3h7l5 5v13a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z" />
      <path d="M14 3v5h5" />
    </svg>
  );
}

function FolderIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z" />
    </svg>
  );
}

function GearIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
      <circle cx="12" cy="12" r="3" />
      <path d="M12 2v3M12 19v3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M2 12h3M19 12h3M4.9 19.1L7 17M17 7l2.1-2.1" />
    </svg>
  );
}

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

function deviceKind(os?: string | null, name = "") {
  const hay = `${os ?? ""} ${name}`.toLowerCase();
  if (hay.includes("ipad") || hay.includes("tablet")) return "tablet";
  if (hay.includes("iphone") || hay.includes("android") || hay.includes("phone")) {
    return "phone";
  }
  return "laptop";
}

function DeviceGlyph({
  os,
  name,
  large = false,
}: {
  os?: string | null;
  name?: string;
  large?: boolean;
}) {
  const kind = deviceKind(os, name);
  const icon =
    kind === "tablet" ? <TabletIcon /> : kind === "phone" ? <PhoneIcon /> : <LaptopIcon />;
  if (large) return icon;
  return icon;
}

function initials(name: string) {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((p) => p[0]?.toUpperCase() ?? "")
    .join("");
}

export default function App() {
  const [session, setSession] = useState<Session | null>(null);
  const [booting, setBooting] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showTip, setShowTip] = useState(false);

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

  useEffect(() => {
    if (booting) return;
    applyWindowChrome(session && !showTip ? "app" : "auth").catch(() => {});
  }, [booting, session, showTip]);

  if (booting) {
    return (
      <div className="get-started">
        <TitleBar title="Get Started" />
        <div className="get-started-body">
          <p className="get-started-copy">Starting…</p>
        </div>
      </div>
    );
  }

  if (!session) {
    return (
      <Onboarding
        error={error}
        setError={setError}
        onDone={async () => {
          const seen = localStorage.getItem(TIP_KEY) === "1";
          await refreshSession();
          if (!seen) setShowTip(true);
        }}
      />
    );
  }

  if (showTip) {
    return (
      <TipStep
        onDone={() => {
          localStorage.setItem(TIP_KEY, "1");
          setShowTip(false);
        }}
      />
    );
  }

  return (
    <MainApp
      session={session}
      error={error}
      setError={setError}
      onLogout={async () => {
        await invoke("logout");
        setSession(null);
      }}
    />
  );
}

function Onboarding({
  error,
  setError,
  onDone,
}: {
  error: string | null;
  setError: (v: string | null) => void;
  onDone: () => Promise<void>;
}) {
  const [step, setStep] = useState<"email" | "otp">("email");
  const [email, setEmail] = useState("");
  const [otp, setOtp] = useState("");
  const [busy, setBusy] = useState(false);
  const deviceName = `${navigator.platform || "Device"}`;

  useEffect(() => {
    const unlisten = listen("tray-sign-in", () => {
      setStep("email");
      setError(null);
    });
    return () => {
      unlisten.then((u) => u());
    };
  }, [setError]);

  async function requestOtp() {
    setBusy(true);
    setError(null);
    try {
      await invoke("request_otp", { email });
      setStep("otp");
      setOtp("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function verify(code = otp) {
    setBusy(true);
    setError(null);
    try {
      await invoke("verify_otp", { email, otp: code, deviceName });
      await onDone();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="get-started">
      <TitleBar title="Get Started" />
      <div className="get-started-body">
        <div className="get-started-inner">
          {step === "email" ? (
            <>
              <div className="hero-icon">
                <KeyIcon />
              </div>
              <h1 className="get-started-title">Sign in with your email address</h1>
              {error && <div className="get-started-error">{error}</div>}
              <form
                className="get-started-form"
                onSubmit={(e) => {
                  e.preventDefault();
                  if (email.includes("@")) void requestOtp();
                }}
              >
                <input
                  className="underline-field"
                  type="email"
                  autoFocus
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="you@example.com"
                />
                <p className="get-started-copy">You'll receive a code to sign in</p>
                <button
                  type="submit"
                  className="primary-btn"
                  disabled={busy || !email.includes("@")}
                >
                  {busy ? "Sending…" : "Send code"}
                </button>
              </form>
            </>
          ) : (
            <>
              <div className="hero-icon">
                <MailIcon />
              </div>
              <h1 className="get-started-title">Check your email and enter the code</h1>
              {error && <div className="get-started-error">{error}</div>}
              <form
                className="get-started-form"
                onSubmit={(e) => {
                  e.preventDefault();
                  if (otp.replace(/\s/g, "").length >= 4) void verify();
                }}
              >
                <input
                  className="underline-field code"
                  autoFocus
                  value={otp}
                  onChange={(e) => {
                    const next = e.target.value.replace(/[^\d]/g, "").slice(0, 8);
                    setOtp(next);
                    if (next.length === 6) void verify(next);
                  }}
                  placeholder="1 2 3 4 5 6"
                  inputMode="numeric"
                  autoComplete="one-time-code"
                />
                <p className="get-started-copy">Code sent to {email}</p>
                <button
                  type="button"
                  className="link-btn"
                  disabled={busy}
                  onClick={() => void requestOtp()}
                >
                  Request a new code
                </button>
              </form>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function TipStep({ onDone }: { onDone: () => void }) {
  return (
    <div className="get-started">
      <TitleBar title="Get Started" />
      <div className="get-started-body">
        <div className="get-started-inner">
          <div className="tip-art" aria-hidden="true">
            <div className="tip-thumb" />
            <div className="tip-menu">
              <span>Open with</span>
              <span>Share</span>
              <span>Properties</span>
              <span className="active">
                <i>M</i> My Share
              </span>
              <span>Show more options</span>
            </div>
          </div>
          <h1 className="get-started-title">Right-click on files and folders to send</h1>
          <p className="get-started-copy">
            Send to other people (and other devices!) by right-clicking in File Explorer
          </p>
          <button type="button" className="primary-btn" onClick={onDone}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

function MainApp({
  session,
  error,
  setError,
  onLogout,
}: {
  session: Session;
  error: string | null;
  setError: (v: string | null) => void;
  onLogout: () => Promise<void>;
}) {
  const [devices, setDevices] = useState<Device[]>([]);
  const [peers, setPeers] = useState<string[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [view, setView] = useState<"home" | "settings">("home");
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const [d, p] = await Promise.all([
        invoke<Device[]>("list_devices"),
        invoke<string[]>("get_connected_peers"),
      ]);
      setDevices(d);
      setPeers(p);
      setSelectedId((prev) => {
        if (prev && d.some((x) => x.id === prev)) return prev;
        return d.find((x) => !x.isSelf)?.id ?? d[0]?.id ?? null;
      });
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  useEffect(() => {
    load();
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, [load]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return devices;
    return devices.filter(
      (d) =>
        d.name.toLowerCase().includes(q) ||
        (d.os ?? "").toLowerCase().includes(q) ||
        (d.peerId ?? "").toLowerCase().includes(q),
    );
  }, [devices, query]);

  const selected = devices.find((d) => d.id === selectedId) ?? null;
  const canSend = !!(selected && !selected.isSelf && selected.peerId);

  async function sendPaths(paths: string[]) {
    if (!selected?.peerId || paths.length === 0) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      await invoke("share_files", { peerId: selected.peerId, paths });
      setStatus(`Sending to ${selected.name}…`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function sendFiles() {
    const selectedPaths = await open({ multiple: true, directory: false });
    if (!selectedPaths) return;
    const paths = Array.isArray(selectedPaths) ? selectedPaths : [selectedPaths];
    await sendPaths(paths);
  }

  async function sendFolder() {
    const selectedPath = await open({ multiple: false, directory: true });
    if (!selectedPath || Array.isArray(selectedPath)) return;
    await sendPaths([selectedPath]);
  }

  async function pasteClipboard() {
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      await invoke("set_clipboard_sync", { enabled: true });
      const text = await readText();
      if (!text?.trim()) {
        setStatus("Clipboard is empty — copy something first");
        return;
      }
      setStatus(`Clipboard sync on — latest text ready for ${selected?.name ?? "devices"}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="app-frame">
      <TitleBar title="My Share" showMaximize />
      <div className="shell">
        <aside className="sidebar">
          <div className="sidebar-top">
            <div className="sidebar-brand">
              <BrandMark />
              My Share
            </div>
            <button
              type="button"
              className={`icon-btn${view === "settings" ? " active" : ""}`}
              aria-label="Settings"
              onClick={() => setView((v) => (v === "settings" ? "home" : "settings"))}
            >
              <GearIcon />
            </button>
          </div>
          <div className="search-wrap">
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Name or email"
            />
          </div>
          <div className="device-list">
            {filtered.map((d) => {
              const connected = d.peerId ? peers.includes(d.peerId) : false;
              return (
                <button
                  key={d.id}
                  type="button"
                  className={`device-row${selectedId === d.id ? " selected" : ""}`}
                  onClick={() => {
                    setSelectedId(d.id);
                    setView("home");
                  }}
                >
                  <div className={`device-avatar${d.isSelf ? " accent" : ""}`}>
                    {d.isSelf ? <DeviceGlyph os={d.os} name={d.name} /> : initials(d.name) || "D"}
                  </div>
                  <div className="device-meta">
                    <div className="device-name">
                      {d.name}
                      {d.isSelf ? " (you)" : ""}
                    </div>
                    <div className="device-sub">
                      {d.isSelf
                        ? session.email
                        : d.os || (connected ? "Connected" : d.online ? "Online" : "Offline")}
                    </div>
                  </div>
                  <span className={`device-status${connected || d.online ? " on" : ""}`} />
                </button>
              );
            })}
            {filtered.length === 0 && (
              <p className="muted tiny" style={{ padding: "8px 10px" }}>
                No devices match.
              </p>
            )}
          </div>
        </aside>

        <main className="main-pane">
          {(error || status) && (
            <div className={`banner${error ? " error" : ""}`}>{error || status}</div>
          )}

          {view === "settings" ? (
            <div className="settings-pane">
              <h2>Settings</h2>
              <div className="settings-card">
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
              <button
                type="button"
                className="danger-btn"
                onClick={() => onLogout().catch((e) => setError(String(e)))}
              >
                Sign out on this device
              </button>
            </div>
          ) : !selected ? (
            <div className="main-empty">
              <p>Sign in on another device to start sharing.</p>
            </div>
          ) : (
            <div className="device-hero">
              <div className="hero-orb">
                <DeviceGlyph os={selected.os} name={selected.name} large />
              </div>
              <h1>{selected.name}</h1>
              <p>
                {selected.isSelf
                  ? "This device"
                  : selected.os
                    ? `Your ${selected.os}`
                    : "Your device"}
              </p>
              <div className="action-row">
                <button
                  type="button"
                  className="action-card"
                  disabled={busy || selected.isSelf}
                  onClick={() => void pasteClipboard()}
                >
                  <PasteIcon />
                  Paste
                </button>
                <button
                  type="button"
                  className="action-card"
                  disabled={busy || !canSend}
                  onClick={() => void sendFiles()}
                >
                  <FileIcon />
                  Send Files
                </button>
                <button
                  type="button"
                  className="action-card"
                  disabled={busy || !canSend}
                  onClick={() => void sendFolder()}
                >
                  <FolderIcon />
                  Send a Folder
                </button>
              </div>
              <p className="hint">
                You can also Ctrl+V to paste or drag & drop files to send them.
              </p>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}
