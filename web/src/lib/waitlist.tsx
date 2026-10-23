import { render } from "@react-email/render";
import { createServerFn } from "@tanstack/react-start";

import bellPng from "../emails/assets/bell.png?inline";
import bubblePng from "../emails/assets/bubble.png?inline";
import chartPng from "../emails/assets/chart.png?inline";
import clockPng from "../emails/assets/clock.png?inline";
import paperclipPng from "../emails/assets/paperclip.png?inline";
import refreshPng from "../emails/assets/refresh.png?inline";
import unlockPng from "../emails/assets/unlock.png?inline";
import usersPng from "../emails/assets/users.png?inline";
import { NotifyEmail } from "../emails/notify";
import { WaitlistEmail } from "../emails/waitlist";
import { MANAGED_PLAN as PLAN } from "./site";

const FROM = "Lennette <harsh@itssingularity.com>";

const SUBJECT = "Your seat is saved";

/**
 * Inline glyphs, shipped as CID attachments. Email clients drop SVG and most
 * refuse data URIs, so a real attachment is the only dependable route.
 *
 * The bytes are imported as base64 rather than read off disk: this deploys to
 * a Worker, where a filesystem path does not exist. Vite inlines these because
 * each PNG sits under the asset limit, so they ride along in the server bundle.
 *
 * `contentId` is the bare name so it matches `<img src="cid:name">`; the SDK
 * strips any surrounding angle brackets.
 */
const ICON_BYTES: Record<string, string> = {
  bell: bellPng,
  bubble: bubblePng,
  chart: chartPng,
  clock: clockPng,
  paperclip: paperclipPng,
  refresh: refreshPng,
  unlock: unlockPng,
  users: usersPng,
};

const iconAttachments = () =>
  Object.entries(ICON_BYTES).map(([name, content]) => ({
    filename: `${name}.png`,
    content,
    contentType: "image/png",
    contentId: name,
  }));

const textBody = (email: string, plan: string, price: string) =>
  [
    "Hey,",
    "",
    `Your seat is held for Lennette ${plan}.`,
    "",
    "Every signup so far is someone I get to build this for directly, so thank",
    "you. Here is what is on the table when it opens.",
    "",
    "  2,000 AI responses a month",
    "  30 hours of live transcription",
    "  Every device you use, same allowance",
    "  Follow-ups included, no second quota",
    "  Attach a CV, a brief, a PDF or a screenshot",
    "  Nothing to configure, no API keys",
    "",
    `${email} is first in line, before anything goes public. One email when it`,
    `opens, at ${price}/month. That is the whole list.`,
    "",
    "Want to try it now? The free version runs on your own keys, so it costs",
    "nothing but the API usage: https://github.com/parazeeknova/lennette",
    "",
    "Hit reply if you have a question. It reaches me, not a support desk.",
    "",
    "--",
    "Harsh",
    "building Lennette",
  ].join("\n");

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
      subject: SUBJECT,
      html: toSignup,
      text: textBody(data.email, PLAN.name, PLAN.price),
      attachments: iconAttachments(),
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
      subject: `New waitlist signup: ${data.email}`,
      html: toOwner,
      text: `New waitlist signup: ${data.email} (${PLAN.name}, ${PLAN.price}/month)`,
      attachments: iconAttachments(),
    });

    return { ok: true, message: "You are on the list. Check your inbox." };
  });
