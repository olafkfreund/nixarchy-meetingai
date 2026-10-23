import { createFileRoute } from "@tanstack/react-router";

import { crawlerResponse, robotsTxt } from "@/lib/crawlers";

const body = () => robotsTxt(process.env["SEO_NOINDEX"] === "true");

export const Route = createFileRoute("/robots.txt")({
  server: {
    handlers: {
      GET: () => crawlerResponse(body(), "text/plain"),
      HEAD: () => crawlerResponse(body(), "text/plain", true),
    },
  },
});
