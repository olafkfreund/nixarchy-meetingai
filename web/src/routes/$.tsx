import { Link, createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import skyHero from "@/assets/sky-hero.jpg";
import { pillLinkClassName } from "@/components/page-shell";
import { PlatformDownload } from "@/components/platform-download";
import { WaitlistDialog } from "@/components/waitlist-dialog";

// Keep the fine print legible against the clouds.
const inkHalo =
  "0 1px 1px oklch(0.98 0.015 235 / 0.9), 0 0 14px oklch(0.98 0.015 235 / 0.75)";

// Frosted "glass balloon" standing in for the middle zero: the same rim, top
// sheen and self-coloured edge as the glass utilities, plus the mascot's two
// eyes, lit from above like the app icon. Sized in em so it scales with the
// digit row.
const GlassZero = () => (
  <svg
    viewBox="0 0 100 100"
    aria-hidden="true"
    className="inline-block h-[0.79em] w-[0.61em] shrink-0 self-center align-middle"
  >
    <defs>
      <linearGradient id="zero-body" x1="30%" y1="0%" x2="60%" y2="100%">
        <stop offset="0" stopColor="oklch(1 0 0)" stopOpacity="0.42" />
        <stop
          offset="0.6"
          stopColor="oklch(0.96 0.03 232)"
          stopOpacity="0.24"
        />
        <stop offset="1" stopColor="oklch(0.9 0.06 238)" stopOpacity="0.28" />
      </linearGradient>
      <linearGradient id="zero-rim" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0" stopColor="oklch(1 0 0)" />
        <stop
          offset="0.34"
          stopColor="oklch(0.85 0.08 230)"
          stopOpacity="0.5"
        />
        <stop offset="0.52" stopColor="oklch(1 0 0)" stopOpacity="0.3" />
        <stop
          offset="0.8"
          stopColor="oklch(0.75 0.12 245)"
          stopOpacity="0.65"
        />
        <stop offset="1" stopColor="oklch(1 0 0)" stopOpacity="0.9" />
      </linearGradient>
      {/* one soft key light from the top left, a cool bounce below */}
      <radialGradient id="zero-sheen" cx="28%" cy="-10%" r="92%">
        <stop offset="0" stopColor="oklch(1 0 0)" stopOpacity="0.5" />
        <stop offset="0.46" stopColor="oklch(1 0 0)" stopOpacity="0.12" />
        <stop offset="1" stopColor="oklch(1 0 0)" stopOpacity="0" />
      </radialGradient>
      <radialGradient id="zero-floor" cx="60%" cy="108%" r="62%">
        <stop offset="0" stopColor="oklch(0.74 0.11 240)" stopOpacity="0.34" />
        <stop offset="1" stopColor="oklch(0.74 0.11 240)" stopOpacity="0" />
      </radialGradient>
      {/* the eyes are lit from above, like the mascot's */}
      <radialGradient id="zero-eye" cx="46%" cy="34%" r="72%">
        <stop offset="0" stopColor="oklch(0.5 0.17 255)" />
        <stop offset="0.42" stopColor="oklch(0.32 0.14 260)" />
        <stop offset="1" stopColor="oklch(0.21 0.09 264)" />
      </radialGradient>
      <clipPath id="zero-clip">
        <rect x="4.5" y="2.5" width="91" height="95" rx="45.5" />
      </clipPath>
    </defs>

    <g clipPath="url(#zero-clip)">
      <rect
        x="4.5"
        y="2.5"
        width="91"
        height="95"
        rx="45.5"
        fill="url(#zero-body)"
      />
      <rect
        x="4.5"
        y="2.5"
        width="91"
        height="95"
        rx="45.5"
        fill="url(#zero-sheen)"
      />
      <rect
        x="4.5"
        y="2.5"
        width="91"
        height="95"
        rx="45.5"
        fill="url(#zero-floor)"
      />

      <rect
        x="27.5"
        y="32"
        width="19"
        height="26"
        rx="9.5"
        transform="rotate(-13 37 45)"
        fill="url(#zero-eye)"
      />
      <rect
        x="56"
        y="43.5"
        width="18"
        height="25"
        rx="9"
        transform="rotate(11 65 56)"
        fill="url(#zero-eye)"
      />
      <circle cx="33" cy="40.6" r="1.9" fill="oklch(1 0 0 / 0.92)" />
      <circle cx="59.4" cy="51.8" r="1.7" fill="oklch(1 0 0 / 0.9)" />
    </g>

    <rect
      x="4.5"
      y="2.5"
      width="91"
      height="95"
      rx="45.5"
      fill="none"
      stroke="url(#zero-rim)"
      strokeWidth="1.4"
    />
  </svg>
);

interface NavItem {
  label: string;
  to?: "/" | "/downloads" | "/pricing" | "/tutorial";
  hash?: string;
  waitlist?: boolean;
}

const navItems: NavItem[] = [
  { label: "Home", to: "/" },
  { label: "Features", to: "/", hash: "features" },
  { label: "Downloads", to: "/downloads" },
  { label: "Pricing", to: "/pricing" },
  { label: "Managed waitlist", waitlist: true },
];

const NotFound = () => {
  const [waitlistOpen, setWaitlistOpen] = useState(false);

  return (
    <main className="flex min-h-svh flex-col overflow-x-hidden bg-cloud">
      <section className="relative isolate flex min-h-svh flex-1 flex-col overflow-hidden">
        {/* Same sky as the hero, same slow drift. It bleeds into the page
            colour below instead of ending on a seam. */}
        <img
          src={skyHero}
          alt=""
          width={1920}
          height={1088}
          fetchPriority="high"
          className="absolute inset-0 -z-20 h-full w-full animate-drift object-cover"
        />
        <div className="absolute inset-x-0 bottom-0 -z-10 h-72 bg-linear-to-b from-transparent to-cloud" />

        <header className="relative z-30 flex items-center justify-between gap-4 px-6 pt-7 sm:px-10">
          <Link to="/" className="flex items-center gap-2.5">
            <img
              src="/lennette.png"
              alt=""
              width={44}
              height={30}
              className="h-8 w-auto"
            />
            <span className="font-display text-2xl leading-none text-on-sky [text-shadow:0_1px_10px_oklch(0.42_0.12_250/0.35)]">
              lennette
            </span>
          </Link>
          <Link
            to="/"
            className="glass btn-liquid h-11 glass-pill px-5 text-sm"
          >
            <span>Back to the sky</span>
          </Link>
        </header>

        <div className="relative z-10 mx-auto flex w-full max-w-6xl flex-1 flex-col items-center justify-center px-6 pt-8 pb-16 text-center">
          <h1
            aria-label="404"
            className="flex items-center justify-center font-display text-[clamp(6rem,23vw,18rem)] leading-[0.9] whitespace-nowrap text-on-sky text-sky-shadow"
          >
            <span aria-hidden="true">4</span>
            <span aria-hidden="true">
              <GlassZero />
            </span>
            <span aria-hidden="true">4</span>
          </h1>

          <PlatformDownload />

          <nav
            aria-label="Popular pages"
            className="mt-6 flex max-w-3xl flex-wrap items-center justify-center gap-3 sm:gap-4"
          >
            {navItems.map((item) =>
              item.waitlist ? (
                <button
                  key={item.label}
                  type="button"
                  onClick={() => setWaitlistOpen(true)}
                  className={pillLinkClassName}
                >
                  <span>{item.label}</span>
                </button>
              ) : (
                <Link
                  key={item.label}
                  to={item.to ?? "/"}
                  {...(item.hash ? { hash: item.hash } : {})}
                  className={pillLinkClassName}
                >
                  <span>{item.label}</span>
                </Link>
              )
            )}
          </nav>

          <p
            className="mt-12 text-xs text-ink-soft"
            style={{ textShadow: inkHalo }}
          >
            Lennette is still up there, flying. This page never was.
          </p>
        </div>
      </section>

      <WaitlistDialog
        open={waitlistOpen}
        onClose={() => setWaitlistOpen(false)}
      />
    </main>
  );
};

export const Route = createFileRoute("/$")({
  // Every path that lands here is a miss, so the route owns the head: a
  // dedicated title and the same noindex policy the root shell assumes for
  // unknown routes.
  head: () => ({
    meta: [
      { title: "Page not found | Lennette" },
      { name: "robots", content: "noindex, follow" },
    ],
  }),
  component: NotFound,
});
