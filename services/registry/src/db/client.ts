import Database from "better-sqlite3";
import { drizzle } from "drizzle-orm/better-sqlite3";
import fs from "node:fs";
import path from "node:path";
import * as schema from "./schema.js";

const dbPath =
  process.env.SQLITE_PATH ??
  path.resolve(process.cwd(), "data", "registry.sqlite");

fs.mkdirSync(path.dirname(dbPath), { recursive: true });

const sqlite = new Database(dbPath);
sqlite.pragma("journal_mode = WAL");
sqlite.pragma("foreign_keys = ON");

export const db = drizzle(sqlite, { schema });

export function migrate() {
  sqlite.exec(`
    CREATE TABLE IF NOT EXISTS users (
      id TEXT PRIMARY KEY,
      email TEXT NOT NULL UNIQUE,
      created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS devices (
      id TEXT PRIMARY KEY,
      user_id TEXT NOT NULL REFERENCES users(id),
      name TEXT NOT NULL,
      fingerprint_hash TEXT NOT NULL,
      peer_id TEXT,
      multiaddrs TEXT NOT NULL DEFAULT '[]',
      os TEXT,
      access_token_hash TEXT NOT NULL,
      last_seen_at INTEGER,
      created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS otp_codes (
      id TEXT PRIMARY KEY,
      email TEXT NOT NULL,
      code_hash TEXT NOT NULL,
      expires_at INTEGER NOT NULL,
      consumed_at INTEGER,
      created_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_devices_user ON devices(user_id);
    CREATE INDEX IF NOT EXISTS idx_devices_token ON devices(access_token_hash);
    CREATE INDEX IF NOT EXISTS idx_otp_email ON otp_codes(email);
  `);
}
