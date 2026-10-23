import { createFileRoute } from "@tanstack/react-router";

import installer from "../../public/install?raw";

// Nitro routes extensionless requests through SSR before Vite's public assets.
// Serve the public file here too so the setup command works in dev and prod.
const response = (head = false) =>
  new Response(head ? null : installer, {
    headers: {
      "Content-Type": "text/plain; charset=utf-8",
      "Cache-Control": "no-cache",
    },
  });

export const Route = createFileRoute("/install")({
  server: {
    handlers: {
      GET: () => response(),
      HEAD: () => response(true),
    },
  },
});
