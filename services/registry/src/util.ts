import { createHash, randomBytes, randomInt, timingSafeEqual } from "node:crypto";
import { nanoid } from "nanoid";

export function newId(prefix?: string): string {
  return prefix ? `${prefix}_${nanoid(16)}` : nanoid(21);
}

export function hashValue(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

export function generateOtp(): string {
  return String(randomInt(0, 1_000_000)).padStart(6, "0");
}

export function generateAccessToken(): string {
  return randomBytes(32).toString("base64url");
}

export function safeEqualHex(a: string, b: string): boolean {
  const ba = Buffer.from(a, "hex");
  const bb = Buffer.from(b, "hex");
  if (ba.length !== bb.length) return false;
  return timingSafeEqual(ba, bb);
}

export function normalizeEmail(email: string): string {
  return email.trim().toLowerCase();
}
