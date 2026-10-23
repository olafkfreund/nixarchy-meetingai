import { PAGE_PATHS, pageUrl } from "./seo";
import { SITE_URL } from "./site";

export const sitemapXml = (): string => `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${PAGE_PATHS.map((path) => `  <url><loc>${pageUrl(path)}</loc></url>`).join("\n")}
</urlset>\n`;

export const robotsTxt = (noindex = false): string =>
  noindex
    ? "User-agent: *\nDisallow: /\n"
    : `User-agent: *\nAllow: /\nDisallow: /_serverFn/\n\nSitemap: ${SITE_URL}/sitemap.xml\n`;

export const crawlerResponse = (body: string, type: string, head = false) =>
  new Response(head ? null : body, {
    headers: {
      "Content-Type": `${type}; charset=utf-8`,
      "Cache-Control": "public, max-age=3600",
      "X-Content-Type-Options": "nosniff",
    },
  });
