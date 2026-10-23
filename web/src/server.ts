// Importing error-capture first installs its console/global error hooks before
// anything else in the server bundle can log.
import { consumeLastCapturedError } from "./lib/error-capture";
import { renderErrorPage } from "./lib/error-page";

interface ServerEntry {
  fetch: (
    request: Request,
    env: unknown,
    ctx: unknown
  ) => Promise<Response> | Response;
}

let serverEntryPromise: Promise<ServerEntry> | undefined;

const isH3SwallowedErrorBody = (body: string): boolean => {
  try {
    const payload = JSON.parse(body) as {
      unhandled?: unknown;
      message?: unknown;
    };
    return payload.unhandled === true && payload.message === "HTTPError";
  } catch {
    return false;
  }
};

const serverEntry = async (): Promise<ServerEntry> => {
  if (!serverEntryPromise) {
    const module = await import("@tanstack/react-start/server-entry");
    serverEntryPromise = Promise.resolve(
      (module.default ?? module) as ServerEntry
    );
  }
  return serverEntryPromise;
};
// h3 swallows in-handler throws into a normal 500 Response with body
// {"unhandled":true,"message":"HTTPError"} — try/catch alone never fires for those.
const normalizeCatastrophicSsrResponse = async (
  response: Response
): Promise<Response> => {
  if (response.status < 500) {
    return response;
  }
  const contentType = response.headers.get("content-type") ?? "";
  if (!contentType.includes("application/json")) {
    return response;
  }

  const body = await response.clone().text();
  if (!isH3SwallowedErrorBody(body)) {
    return response;
  }

  console.error(
    consumeLastCapturedError() ?? new Error(`h3 swallowed SSR error: ${body}`)
  );
  return new Response(renderErrorPage(), {
    status: 500,
    headers: { "content-type": "text/html; charset=utf-8" },
  });
};

export default {
  async fetch(request: Request, env: unknown, ctx: unknown) {
    try {
      const handler = await serverEntry();
      const response = await handler.fetch(request, env, ctx);
      return await normalizeCatastrophicSsrResponse(response);
    } catch (error) {
      console.error(error);
      return new Response(renderErrorPage(), {
        status: 500,
        headers: { "content-type": "text/html; charset=utf-8" },
      });
    }
  },
};
