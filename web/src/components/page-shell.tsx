import { Link } from "@tanstack/react-router";
import { ChevronDown, X } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import type { ReactNode } from "react";

import skyFooter from "@/assets/sky-footer.jpg";
import { PlatformDownload } from "@/components/platform-download";
import { GITHUB_URL } from "@/lib/site";

const appIcon =
  "https://swr2.itssingularity.com/lennette/lennette_app_icon.png";
const footerArt =
  "https://swr2.itssingularity.com/lennette/lennette_footer_with_text.png";

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

export const SiteNav = () => {
  const [open, setOpen] = useState(false);
  const menu = useRef<HTMLDivElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const menuId = useId();

  useEffect(() => {
    if (!open) {
      return;
    }
    const dismissOutside = (event: PointerEvent) => {
      if (
        event.target instanceof Node &&
        !menu.current?.contains(event.target)
      ) {
        setOpen(false);
      }
    };
    const dismissEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        toggle.current?.focus();
      }
    };
    document.addEventListener("pointerdown", dismissOutside);
    document.addEventListener("keydown", dismissEscape);
    return () => {
      document.removeEventListener("pointerdown", dismissOutside);
      document.removeEventListener("keydown", dismissEscape);
    };
  }, [open]);

  return (
    <header className="absolute inset-x-0 top-0 z-30 px-4 pt-5 sm:px-8">
      <nav
        aria-label="Main navigation"
        className="glass mx-auto flex h-16 max-w-5xl items-center justify-between gap-3 glass-pill pr-2 pl-4"
      >
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
        <div className="relative z-20 flex items-center gap-2">
          <div ref={menu} className="relative lg:hidden">
            <button
              ref={toggle}
              type="button"
              aria-expanded={open}
              aria-controls={menuId}
              aria-label={
                open ? "Close page navigation" : "Open page navigation"
              }
              onClick={() => setOpen(!open)}
              className="glass flex h-11 items-center justify-center gap-2 glass-pill px-4 text-sm font-semibold text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-deep"
            >
              <span className="relative z-10">Pages</span>
              {open ? (
                <X className="relative z-10 size-4" />
              ) : (
                <ChevronDown className="relative z-10 size-4" />
              )}
            </button>
            {open ? (
              <div
                id={menuId}
                className="glass absolute top-full right-0 mt-3 w-64 max-w-[calc(100vw-3rem)] rounded-3xl bg-cloud/95 p-2"
              >
                {navLinks.map((link) => (
                  <Link
                    key={link.label}
                    to={link.to}
                    {...("hash" in link ? { hash: link.hash } : {})}
                    onClick={() => setOpen(false)}
                    className="relative z-10 flex min-h-12 items-center rounded-2xl px-4 text-base font-medium text-ink transition-colors hover:bg-white/65 focus-visible:bg-white/65 focus-visible:outline-2 focus-visible:outline-sky-deep"
                  >
                    {link.label}
                  </Link>
                ))}
              </div>
            ) : null}
          </div>
          <Link
            to="/downloads"
            className="glass btn-liquid hidden h-12 glass-pill px-5 text-sm sm:inline-flex"
          >
            <span>Get lennette</span>
          </Link>
        </div>
      </nav>
    </header>
  );
};

export const pillLinkClassName =
  "glass btn-liquid h-12 glass-pill px-5 text-base text-ink focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-sky-deep sm:h-14 sm:px-6 sm:text-lg";

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
    <div className="relative z-10 mx-auto max-w-5xl px-6 pt-28 sm:px-10">
      <div className="flex flex-col items-center gap-10 text-center lg:flex-row lg:items-end lg:justify-between lg:gap-14 lg:text-left">
        <h2 className="max-w-xl font-display text-[clamp(2.4rem,5.5vw,4.5rem)] leading-[1.05] text-ink">
          Walk into every meeting ready.
        </h2>
        <div className="flex shrink-0 flex-col items-center gap-6 sm:flex-row lg:flex-col lg:items-end">
          <img
            src={appIcon}
            alt="lennette app icon"
            width={96}
            height={96}
            className="h-20 w-20 rounded-3xl lg:-translate-y-4"
          />
          <div className="flex flex-col gap-3">
            <PlatformDownload
              id="footer-download"
              className="glass-blue text-on-sky hover:bg-[oklch(0.5_0.15_252/0.25)]"
            />
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

      {/* Keep the illustration in flow above the footer links. */}
      <img
        src={footerArt}
        alt=""
        aria-hidden="true"
        loading="lazy"
        width={1983}
        height={793}
        className="pointer-events-none mt-10 w-full"
      />
    </div>

    <div className="relative z-20 mx-auto max-w-5xl px-6 pt-4 pb-10 sm:px-10 sm:pb-14">
      <nav
        aria-label="Footer"
        className="flex flex-wrap items-center justify-center gap-3 sm:gap-4"
      >
        <Link to="/tutorial" className={pillLinkClassName}>
          <span>Hyprland setup</span>
        </Link>
        <Link to="/downloads" className={pillLinkClassName}>
          <span>Downloads</span>
        </Link>
        <Link to="/pricing" className={pillLinkClassName}>
          <span>Pricing</span>
        </Link>
        <a
          href="https://itssingularity.com"
          target="_blank"
          rel="noreferrer"
          className={pillLinkClassName}
        >
          <span>Singularity Works</span>
        </a>
        <a
          href="https://github.com/parazeeknova"
          target="_blank"
          rel="noreferrer"
          className={pillLinkClassName}
        >
          <span>Creator</span>
        </a>
      </nav>
    </div>
  </footer>
);

export const PageShell = ({
  children,
  title,
  intro,
  titleIcon,
  breadcrumb,
}: {
  children: ReactNode;
  intro: string;
  /** Heading text. Omit when `titleIcon` already spells the full heading. */
  title?: string;
  titleIcon?: [before: string, after: string];
  breadcrumb: string;
}) => (
  <main className="overflow-x-hidden bg-sky-wash">
    <SiteNav />
    <section className="px-6 pt-40 pb-16 sm:px-10">
      <div className="mx-auto max-w-5xl">
        <nav aria-label="Breadcrumb" className="mb-7 text-sm text-ink-soft">
          <ol className="flex items-center justify-center gap-2">
            <li>
              <Link to="/" className="hover:text-ink">
                Home
              </Link>
            </li>
            <li aria-hidden="true">/</li>
            <li aria-current="page">{breadcrumb}</li>
          </ol>
        </nav>
        {titleIcon ? (
          <div className="text-center">
            <h1 className="font-display text-[clamp(2.6rem,6vw,4.8rem)] leading-[1.05] tracking-[-0.015em] text-ink">
              <span className="inline-flex flex-wrap items-center justify-center gap-x-4 gap-y-2">
                <span>{titleIcon[0]}</span>
                <img
                  src={appIcon}
                  alt=""
                  aria-hidden="true"
                  width={96}
                  height={96}
                  className="inline-block size-[1.05em] shrink-0 rounded-[28%] align-middle"
                />
                <span>{titleIcon[1]}</span>
              </span>
            </h1>
            <p className="mx-auto mt-5 max-w-xl text-lg leading-relaxed text-ink-soft">
              {intro}
            </p>
          </div>
        ) : (
          <>
            <h1 className="max-w-3xl font-display text-[clamp(2.6rem,6vw,4.8rem)] leading-[1.02] tracking-[-0.015em] text-ink">
              {title}
            </h1>
            <p className="mt-6 max-w-2xl text-lg leading-relaxed text-ink-soft">
              {intro}
            </p>
          </>
        )}
      </div>
    </section>
    <div className="px-6 pb-24 sm:px-10">
      <div className="mx-auto max-w-5xl">{children}</div>
    </div>
    <SiteFooter />
  </main>
);

export { GITHUB_URL, RELEASES_URL, SITE_URL } from "@/lib/site";
