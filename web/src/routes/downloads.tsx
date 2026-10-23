import { createFileRoute } from "@tanstack/react-router";

import { Command } from "@/components/command";
import { GITHUB_URL, PageShell, RELEASES_URL } from "@/components/page-shell";

interface Platform {
  name: string;
  requirement: string;
  body: string;
  status?: string;
  action?: { label: string; href: string; external: boolean };
  available: boolean;
}

const platforms: Platform[] = [
  {
    name: "Linux",
    requirement: "x86_64",
    body: "For private screen-share protection on Hyprland, install the patched compositor first.",
    status: "Wayland and X11",
    action: { label: "Hyprland setup", href: "/tutorial", external: false },
    available: true,
  },
  {
    name: "Windows",
    requirement: "x86_64",
    body: "Launches privately by default using WDA_EXCLUDEFROMCAPTURE. No compositor patch needed.",
    status: "Windows 10 or later",
    action: { label: "Build instructions", href: GITHUB_URL, external: true },
    available: true,
  },
  {
    name: "macOS",
    requirement: "Coming soon",
    body: "Capture exclusion needs a macOS screen-capture path Lennette does not implement yet.",
    available: false,
  },
];

const DownloadsPage = () => (
  <PageShell
    title="Download Lennette"
    intro="Lennette runs on your own machine and reads your device audio locally, so the meeting never receives a bot or an upload."
  >
    <div className="grid gap-5 md:grid-cols-3">
      {platforms.map((platform) => (
        <article
          key={platform.name}
          className="glass flex flex-col rounded-3xl p-6"
        >
          <div className="relative z-10 flex flex-1 flex-col">
            <div className="flex items-baseline justify-between gap-2">
              <h2 className="font-display text-2xl text-ink">
                {platform.name}
              </h2>
              <span className="text-xs text-ink-soft">
                {platform.requirement}
              </span>
            </div>
            {platform.status ? (
              <p className="mt-1 text-xs text-ink-soft/80">{platform.status}</p>
            ) : null}
            <p className="mt-4 flex-1 leading-relaxed text-ink-soft">
              {platform.body}
            </p>
            {platform.action ? (
              <a
                href={platform.action.href}
                target={platform.action.external ? "_blank" : undefined}
                rel={platform.action.external ? "noreferrer" : undefined}
                className="glass btn-liquid mt-6 h-11 glass-pill text-sm glass-blue text-on-sky"
              >
                <span>{platform.action.label}</span>
              </a>
            ) : (
              <span className="mt-6 flex h-11 items-center justify-center rounded-full border border-ink/15 text-sm font-medium text-ink-soft">
                Coming soon
              </span>
            )}
          </div>
        </article>
      ))}
    </div>

    <section className="mt-14">
      <h2 className="font-display text-2xl text-ink">Build from source</h2>
      <p className="mt-3 max-w-2xl leading-relaxed text-ink-soft">
        No release binaries are published yet, so this is how to get a running
        copy today. You need Rust 1.99 plus the usual Wayland, Vulkan and audio
        development packages.
      </p>
      <div className="mt-6 flex flex-col gap-3">
        <Command label="clone and build">
          {`git clone ${GITHUB_URL}.git\ncd lennette\ndevenv shell -- just install`}
        </Command>
        <p className="max-w-2xl text-sm leading-relaxed text-ink-soft">
          <code className="rounded bg-white/45 px-1.5 py-0.5 text-ink">
            just install
          </code>{" "}
          validates formatting, lints and type-checks, then builds an optimised
          binary into{" "}
          <code className="rounded bg-white/45 px-1.5 py-0.5 text-ink">
            ~/.local/bin
          </code>
          .
        </p>
      </div>
    </section>

    <div className="mt-12">
      <a
        href={RELEASES_URL}
        target="_blank"
        rel="noreferrer"
        className="glass btn-liquid glass-pill"
      >
        <span>Watch the releases page</span>
      </a>
    </div>
  </PageShell>
);

export const Route = createFileRoute("/downloads")({
  head: () => ({
    meta: [
      { title: "Downloads - Lennette" },
      {
        name: "description",
        content:
          "Download Lennette for Linux and Windows. macOS support is coming soon.",
      },
    ],
  }),
  component: DownloadsPage,
});
