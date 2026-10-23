import { createFileRoute } from "@tanstack/react-router";

import { Command } from "@/components/command";
import { GITHUB_URL, PageShell, RELEASES_URL } from "@/components/page-shell";

interface Platform {
  name: string;
  requirement: string;
  body: string;
  note: string;
  action?: { label: string; href: string; external: boolean };
}

const platforms: Platform[] = [
  {
    name: "Linux",
    requirement: "x86_64 · Hyprland or any Wayland/X11 desktop",
    body: "For private screen-share protection on Hyprland, install the patched compositor first.",
    note: "Build from source today; binaries will land on the releases page.",
    action: {
      label: "Hyprland setup guide",
      href: "/tutorial",
      external: false,
    },
  },
  {
    name: "Windows",
    requirement: "x86_64 · Windows 10 or later",
    body: "Launches privately by default using WDA_EXCLUDEFROMCAPTURE. No compositor patch and no environment variable needed.",
    note: "Build from source today; binaries will land on the releases page.",
    action: {
      label: "Build instructions",
      href: GITHUB_URL,
      external: true,
    },
  },
  {
    name: "macOS",
    requirement: "Not supported",
    body: "There is no macOS build. Capture exclusion depends on an OS interface Lennette does not implement for macOS, so nothing is published for it.",
    note: "Will be reconsidered if that changes.",
  },
];

const DownloadsPage = () => (
  <PageShell
    title="Download Lennette"
    intro="Lennette ships for Linux and Windows. It listens to your device audio locally, so the meeting never receives a bot or an upload."
  >
    <div className="flex flex-col gap-5">
      {platforms.map((platform) => (
        <article key={platform.name} className="glass rounded-3xl p-6 sm:p-8">
          <div className="relative z-10 flex flex-col gap-6 sm:flex-row sm:items-start sm:justify-between">
            <div className="max-w-2xl">
              <div className="flex flex-wrap items-baseline gap-3">
                <h2 className="font-display text-2xl text-ink">
                  {platform.name}
                </h2>
                <span className="text-sm text-ink-soft">
                  {platform.requirement}
                </span>
              </div>
              <p className="mt-3 leading-relaxed text-ink-soft">
                {platform.body}
              </p>
              <p className="mt-2 text-sm text-ink-soft/80">{platform.note}</p>
            </div>
            {platform.action ? (
              <div className="shrink-0">
                <a
                  href={platform.action.href}
                  target={platform.action.external ? "_blank" : undefined}
                  rel={platform.action.external ? "noreferrer" : undefined}
                  className="glass btn-liquid glass-pill glass-blue text-on-sky"
                >
                  <span>{platform.action.label}</span>
                </a>
              </div>
            ) : null}
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
          <code className="rounded bg-ink/8 px-1.5 py-0.5 text-ink">
            just install
          </code>{" "}
          validates formatting, lints and type-checks, then builds an optimised
          binary into{" "}
          <code className="rounded bg-ink/8 px-1.5 py-0.5 text-ink">
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
          "Download Lennette for Linux and Windows, or build it from source. macOS is not supported yet.",
      },
    ],
  }),
  component: DownloadsPage,
});
