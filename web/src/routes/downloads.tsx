import { createFileRoute } from "@tanstack/react-router";

import { Command } from "@/components/command";
import { GITHUB_URL, PageShell, RELEASES_URL } from "@/components/page-shell";
import { platformMarks } from "@/components/platform-marks";
import type { Platform } from "@/components/platform-marks";

interface Release {
  /** Asset filename published by the release workflow. */
  asset: string;
  label: string;
  note: string;
}

interface Card {
  platform: Platform;
  name: string;
  requirement: string;
  status: string;
  body: string;
  caution?: string;
  releases?: Release[];
  pending?: boolean;
  secondary?: { label: string; href: string; external: boolean };
}

// GitHub serves the newest release's assets from this stable path, so the links
// keep working without hardcoding a tag.
const assetUrl = (asset: string) => `${RELEASES_URL}/latest/download/${asset}`;

const cards: Card[] = [
  {
    platform: "linux",
    name: "Linux",
    requirement: "x86_64",
    status: "Wayland and X11",
    body: "Runs on any modern Linux desktop. Private screen-share protection is only available on the patched Hyprland compositor described in the setup guide.",
    caution:
      "Screen-share protection currently supports Hyprland only, from a pinned compositor revision. On GNOME, KDE, Sway or an unpatched Hyprland, the overlay is visible in shares.",
    releases: [
      {
        asset: "lennette-linux-x86_64.tar.gz",
        label: "Download .tar.gz",
        note: "Extract and run lennette",
      },
    ],
    secondary: { label: "Hyprland setup", href: "/tutorial", external: false },
  },
  {
    platform: "windows",
    name: "Windows",
    requirement: "x86_64",
    status: "Windows 10 or later",
    body: "Portable build, no installer. Launches privately by default using WDA_EXCLUDEFROMCAPTURE, so no compositor patch is needed.",
    releases: [
      {
        asset: "lennette-windows-x86_64.exe",
        label: "Download .exe",
        note: "Portable, no install",
      },
    ],
  },
  {
    platform: "macos",
    name: "macOS",
    requirement: "Coming soon",
    status: "Not yet supported",
    body: "Capture exclusion needs a macOS screen-capture path Lennette does not implement yet, so no build is published.",
    pending: true,
  },
];

const PlatformIcon = ({ platform }: { platform: Platform }) => {
  const Mark = platformMarks[platform];
  return (
    <span className="glass flex size-12 items-center justify-center rounded-2xl glass-blue">
      <Mark className="relative z-10 size-6 text-on-sky" />
    </span>
  );
};

const DownloadsPage = () => (
  <PageShell
    titleIcon={["Download", "lennette."]}
    intro="Lennette runs on your own machine and reads your device audio locally, so the meeting never receives a bot or an upload."
  >
    <div className="grid gap-5 md:grid-cols-3">
      {cards.map((card) => (
        <article
          key={card.name}
          className="glass flex flex-col rounded-3xl p-6"
        >
          <div className="relative z-10 flex flex-1 flex-col">
            <div className="flex items-start justify-between gap-3">
              <div>
                <h2 className="font-display text-2xl text-ink">{card.name}</h2>
                <p className="mt-1 text-xs text-ink-soft/80">
                  {card.requirement}
                </p>
              </div>
              <PlatformIcon platform={card.platform} />
            </div>

            <p className="mt-4 text-xs font-medium text-sky-deep">
              {card.status}
            </p>
            <p className="mt-2 flex-1 text-sm leading-relaxed text-ink-soft">
              {card.body}
            </p>

            {card.caution ? (
              <div className="mt-4 rounded-xl border border-ink/10 bg-white/45 p-3">
                <p className="text-xs leading-relaxed text-ink-soft">
                  {card.caution}
                </p>
                <a
                  href={`${GITHUB_URL}/issues/new?title=${encodeURIComponent("Screen-share protection on ")}&body=${encodeURIComponent("Desktop environment / compositor:\nVersion:\n\nWhat happens when you share your screen with Lennette running:\n")}`}
                  target="_blank"
                  rel="noreferrer"
                  className="mt-2 inline-block text-xs font-medium text-sky-deep transition-colors hover:text-ink"
                >
                  Request support for your desktop →
                </a>
              </div>
            ) : null}

            <div className="mt-5 flex flex-col gap-2">
              {card.releases?.map((release) => (
                <a
                  key={release.asset}
                  href={assetUrl(release.asset)}
                  className="glass btn-liquid h-11 glass-pill text-sm"
                >
                  <span>{release.label}</span>
                </a>
              ))}
              {card.secondary ? (
                <a
                  href={card.secondary.href}
                  {...(card.secondary.external
                    ? { target: "_blank", rel: "noreferrer" }
                    : {})}
                  className="text-center text-sm font-medium text-sky-deep transition-colors hover:text-ink"
                >
                  {card.secondary.label} →
                </a>
              ) : null}
              {card.pending ? (
                <span className="flex h-11 items-center justify-center rounded-full border border-ink/15 text-sm font-medium text-ink-soft">
                  Coming soon
                </span>
              ) : null}
            </div>
          </div>
        </article>
      ))}
    </div>

    <p className="mt-5 text-sm text-ink-soft">
      Every download is published from CI with a matching{" "}
      <code className="rounded bg-white/45 px-1.5 py-0.5 text-ink">
        SHA256SUMS
      </code>{" "}
      manifest. If a button returns 404, no release has been published yet —
      build from source below.
    </p>

    <section className="mt-14">
      <h2 className="font-display text-2xl text-ink">Build from source</h2>
      <p className="mt-3 max-w-2xl leading-relaxed text-ink-soft">
        You need Rust 1.99 plus the usual Wayland, Vulkan and audio development
        packages.
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
        <span>All releases</span>
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
          "Download Lennette for Linux and Windows. Linux screen-share protection requires patched Hyprland. macOS is coming soon.",
      },
    ],
  }),
  component: DownloadsPage,
});
