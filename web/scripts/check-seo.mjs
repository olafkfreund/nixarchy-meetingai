import assert from "node:assert/strict";

import { JSDOM } from "jsdom";

import { HOME_FAQS, PRICING_FAQS } from "../src/lib/faqs.ts";
import { PAGE_PATHS, SEO_PAGES, pageUrl } from "../src/lib/seo.ts";
import { SITE_URL, SOCIAL_IMAGE } from "../src/lib/site.ts";

const base = new URL(process.argv[2] ?? "http://localhost:3000");
const get = (path, options) => fetch(new URL(path, base), options);
const normalize = (text) => text.replaceAll(/\s+/gu, " ").trim();

await Promise.all(
  PAGE_PATHS.map(async (path) => {
    const response = await get(path);
    assert.equal(response.status, 200, `${path}: HTTP status`);
    assert.match(response.headers.get("content-type") ?? "", /text\/html/u);
    assert.doesNotMatch(
      response.headers.get("x-robots-tag") ?? "",
      /noindex/u,
      `${path}: indexing blocked`
    );
    const html = await response.text();
    // Parse the raw server response without running JavaScript, just as a crawler can.
    const { document } = new JSDOM(html).window;
    const one = (selector) => {
      const elements = document.querySelectorAll(selector);
      assert.equal(elements.length, 1, `${path}: exactly one ${selector}`);
      return elements[0];
    };
    const meta = (attribute, key) =>
      one(`meta[${attribute}="${key}"]`).getAttribute("content");
    const page = SEO_PAGES[path];
    assert.equal(document.documentElement.lang, "en");
    assert.equal(one("title").textContent, page.title);
    assert.equal(meta("name", "description"), page.description);
    assert.match(meta("name", "robots"), /^index, follow/u);
    assert.equal(
      one('link[rel="canonical"]').getAttribute("href"),
      pageUrl(path)
    );
    assert.equal(meta("property", "og:url"), pageUrl(path));
    for (const key of ["og:title", "twitter:title"]) {
      assert.equal(
        meta(key.startsWith("og:") ? "property" : "name", key),
        page.title
      );
    }
    assert.equal(meta("property", "og:description"), page.description);
    assert.equal(meta("name", "twitter:description"), page.description);
    assert.equal(meta("property", "og:image"), SOCIAL_IMAGE.url);
    assert.equal(meta("name", "twitter:image"), SOCIAL_IMAGE.url);
    assert.equal(meta("name", "twitter:card"), "summary_large_image");
    assert.equal(one("h1").textContent.trim().length > 0, true);
    const graph = JSON.parse(
      one('script[type="application/ld+json"]').textContent
    )["@graph"];
    const application = graph.find(
      (node) => node["@type"] === "SoftwareApplication"
    );
    assert.equal(application.offers.price, "0");
    assert.equal(
      graph.find((node) => node["@id"] === `${pageUrl(path)}#webpage`).url,
      pageUrl(path)
    );
    if (path !== "/") {
      one('nav[aria-label="Breadcrumb"]');
      assert.equal(
        graph
          .find((node) => node["@type"] === "BreadcrumbList")
          .itemListElement.at(-1).item,
        pageUrl(path)
      );
    }
    const faqs = { "/": HOME_FAQS, "/pricing": PRICING_FAQS }[path] ?? [];
    if (faqs.length) {
      const faqSchema = graph.find((node) => node["@type"] === "FAQPage");
      assert.equal(faqSchema.mainEntity.length, faqs.length);
      const text = normalize(document.body.textContent);
      for (const faq of faqs) {
        assert.ok(
          text.includes(normalize(faq.question)),
          `${path}: FAQ question in HTML`
        );
        assert.ok(
          text.includes(normalize(faq.answer)),
          `${path}: FAQ answer in HTML`
        );
      }
    }
    if (path === "/tutorial") {
      assert.equal(document.querySelectorAll('[role="tabpanel"]').length, 2);
      assert.equal(
        document.querySelectorAll('[role="tabpanel"][hidden]').length,
        1
      );
    }
    console.log(
      `PASS ${path}: SSR metadata, canonical, social cards, structured data and content`
    );
  })
);

const [sitemap, robots, missing, redirect] = await Promise.all([
  get("/sitemap.xml"),
  get("/robots.txt"),
  get("/seo-check-missing-page"),
  get("/pricing/?utm_source=seo-check", { redirect: "manual" }),
]);
assert.equal(sitemap.status, 200);
assert.match(sitemap.headers.get("content-type"), /application\/xml/u);
const xml = new JSDOM(await sitemap.text(), { contentType: "application/xml" })
  .window.document;
assert.deepEqual(
  [...xml.querySelectorAll("loc")].map((node) => node.textContent),
  PAGE_PATHS.map(pageUrl)
);
assert.equal(robots.status, 200);
assert.match(robots.headers.get("content-type"), /text\/plain/u);
const robotsBody = await robots.text();
assert.ok(robotsBody.includes(`Sitemap: ${SITE_URL}/sitemap.xml`));
assert.equal(missing.status, 404);
assert.match(missing.headers.get("x-robots-tag"), /noindex/u);
const missingDocument = new JSDOM(await missing.text()).window.document;
assert.match(
  missingDocument.querySelector('meta[name="robots"]').getAttribute("content"),
  /noindex/u
);
assert.equal(missingDocument.querySelector('link[rel="canonical"]'), null);
assert.equal(redirect.status, 308);
assert.equal(
  new URL(redirect.headers.get("location"), base).pathname,
  "/pricing"
);
assert.equal(
  new URL(redirect.headers.get("location"), base).search,
  "?utm_source=seo-check"
);
await Promise.all(
  ["/robots.txt", "/sitemap.xml"].map(async (path) => {
    const response = await get(path, { method: "HEAD" });
    assert.equal(response.status, 200);
    assert.equal(await response.text(), "");
  })
);
console.log(
  "PASS sitemap, robots, HEAD requests, 404 noindex and permanent canonical redirects"
);
