// Dev-only: renders both waitlist emails to HTML with the CID icons inlined as
// data URIs, so the result can be opened in any browser and inspected.
// Not shipped; the real send uses CID attachments.

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

import { render } from "@react-email/render";

import { NotifyEmail } from "../src/emails/notify";
import { WaitlistEmail } from "../src/emails/waitlist";

const assets = path.join(import.meta.dirname, "../src/emails/assets");

const inline = (html: string) =>
  html.replaceAll(
    /src="cid:(?<name>\w+)"/gu,
    (_match, name: string | undefined) => {
      const bytes = readFileSync(path.join(assets, `${name ?? ""}.png`));
      return `src="data:image/png;base64,${bytes.toString("base64")}"`;
    }
  );

const out = process.argv[2] ?? "/tmp/opencode";

writeFileSync(
  path.join(out, "email-signup.html"),
  inline(
    await render(<WaitlistEmail email="you@example.com" />, { pretty: true })
  )
);

writeFileSync(
  path.join(out, "email-owner.html"),
  inline(
    await render(
      <NotifyEmail
        email="you@example.com"
        receivedAt="Fri, 23 Oct 2026 14:02:11 GMT"
        userAgent="Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"
      />,
      { pretty: true }
    )
  )
);

console.log("rendered");
