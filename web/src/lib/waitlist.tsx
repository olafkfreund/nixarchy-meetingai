import { render } from "@react-email/render";
import { createServerFn } from "@tanstack/react-start";
import { createTransport } from "nodemailer";

import { NotifyEmail } from "../emails/notify";
import { WaitlistEmail } from "../emails/waitlist";

const PLAN = { name: "Managed", price: "$35" } as const;

// Deliberately permissive: the real check is whether the address can receive
// mail. Over-strict regexes reject valid addresses.
const looksLikeEmail = (value: string) =>
  /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/u.test(value) && value.length <= 254;

interface SubmitInput {
  email: string;
  userAgent?: string | undefined;
}

interface SubmitResult {
  ok: boolean;
  message: string;
}

const smtpConfig = () => {
  const host = process.env["SMTP_HOST"];
  const user = process.env["SMTP_USER"];
  const pass = process.env["SMTP_PASSWORD"];
  const from = process.env["SMTP_FROM"] ?? user;
  const notify = process.env["WAITLIST_NOTIFY_EMAIL"];

  if (!host || !user || !pass || !from || !notify) {
    return null;
  }

  return {
    host,
    port: Number(process.env["SMTP_PORT"] ?? 465),
    // 465 is implicit TLS, 587 upgrades via STARTTLS.
    secure: Number(process.env["SMTP_PORT"] ?? 465) === 465,
    auth: { user, pass },
    from,
    notify,
  };
};

export const joinWaitlist = createServerFn({ method: "POST" })
  .validator((data: SubmitInput): SubmitInput => {
    const email = typeof data?.email === "string" ? data.email.trim() : "";
    if (!looksLikeEmail(email)) {
      throw new Error("That email address does not look right.");
    }
    return {
      email: email.toLowerCase(),
      userAgent:
        typeof data.userAgent === "string"
          ? data.userAgent.slice(0, 200)
          : undefined,
    };
  })
  .handler(async ({ data }): Promise<SubmitResult> => {
    const config = smtpConfig();
    if (!config) {
      // Never claim success when nothing was sent.
      return {
        ok: false,
        message:
          "The waitlist is not connected yet. Email hello@lennette.dev and I will add you by hand.",
      };
    }

    const transport = createTransport({
      host: config.host,
      port: config.port,
      secure: config.secure,
      auth: config.auth,
    });

    const [toCustomer, toOwner] = await Promise.all([
      render(
        <WaitlistEmail email={data.email} plan={PLAN.name} price={PLAN.price} />
      ),
      render(
        <NotifyEmail
          email={data.email}
          plan={PLAN.name}
          price={PLAN.price}
          receivedAt={new Date().toUTCString()}
          {...(data.userAgent ? { userAgent: data.userAgent } : {})}
        />
      ),
    ]);

    // Send the customer note first: if it bounces, the signup is not recorded
    // as complete and we do not spam ourselves about a failed address.
    await transport.sendMail({
      from: config.from,
      to: data.email,
      subject: "You're on the Lennette list",
      html: toCustomer,
      text: "Thanks for joining the Lennette waitlist. You are first in line for the Managed plan when it opens.",
    });

    await transport.sendMail({
      from: config.from,
      to: config.notify,
      replyTo: data.email,
      subject: `Waitlist: ${data.email}`,
      html: toOwner,
      text: `New waitlist signup: ${data.email} (${PLAN.name}, ${PLAN.price}/month)`,
    });

    return { ok: true, message: "You're on the list. Check your inbox." };
  });
