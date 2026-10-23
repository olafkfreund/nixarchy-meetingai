import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";

const GITHUB_URL = "https://github.com/parazeeknova/lennette";
const RELEASES_URL = "https://github.com/parazeeknova/lennette/releases";

const navLinks = [
  { label: "Features", to: "/", hash: "features" },
  { label: "Tutorial", to: "/tutorial" },
  { label: "Downloads", to: "/downloads" },
  { label: "Pricing", to: "/pricing" },
] as const;

const SiteNav = () => (
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
      <div className="relative z-10 hidden items-center gap-8 text-sm font-medium text-ink-soft md:flex">
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
        <span>Download</span>
      </a>
    </nav>
  </header>
);

const SiteFooter = () => (
  <footer className="relative border-t border-ink/10 px-6 py-14 sm:px-10">
    <div className="mx-auto flex max-w-5xl flex-col items-center gap-6 text-sm text-ink-soft sm:flex-row sm:justify-between">
      <span>© 2026 lennette</span>
      <nav className="flex flex-wrap justify-center gap-x-7 gap-y-2">
        <Link to="/tutorial" className="transition-colors hover:text-ink">
          Tutorial
        </Link>
        <Link to="/downloads" className="transition-colors hover:text-ink">
          Downloads
        </Link>
        <Link to="/pricing" className="transition-colors hover:text-ink">
          Pricing
        </Link>
        <a
          href={GITHUB_URL}
          target="_blank"
          rel="noreferrer"
          className="transition-colors hover:text-ink"
        >
          GitHub
        </a>
      </nav>
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
