import { and, desc, eq, gt, isNull } from "drizzle-orm";
import type { FastifyRequest } from "fastify";
import { db } from "./db/client.js";
import { devices, otpCodes, users } from "./db/schema.js";
import {
  generateAccessToken,
  generateOtp,
  hashValue,
  newId,
  normalizeEmail,
  safeEqualHex,
} from "./util.js";
import { sendOtpEmail } from "./mail.js";

const OTP_TTL_MS = 10 * 60 * 1000;
const ONLINE_THRESHOLD_MS = 90 * 1000;

export async function requestOtp(emailRaw: string) {
  const email = normalizeEmail(emailRaw);
  const otp = generateOtp();
  const now = new Date();

  await db.insert(otpCodes).values({
    id: newId("otp"),
    email,
    codeHash: hashValue(otp),
    expiresAt: new Date(now.getTime() + OTP_TTL_MS),
    createdAt: now,
  });

  await sendOtpEmail(email, otp);
  return { ok: true as const, expiresInSec: OTP_TTL_MS / 1000 };
}

export type DeviceInput = {
  name: string;
  fingerprintHash: string;
  peerId?: string;
  multiaddrs?: string[];
  os?: string;
};

export async function verifyOtp(emailRaw: string, otp: string, device: DeviceInput) {
  const email = normalizeEmail(emailRaw);
  const now = new Date();

  const candidates = await db
    .select()
    .from(otpCodes)
    .where(
      and(
        eq(otpCodes.email, email),
        isNull(otpCodes.consumedAt),
        gt(otpCodes.expiresAt, now),
      ),
    )
    .orderBy(desc(otpCodes.createdAt))
    .limit(5);

  const match = candidates.find((row) => safeEqualHex(row.codeHash, hashValue(otp)));
  if (!match) {
    throw Object.assign(new Error("Invalid or expired OTP"), { statusCode: 401 });
  }

  await db
    .update(otpCodes)
    .set({ consumedAt: now })
    .where(eq(otpCodes.id, match.id));

  let user = (
    await db.select().from(users).where(eq(users.email, email)).limit(1)
  )[0];

  if (!user) {
    user = {
      id: newId("usr"),
      email,
      createdAt: now,
    };
    await db.insert(users).values(user);
  }

  const accessToken = generateAccessToken();
  const existing = (
    await db
      .select()
      .from(devices)
      .where(
        and(
          eq(devices.userId, user.id),
          eq(devices.fingerprintHash, device.fingerprintHash),
        ),
      )
      .limit(1)
  )[0];

  let deviceId: string;
  if (existing) {
    deviceId = existing.id;
    await db
      .update(devices)
      .set({
        name: device.name,
        peerId: device.peerId ?? existing.peerId,
        multiaddrs: JSON.stringify(device.multiaddrs ?? []),
        os: device.os ?? existing.os,
        accessTokenHash: hashValue(accessToken),
        lastSeenAt: now,
      })
      .where(eq(devices.id, existing.id));
  } else {
    deviceId = newId("dev");
    await db.insert(devices).values({
      id: deviceId,
      userId: user.id,
      name: device.name,
      fingerprintHash: device.fingerprintHash,
      peerId: device.peerId ?? null,
      multiaddrs: JSON.stringify(device.multiaddrs ?? []),
      os: device.os ?? null,
      accessTokenHash: hashValue(accessToken),
      lastSeenAt: now,
      createdAt: now,
    });
  }

  return {
    accessToken,
    userId: user.id,
    deviceId,
    email: user.email,
  };
}

export async function authenticate(req: FastifyRequest) {
  const header = req.headers.authorization;
  if (!header?.startsWith("Bearer ")) {
    throw Object.assign(new Error("Missing bearer token"), { statusCode: 401 });
  }
  const token = header.slice("Bearer ".length).trim();
  const tokenHash = hashValue(token);
  const device = (
    await db
      .select()
      .from(devices)
      .where(eq(devices.accessTokenHash, tokenHash))
      .limit(1)
  )[0];
  if (!device) {
    throw Object.assign(new Error("Invalid token"), { statusCode: 401 });
  }
  return device;
}

export async function listDevicesForUser(userId: string, selfDeviceId: string) {
  const rows = await db.select().from(devices).where(eq(devices.userId, userId));
  const now = Date.now();
  return rows.map((d) => ({
    id: d.id,
    name: d.name,
    fingerprintHash: d.fingerprintHash,
    peerId: d.peerId,
    multiaddrs: JSON.parse(d.multiaddrs || "[]") as string[],
    os: d.os,
    lastSeenAt: d.lastSeenAt ? new Date(d.lastSeenAt).toISOString() : null,
    online: d.lastSeenAt
      ? now - new Date(d.lastSeenAt).getTime() < ONLINE_THRESHOLD_MS
      : false,
    isSelf: d.id === selfDeviceId,
  }));
}

export async function updateDeviceMe(
  deviceId: string,
  patch: {
    peerId?: string;
    multiaddrs?: string[];
    name?: string;
  },
) {
  const now = new Date();
  await db
    .update(devices)
    .set({
      ...(patch.peerId !== undefined ? { peerId: patch.peerId } : {}),
      ...(patch.multiaddrs !== undefined
        ? { multiaddrs: JSON.stringify(patch.multiaddrs) }
        : {}),
      ...(patch.name !== undefined ? { name: patch.name } : {}),
      lastSeenAt: now,
    })
    .where(eq(devices.id, deviceId));
}

export async function heartbeat(deviceId: string) {
  await db
    .update(devices)
    .set({ lastSeenAt: new Date() })
    .where(eq(devices.id, deviceId));
}

export async function unlinkDevice(userId: string, deviceId: string) {
  const row = (
    await db.select().from(devices).where(eq(devices.id, deviceId)).limit(1)
  )[0];
  if (!row || row.userId !== userId) {
    throw Object.assign(new Error("Device not found"), { statusCode: 404 });
  }
  await db.delete(devices).where(eq(devices.id, deviceId));
}
