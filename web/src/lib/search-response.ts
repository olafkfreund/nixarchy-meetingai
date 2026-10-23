import { PAGE_PATHS } from "./seo";

export const canonicalPathRedirect = (request: Request): Response | null => {
  if (request.method !== "GET" && request.method !== "HEAD") {
    return null;
  }
  const url = new URL(request.url);
  const normalized = url.pathname.replace(/\/+$/u, "") || "/";
  if (
    normalized !== url.pathname &&
    PAGE_PATHS.some((path) => path === normalized)
  ) {
    return new Response(null, {
      status: 308,
      headers: { Location: `${normalized}${url.search}` },
    });
  }
  return null;
};

export const searchResponse = (
  response: Response,
  request: Request,
  noindex: boolean
): Response => {
  const isError = response.status >= 400;
  const isServerFunction = new URL(request.url).pathname.startsWith(
    "/_serverFn/"
  );
  if (!(isError || isServerFunction || noindex)) {
    return response;
  }
  const headers = new Headers(response.headers);
  headers.set("X-Robots-Tag", "noindex, follow");
  if (isError || noindex) {
    headers.set("Cache-Control", "no-store");
  }
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
};
