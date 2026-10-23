import { createFileRoute } from "@tanstack/react-router";

import { crawlerResponse, sitemapXml } from "@/lib/crawlers";

export const Route = createFileRoute("/sitemap.xml")({
  server: {
    handlers: {
      GET: () => crawlerResponse(sitemapXml(), "application/xml"),
      HEAD: () => crawlerResponse(sitemapXml(), "application/xml", true),
    },
  },
});
