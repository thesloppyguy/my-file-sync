import { integer, sqliteTable, text } from "drizzle-orm/sqlite-core";

export const users = sqliteTable("users", {
  id: text("id").primaryKey(),
  email: text("email").notNull().unique(),
  createdAt: integer("created_at", { mode: "timestamp_ms" }).notNull(),
});

export const devices = sqliteTable("devices", {
  id: text("id").primaryKey(),
  userId: text("user_id")
    .notNull()
    .references(() => users.id),
  name: text("name").notNull(),
  fingerprintHash: text("fingerprint_hash").notNull(),
  peerId: text("peer_id"),
  multiaddrs: text("multiaddrs").notNull().default("[]"),
  os: text("os"),
  accessTokenHash: text("access_token_hash").notNull(),
  lastSeenAt: integer("last_seen_at", { mode: "timestamp_ms" }),
  createdAt: integer("created_at", { mode: "timestamp_ms" }).notNull(),
});

export const otpCodes = sqliteTable("otp_codes", {
  id: text("id").primaryKey(),
  email: text("email").notNull(),
  codeHash: text("code_hash").notNull(),
  expiresAt: integer("expires_at", { mode: "timestamp_ms" }).notNull(),
  consumedAt: integer("consumed_at", { mode: "timestamp_ms" }),
  createdAt: integer("created_at", { mode: "timestamp_ms" }).notNull(),
});
