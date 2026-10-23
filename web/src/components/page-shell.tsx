import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";

import skyFooter from "@/assets/sky-footer.jpg";

const appIcon =
  "https://swr2.itssingularity.com/lennette/lennette_app_icon.png";
const footerArt =
  "https://swr2.itssingularity.com/lennette/lennette_footer_with_text.png";

const GITHUB_URL = "https://github.com/parazeeknova/lennette";
const RELEASES_URL = "https://github.com/parazeeknova/lennette/releases";

const GithubMark = ({ className = "" }: { className?: string }) => (
  <svg
    viewBox="0 0 24 24"
    aria-hidden="true"
    className={className}
    fill="currentColor"
  >
    <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" />
  </svg>
);

const navLinks = [
  { label: "How it works", to: "/", hash: "how" },
  { label: "Features", to: "/", hash: "features" },
  { label: "Tutorial", to: "/tutorial" },
  { label: "Downloads", to: "/downloads" },
  { label: "Pricing", to: "/pricing" },
] as const;

export const SiteNav = () => (
  <header className="absolute inset-x-0 top-0 z-30 px-4 pt-5 sm:px-8">
    <nav className="glass mx-auto flex h-16 max-w-5xl items-center justify-between glass-pill pr-2 pl-4">
      <Link to="/" className="relative z-10 flex items-center gap-2.5">
        <img
          src="/lennette.png"
          alt=""
          width={44}
          height={30}
          className="h-8 w-auto"
        />
        <span className="font-display text-2xl leading-none text-ink">
          lennette
        </span>
      </Link>
      <div className="relative z-10 hidden items-center gap-7 text-sm font-medium text-ink-soft lg:flex">
        {navLinks.map((link) => (
          <Link
            key={link.label}
            to={link.to}
            {...("hash" in link ? { hash: link.hash } : {})}
            className="transition-colors hover:text-ink"
          >
            {link.label}
          </Link>
        ))}
      </div>
      <a
        href={RELEASES_URL}
        target="_blank"
        rel="noreferrer"
        className="glass btn-liquid h-12 glass-pill px-5 text-sm"
      >
        <span>Get lennette</span>
      </a>
    </nav>
  </header>
);

export const SiteFooter = () => (
  <footer className="relative">
    <img
      src={skyFooter}
      alt=""
      loading="lazy"
      width={1920}
      height={1088}
      className="absolute inset-0 h-full w-full feather-top object-cover"
    />
    <img
      src={footerArt}
      alt=""
      aria-hidden="true"
      loading="lazy"
      width={1983}
      height={793}
      className="pointer-events-none absolute inset-x-0 bottom-0 mx-auto w-full max-w-5xl"
    />
    <div className="relative z-10 mx-auto max-w-5xl px-6 pt-28 sm:px-10">
      <div className="flex flex-col items-center gap-10 text-center lg:flex-row lg:items-center lg:justify-between lg:gap-14 lg:text-left">
        <h2 className="max-w-xl font-display text-[clamp(2.4rem,5.5vw,4.5rem)] leading-[1.05] text-ink">
          Walk into every meeting ready.
        </h2>
        <div className="flex shrink-0 flex-col items-center gap-6 sm:flex-row lg:flex-col lg:items-end">
          <img
            src={appIcon}
            alt="lennette app icon"
            width={96}
            height={96}
            className="h-20 w-20 rounded-3xl"
          />
          <div className="flex flex-col gap-3">
            <a
              href={RELEASES_URL}
              target="_blank"
              rel="noreferrer"
              className="glass btn-liquid glass-pill glass-blue text-on-sky hover:bg-[oklch(0.5_0.15_252/0.25)]"
            >
              <span>Download lennette</span>
            </a>
            <a
              href={GITHUB_URL}
              target="_blank"
              rel="noreferrer"
              className="glass btn-liquid glass-pill"
            >
              <GithubMark className="h-5 w-5" />
              <span>Star on GitHub</span>
            </a>
          </div>
        </div>
      </div>

      <div className="mt-14 flex flex-wrap items-center justify-center gap-x-8 gap-y-3 text-sm text-ink-soft">
        <Link to="/tutorial" className="transition-colors hover:text-ink">
          Hyprland setup
        </Link>
        <Link to="/downloads" className="transition-colors hover:text-ink">
          Downloads
        </Link>
        <Link to="/pricing" className="transition-colors hover:text-ink">
          Pricing
        </Link>
        <a
          href="https://itssingularity.com"
          target="_blank"
          rel="noreferrer"
          className="transition-colors hover:text-ink"
        >
          Singularity Works
        </a>
        <a
          href="https://github.com/parazeeknova"
          target="_blank"
          rel="noreferrer"
          className="transition-colors hover:text-ink"
        >
          Creator
        </a>
      </div>

      {/* Room for the mascot illustration pinned to the bottom edge. Its height
          is 40vw until the art hits max-w-5xl, then it caps at 409px. */}
      <div aria-hidden="true" className="mt-10 h-[40vw] max-h-102.25 w-full" />
    </div>
  </footer>
);

export const PageShell = ({
  children,
  title,
  intro,
}: {
  children: ReactNode;
  title: string;
  intro: string;
}) => (
  <main className="overflow-x-hidden bg-sky-wash">
    <SiteNav />
    <section className="px-6 pt-40 pb-16 sm:px-10">
      <div className="mx-auto max-w-5xl">
        <h1 className="max-w-3xl font-display text-[clamp(2.6rem,6vw,4.8rem)] leading-[1.02] tracking-[-0.015em] text-ink">
          {title}
        </h1>
        <p className="mt-6 max-w-2xl text-lg leading-relaxed text-ink-soft">
          {intro}
        </p>
      </div>
    </section>
    <div className="px-6 pb-24 sm:px-10">
      <div className="mx-auto max-w-5xl">{children}</div>
    </div>
    <SiteFooter />
  </main>
);

export { GITHUB_URL, RELEASES_URL };
