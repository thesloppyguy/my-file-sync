import cors from "@fastify/cors";
import rateLimit from "@fastify/rate-limit";
import Fastify from "fastify";
import { z } from "zod";
import {
  authenticate,
  heartbeat,
  listDevicesForUser,
  requestOtp,
  unlinkDevice,
  updateDeviceMe,
  verifyOtp,
} from "./auth.js";
import { migrate } from "./db/client.js";
import { getRecentOtps } from "./mail.js";

migrate();

const app = Fastify({ logger: true });

await app.register(cors, { origin: true });
await app.register(rateLimit, {
  max: 60,
  timeWindow: "1 minute",
});

app.get("/health", async () => ({
  ok: true,
  relayMultiaddr: process.env.RELAY_MULTIADDR ?? null,
}));

app.get("/config", async () => ({
  relayMultiaddr: process.env.RELAY_MULTIADDR ?? null,
}));

const emailSchema = z.object({
  email: z.string().email(),
});

app.post("/auth/request-otp", {
  config: {
    rateLimit: {
      max: 5,
      timeWindow: "1 minute",
    },
  },
  handler: async (req, reply) => {
    const body = emailSchema.parse(req.body);
    const result = await requestOtp(body.email);
    return reply.send(result);
  },
});

const verifySchema = z.object({
  email: z.string().email(),
  otp: z.string().min(4).max(8),
  device: z.object({
    name: z.string().min(1).max(120),
    fingerprintHash: z.string().min(8).max(128),
    peerId: z.string().optional(),
    multiaddrs: z.array(z.string()).optional(),
    os: z.string().optional(),
  }),
});

app.post("/auth/verify-otp", {
  config: {
    rateLimit: {
      max: 10,
      timeWindow: "1 minute",
    },
  },
  handler: async (req, reply) => {
    const body = verifySchema.parse(req.body);
    try {
      const result = await verifyOtp(body.email, body.otp, body.device);
      return reply.send(result);
    } catch (err) {
      const e = err as Error & { statusCode?: number };
      return reply.code(e.statusCode ?? 500).send({ error: e.message });
    }
  },
});

app.get("/devices", async (req, reply) => {
  try {
    const device = await authenticate(req);
    const list = await listDevicesForUser(device.userId, device.id);
    return reply.send({ devices: list });
  } catch (err) {
    const e = err as Error & { statusCode?: number };
    return reply.code(e.statusCode ?? 500).send({ error: e.message });
  }
});

const updateMeSchema = z.object({
  peerId: z.string().optional(),
  multiaddrs: z.array(z.string()).optional(),
  name: z.string().min(1).max(120).optional(),
});

app.put("/devices/me", async (req, reply) => {
  try {
    const device = await authenticate(req);
    const body = updateMeSchema.parse(req.body ?? {});
    await updateDeviceMe(device.id, body);
    return reply.send({ ok: true });
  } catch (err) {
    const e = err as Error & { statusCode?: number };
    return reply.code(e.statusCode ?? 500).send({ error: e.message });
  }
});

app.post("/devices/me/heartbeat", async (req, reply) => {
  try {
    const device = await authenticate(req);
    await heartbeat(device.id);
    return reply.send({ ok: true });
  } catch (err) {
    const e = err as Error & { statusCode?: number };
    return reply.code(e.statusCode ?? 500).send({ error: e.message });
  }
});

app.delete("/devices/:id", async (req, reply) => {
  try {
    const device = await authenticate(req);
    const { id } = req.params as { id: string };
    await unlinkDevice(device.userId, id);
    return reply.send({ ok: true });
  } catch (err) {
    const e = err as Error & { statusCode?: number };
    return reply.code(e.statusCode ?? 500).send({ error: e.message });
  }
});

// Dev-only helper when SMTP is not configured.
if (process.env.NODE_ENV !== "production") {
  app.get("/debug/otps", async () => ({ otps: getRecentOtps() }));
}

const port = Number(process.env.PORT ?? 8787);
const host = process.env.HOST ?? "0.0.0.0";

try {
  await app.listen({ port, host });
  console.log(`registry listening on http://${host}:${port}`);
} catch (err) {
  app.log.error(err);
  process.exit(1);
}
