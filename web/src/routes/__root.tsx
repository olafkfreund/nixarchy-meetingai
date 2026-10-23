import type { QueryClient } from "@tanstack/react-query";
import { QueryClientProvider } from "@tanstack/react-query";
import type { ErrorComponentProps } from "@tanstack/react-router";
import {
  Outlet,
  Link,
  createRootRouteWithContext,
  useRouter,
  HeadContent,
  Scripts,
} from "@tanstack/react-router";
import { useEffect } from "react";
import type { ReactNode } from "react";

import appCss from "../styles.css?url";

const NotFoundComponent = () => (
  <div className="flex min-h-screen items-center justify-center bg-background px-4">
    <div className="max-w-md text-center">
      <h1 className="font-display text-7xl text-foreground">404</h1>
      <h2 className="mt-4 text-xl font-semibold text-foreground">
        Page not found
      </h2>
      <p className="mt-2 text-sm text-muted-foreground">
        The page you&apos;re looking for doesn&apos;t exist or has been moved.
      </p>
      <div className="mt-6">
        <Link
          to="/"
          className="inline-flex items-center justify-center rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
        >
          Go home
        </Link>
      </div>
    </div>
  </div>
);

const ErrorComponent = ({ error, reset }: ErrorComponentProps) => {
  const router = useRouter();
  useEffect(() => {
    console.error(error);
  }, [error]);

  return (
    <div className="flex min-h-screen items-center justify-center bg-background px-4">
      <div className="max-w-md text-center">
        <h1 className="text-xl font-semibold tracking-tight text-foreground">
          This page didn&apos;t load
        </h1>
        <p className="mt-2 text-sm text-muted-foreground">
          Something went wrong on our end. You can try refreshing or head back
          home.
        </p>
        <div className="mt-6 flex flex-wrap justify-center gap-2">
          <button
            type="button"
            onClick={() => {
              router.invalidate();
              reset();
            }}
            className="inline-flex items-center justify-center rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
          >
            Try again
          </button>
          <a
            href="/"
            className="inline-flex items-center justify-center rounded-md border border-input bg-background px-4 py-2 text-sm font-medium text-foreground transition-colors hover:bg-accent"
          >
            Go home
          </a>
        </div>
      </div>
    </div>
  );
};

const RootShell = ({ children }: { children: ReactNode }) => (
  <html lang="en">
    <head>
      <HeadContent />
      {/* Analytics only in production builds. import.meta.env.PROD is replaced
          at build time, so the tag is absent entirely during local dev. */}
      {import.meta.env.PROD ? (
        <script
          defer
          src="https://tracking.przknv.cc/script.js"
          data-website-id="8ef5aa50-97b4-4deb-8a6b-e5c825338bd9"
        />
      ) : null}
    </head>
    <body>
      {children}
      <Scripts />
    </body>
  </html>
);

const RootComponent = () => {
  const { queryClient } = Route.useRouteContext();

  return (
    <QueryClientProvider client={queryClient}>
      <Outlet />
    </QueryClientProvider>
  );
};

export const Route = createRootRouteWithContext<{ queryClient: QueryClient }>()(
  {
    head: () => ({
      meta: [
        { charSet: "utf-8" },
        { name: "viewport", content: "width=device-width, initial-scale=1" },
        { title: "Lennette" },
        { name: "description", content: "Know exactly what to say, live." },
        // Public pages override this. Missing/error routes should not be indexed.
        { name: "robots", content: "noindex, follow" },
        { name: "application-name", content: "Lennette" },
        { name: "theme-color", content: "#d0edff" },
      ],
      links: [
        { rel: "stylesheet", href: appCss },
        { rel: "preconnect", href: "https://api.fontshare.com" },
        {
          rel: "preconnect",
          href: "https://cdn.fontshare.com",
          crossOrigin: "",
        },
        // Satoshi is self-hosted from src/fonts, so only the display face is
        // fetched from Fontshare here.
        {
          rel: "stylesheet",
          href: "https://api.fontshare.com/v2/css?f[]=gambarino@400&display=swap",
        },
        {
          rel: "icon",
          type: "image/png",
          sizes: "48x48",
          href: "/favicon.png",
        },
        {
          rel: "apple-touch-icon",
          sizes: "180x180",
          href: "/apple-touch-icon.png",
        },
        { rel: "manifest", href: "/site.webmanifest" },
      ],
    }),
    shellComponent: RootShell,
    component: RootComponent,
    notFoundComponent: NotFoundComponent,
    errorComponent: ErrorComponent,
  }
);
