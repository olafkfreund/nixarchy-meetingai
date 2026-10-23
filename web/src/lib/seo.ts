import type { FaqEntry } from "./faqs";
import {
  ASSET_URL,
  GITHUB_URL,
  MANAGED_PLAN,
  SITE_URL,
  SOCIAL_IMAGE,
} from "./site";

export const SEO_PAGES = {
  "/": {
    name: "Lennette AI Meeting Copilot",
    title: "Lennette | Live AI Meeting Copilot for Windows & Linux",
    description:
      "Get live transcription and contextual AI answers during meetings. Lennette is a native Windows and Linux copilot, free with your own provider API keys.",
    type: "WebPage",
  },
  "/pricing": {
    name: "Pricing",
    title: `Lennette Pricing | Free BYOK & ${MANAGED_PLAN.price}/Month Managed`,
    description: `Use Lennette free with your own AI and transcription keys. Join the waitlist for Managed at ${MANAGED_PLAN.price}/month, with provider usage included.`,
    type: "WebPage",
  },
  "/downloads": {
    name: "Downloads",
    title: "Download Lennette | AI Meeting Copilot for Windows & Linux",
    description:
      "Download the free Lennette desktop app for Windows and Linux. Get portable Windows and Linux builds, release checksums, and Hyprland setup instructions.",
    type: "CollectionPage",
  },
  "/tutorial": {
    name: "Hyprland Setup",
    title: "Lennette Hyprland Setup | Linux Private Overlay Guide",
    description:
      "Set up Lennette's private overlay on Hyprland. Follow the NixOS or Linux build guide, configure shortcuts, and understand screen-share protection limits.",
    type: "WebPage",
  },
} as const;

export type PagePath = keyof typeof SEO_PAGES;
export const PAGE_PATHS = Object.keys(SEO_PAGES) as PagePath[];

export const pageUrl = (path: PagePath): string => `${SITE_URL}${path}`;

// JSON-LD is raw script text. Escape '<' so content cannot close the script tag.
export const serializeJsonLd = (value: unknown): string =>
  JSON.stringify(value).replaceAll("<", "\\u003c");

export const pageHead = (path: PagePath, faqs: readonly FaqEntry[] = []) => {
  const page = SEO_PAGES[path];
  const url = pageUrl(path);
  const websiteId = `${SITE_URL}/#website`;
  const publisherId = `${SITE_URL}/#publisher`;
  const applicationId = `${SITE_URL}/#application`;
  const imageId = `${url}#social-image`;
  const breadcrumbId = `${url}#breadcrumbs`;
  const graph: Record<string, unknown>[] = [
    {
      "@type": "Organization",
      "@id": publisherId,
      name: "Lennette",
      url: pageUrl("/"),
      logo: `${ASSET_URL}/lennette_app_icon.png`,
      sameAs: [GITHUB_URL],
    },
    {
      "@type": "WebSite",
      "@id": websiteId,
      url: pageUrl("/"),
      name: "Lennette",
      inLanguage: "en",
      publisher: { "@id": publisherId },
    },
    {
      "@type": "SoftwareApplication",
      "@id": applicationId,
      name: "Lennette",
      url: pageUrl("/"),
      applicationCategory: "BusinessApplication",
      operatingSystem: "Windows 10 or later, Linux",
      description: SEO_PAGES["/"].description,
      image: SOCIAL_IMAGE.url,
      screenshot: `${ASSET_URL}/gmeet.png`,
      downloadUrl: pageUrl("/downloads"),
      publisher: { "@id": publisherId },
      sameAs: [GITHUB_URL],
      offers: {
        "@type": "Offer",
        name: "Free BYOK desktop app",
        price: "0",
        priceCurrency: "USD",
        url: pageUrl("/downloads"),
        description:
          "The desktop app is free. AI and transcription providers bill API usage separately. Managed access is not available yet.",
      },
    },
    {
      "@type": "ImageObject",
      "@id": imageId,
      url: SOCIAL_IMAGE.url,
      width: SOCIAL_IMAGE.width,
      height: SOCIAL_IMAGE.height,
      caption: SOCIAL_IMAGE.alt,
    },
    {
      "@type": page.type,
      "@id": `${url}#webpage`,
      url,
      name: page.title,
      description: page.description,
      inLanguage: "en",
      isPartOf: { "@id": websiteId },
      about: { "@id": applicationId },
      primaryImageOfPage: { "@id": imageId },
      ...(path === "/" ? {} : { breadcrumb: { "@id": breadcrumbId } }),
      ...(faqs.length ? { hasPart: { "@id": `${url}#faq` } } : {}),
    },
  ];

  if (path !== "/") {
    graph.push({
      "@type": "BreadcrumbList",
      "@id": breadcrumbId,
      itemListElement: [
        { "@type": "ListItem", position: 1, name: "Home", item: pageUrl("/") },
        { "@type": "ListItem", position: 2, name: page.name, item: url },
      ],
    });
  }
  if (faqs.length) {
    graph.push({
      "@type": "FAQPage",
      "@id": `${url}#faq`,
      isPartOf: { "@id": `${url}#webpage` },
      mainEntity: faqs.map(({ question, answer }) => ({
        "@type": "Question",
        name: question,
        acceptedAnswer: { "@type": "Answer", text: answer },
      })),
    });
  }

  return {
    meta: [
      { title: page.title },
      { name: "description", content: page.description },
      {
        name: "robots",
        content:
          "index, follow, max-image-preview:large, max-snippet:-1, max-video-preview:-1",
      },
      { property: "og:type", content: "website" },
      { property: "og:site_name", content: "Lennette" },
      { property: "og:locale", content: "en_US" },
      { property: "og:title", content: page.title },
      { property: "og:description", content: page.description },
      { property: "og:url", content: url },
      { property: "og:image", content: SOCIAL_IMAGE.url },
      { property: "og:image:secure_url", content: SOCIAL_IMAGE.url },
      { property: "og:image:type", content: SOCIAL_IMAGE.type },
      { property: "og:image:width", content: String(SOCIAL_IMAGE.width) },
      { property: "og:image:height", content: String(SOCIAL_IMAGE.height) },
      { property: "og:image:alt", content: SOCIAL_IMAGE.alt },
      { name: "twitter:card", content: "summary_large_image" },
      { name: "twitter:title", content: page.title },
      { name: "twitter:description", content: page.description },
      { name: "twitter:image", content: SOCIAL_IMAGE.url },
      { name: "twitter:image:alt", content: SOCIAL_IMAGE.alt },
    ],
    links: [{ rel: "canonical", href: url }],
    scripts: [
      {
        type: "application/ld+json",
        children: serializeJsonLd({
          "@context": "https://schema.org",
          "@graph": graph,
        }),
      },
    ] satisfies [{ type: string; children: string }],
  };
};
