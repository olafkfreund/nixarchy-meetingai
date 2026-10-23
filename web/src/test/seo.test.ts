import { describe, expect, it } from "vitest";

import { crawlerResponse, robotsTxt, sitemapXml } from "@/lib/crawlers";
import { HOME_FAQS, PRICING_FAQS } from "@/lib/faqs";
import { canonicalPathRedirect, searchResponse } from "@/lib/search-response";
import {
  PAGE_PATHS,
  SEO_PAGES,
  pageHead,
  pageUrl,
  serializeJsonLd,
} from "@/lib/seo";
import { SITE_URL, SOCIAL_IMAGE } from "@/lib/site";

describe("Public page SEO", () => {
  it("uses unique page metadata and the production origin for every share and canonical URL", () => {
    expect(new Set(PAGE_PATHS.map((path) => SEO_PAGES[path].title)).size).toBe(
      PAGE_PATHS.length
    );
    expect(
      new Set(PAGE_PATHS.map((path) => SEO_PAGES[path].description)).size
    ).toBe(PAGE_PATHS.length);
    for (const path of PAGE_PATHS) {
      const head = pageHead(path);
      const meta = Object.fromEntries(
        head.meta.map((tag) => [
          tag.name ?? tag.property ?? "title",
          tag.content ?? tag.title,
        ])
      );
      expect(head.links).toEqual([{ rel: "canonical", href: pageUrl(path) }]);
      expect(meta["og:url"]).toBe(pageUrl(path));
      expect(meta["og:title"]).toBe(meta["title"]);
      expect(meta["twitter:title"]).toBe(meta["title"]);
      expect(meta["og:description"]).toBe(meta["description"]);
      expect(meta["twitter:description"]).toBe(meta["description"]);
      expect(meta["og:image"]).toBe(SOCIAL_IMAGE.url);
      expect(meta["twitter:image"]).toBe(SOCIAL_IMAGE.url);
      expect(meta["robots"]).toContain("index, follow");
    }
  });

  it("describes the free app without presenting the waitlisted plan or ratings as available", () => {
    const graph = JSON.parse(
      pageHead("/pricing", PRICING_FAQS).scripts[0].children
    )["@graph"];
    const application = graph.find(
      (node: Record<string, unknown>) => node["@type"] === "SoftwareApplication"
    );
    expect(application.offers.price).toBe("0");
    expect(application.offers.description).toContain(
      "bill API usage separately"
    );
    expect(application).not.toHaveProperty("aggregateRating");
    expect(application.downloadUrl).toBe(`${SITE_URL}/downloads`);
    expect(application.operatingSystem).not.toContain("macOS");
  });

  it("uses the displayed FAQ answers and keeps breadcrumb URLs canonical", () => {
    for (const [path, faqs] of [
      ["/", HOME_FAQS],
      ["/pricing", PRICING_FAQS],
    ] as const) {
      const graph = JSON.parse(pageHead(path, faqs).scripts[0].children)[
        "@graph"
      ];
      const faq = graph.find(
        (node: Record<string, unknown>) => node["@type"] === "FAQPage"
      );
      expect(
        faq.mainEntity.map(
          (item: { name: string; acceptedAnswer: { text: string } }) => ({
            question: item.name,
            answer: item.acceptedAnswer.text,
          })
        )
      ).toEqual(faqs);
      const breadcrumbs = graph.find(
        (node: Record<string, unknown>) => node["@type"] === "BreadcrumbList"
      );
      if (path === "/") {
        expect(breadcrumbs).toBeUndefined();
      } else {
        expect(breadcrumbs.itemListElement.at(-1).item).toBe(pageUrl(path));
      }
    }
  });

  it("cannot terminate an inline JSON-LD script through its content", () => {
    const value = { text: '</script><script>alert("x")</script>' };
    const serialized = serializeJsonLd(value);
    expect(serialized).not.toContain("<");
    expect(JSON.parse(serialized)).toEqual(value);
  });
});

describe("Crawler endpoints and indexing boundaries", () => {
  it("includes every public page once in the sitemap and advertises it in robots.txt", () => {
    const document = new DOMParser().parseFromString(
      sitemapXml(),
      "application/xml"
    );
    expect(document.querySelector("parsererror")).toBeNull();
    expect(
      [...document.querySelectorAll("loc")].map((node) => node.textContent)
    ).toEqual(PAGE_PATHS.map(pageUrl));
    expect(robotsTxt()).toContain(`Sitemap: ${SITE_URL}/sitemap.xml`);
    expect(robotsTxt()).toContain("Disallow: /_serverFn/");
    expect(robotsTxt(true)).toBe("User-agent: *\nDisallow: /\n");
  });

  it("serves crawler documents with correct types and an empty HEAD body", async () => {
    const response = crawlerResponse(sitemapXml(), "application/xml");
    expect(response.status).toBe(200);
    expect(response.headers.get("Content-Type")).toBe(
      "application/xml; charset=utf-8"
    );
    expect(await response.text()).toBe(sitemapXml());
    expect(
      await crawlerResponse(sitemapXml(), "application/xml", true).text()
    ).toBe("");
  });

  it("normalizes only public GET/HEAD page paths without discarding campaign parameters", () => {
    const redirect = canonicalPathRedirect(
      new Request(`${SITE_URL}/pricing/?utm_source=x`)
    );
    expect(redirect?.status).toBe(308);
    expect(redirect?.headers.get("Location")).toBe("/pricing?utm_source=x");
    expect(
      canonicalPathRedirect(
        new Request(`${SITE_URL}/downloads/`, { method: "HEAD" })
      )?.status
    ).toBe(308);
    for (const path of ["/", "/pricing", "/missing/", "/_serverFn/action/"]) {
      expect(
        canonicalPathRedirect(new Request(`${SITE_URL}${path}`))
      ).toBeNull();
    }
    expect(
      canonicalPathRedirect(
        new Request(`${SITE_URL}/pricing/`, { method: "POST" })
      )
    ).toBeNull();
  });

  it("keeps errors, server functions and preview responses out of search without changing success responses", async () => {
    const request = new Request(`${SITE_URL}/pricing`);
    const success = new Response("page");
    expect(searchResponse(success, request, false)).toBe(success);
    await Promise.all(
      [404, 500].map(async (status) => {
        const response = searchResponse(
          new Response("error", { status }),
          request,
          false
        );
        expect(response.status).toBe(status);
        expect(response.headers.get("X-Robots-Tag")).toBe("noindex, follow");
        expect(response.headers.get("Cache-Control")).toBe("no-store");
        expect(await response.text()).toBe("error");
      })
    );
    expect(
      searchResponse(new Response("preview"), request, true).headers.get(
        "X-Robots-Tag"
      )
    ).toContain("noindex");
    expect(
      searchResponse(
        new Response("result"),
        new Request(`${SITE_URL}/_serverFn/action`),
        false
      ).headers.get("X-Robots-Tag")
    ).toContain("noindex");
  });
});
