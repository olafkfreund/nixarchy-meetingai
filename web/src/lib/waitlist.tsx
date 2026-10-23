import { render } from "@react-email/render";
import { createServerFn } from "@tanstack/react-start";

import { NotifyEmail } from "../emails/notify";
import { WaitlistEmail } from "../emails/waitlist";
import { MANAGED_PLAN as PLAN } from "./site";

const FROM = "Lennette <harsh@itssingularity.com>";

// Deliberately permissive: over-strict patterns reject valid addresses. The
// real check is whether the address can receive mail.
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

const configuration = () => {
  const apiKey = process.env["RESEND_API_KEY"];
  const notify = process.env["WAITLIST_NOTIFY_EMAIL"];
  const from = process.env["WAITLIST_FROM"] ?? FROM;

  if (!apiKey || !notify) {
    return null;
  }
  return { apiKey, notify, from };
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
    const config = configuration();
    if (!config) {
      // Never claim success when nothing was sent.
      return {
        ok: false,
        message:
          "The waitlist is not connected yet. Email harsh@itssingularity.com and I will add you by hand.",
      };
    }

    // Imported lazily so the SDK never reaches the client bundle.
    const { Resend } = await import("resend");
    const resend = new Resend(config.apiKey);

    const [toSignup, toOwner] = await Promise.all([
      render(
        <WaitlistEmail
          email={data.email}
          plan={PLAN.name}
          price={PLAN.price}
        />,
        { pretty: false }
      ),
      render(
        <NotifyEmail
          email={data.email}
          plan={PLAN.name}
          price={PLAN.price}
          receivedAt={new Date().toUTCString()}
          {...(data.userAgent ? { userAgent: data.userAgent } : {})}
        />,
        { pretty: false }
      ),
    ]);

    // The signup note goes first: if that fails, we do not tell the owner a
    // signup completed when the person never heard back.
    const signup = await resend.emails.send({
      from: config.from,
      to: data.email,
      subject: "You are on the Lennette list",
      html: toSignup,
      text: "Thanks for joining the Lennette waitlist. You are first in line for the Managed plan when it opens. Reply to this email and it reaches me directly.",
    });

    if (signup.error) {
      return {
        ok: false,
        message:
          "That did not go through. Email harsh@itssingularity.com and I will add you by hand.",
      };
    }

    await resend.emails.send({
      from: config.from,
      to: config.notify,
      replyTo: data.email,
      subject: `Waitlist: ${data.email}`,
      html: toOwner,
      text: `New waitlist signup: ${data.email} (${PLAN.name}, ${PLAN.price}/month)`,
    });

    return { ok: true, message: "You are on the list. Check your inbox." };
  });
