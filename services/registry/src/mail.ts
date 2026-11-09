import nodemailer from "nodemailer";

const otpLog: Array<{ email: string; otp: string; at: string }> = [];

export function getRecentOtps() {
  return otpLog.slice(-20);
}

export async function sendOtpEmail(email: string, otp: string): Promise<void> {
  const host = process.env.SMTP_HOST;
  const from = process.env.SMTP_FROM ?? "noreply@mysync.local";

  // Always keep a local log for development / tests.
  otpLog.push({ email, otp, at: new Date().toISOString() });
  if (otpLog.length > 100) otpLog.shift();

  if (!host) {
    console.log(`[otp] ${email} => ${otp}`);
    return;
  }

  const transporter = nodemailer.createTransport({
    host,
    port: Number(process.env.SMTP_PORT ?? 587),
    secure: process.env.SMTP_SECURE === "true",
    auth: process.env.SMTP_USER
      ? {
          user: process.env.SMTP_USER,
          pass: process.env.SMTP_PASS,
        }
      : undefined,
  });

  await transporter.sendMail({
    from,
    to: email,
    subject: "Your my-file-sync code",
    text: `Your one-time code is ${otp}. It expires in 10 minutes.`,
    html: `<p>Your one-time code is <strong>${otp}</strong>.</p><p>It expires in 10 minutes.</p>`,
  });
}
