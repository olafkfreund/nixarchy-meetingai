import { createFileRoute } from "@tanstack/react-router";
import {
  AudioLines,
  Feather,
  Gauge,
  Layers,
  Mic,
  PictureInPicture2,
  SlidersHorizontal,
} from "lucide-react";

import skyFooter from "@/assets/sky-footer.jpg";
import skyHero from "@/assets/sky-hero.jpg";

const GITHUB_URL = "https://github.com/parazeeknova/lennette";

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

const Nav = () => (
  <header className="absolute inset-x-0 top-0 z-30 px-4 pt-5 sm:px-8">
    <nav className="glass mx-auto flex h-16 max-w-5xl items-center justify-between glass-pill pr-2 pl-4">
      <a href="#top" className="relative z-10 flex items-center gap-2.5">
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
      </a>
      <div className="relative z-10 hidden items-center gap-8 text-sm font-medium text-ink-soft md:flex">
        <a href="#how" className="transition-colors hover:text-ink">
          How it works
        </a>
        <a href="#features" className="transition-colors hover:text-ink">
          Features
        </a>
        <a href="#privacy" className="transition-colors hover:text-ink">
          Privacy
        </a>
        <a href="#faq" className="transition-colors hover:text-ink">
          FAQ
        </a>
      </div>
      <a
        href="#download"
        className="glass btn-liquid h-12 glass-pill px-5 text-sm"
      >
        <span>Get lennette</span>
      </a>
    </nav>
  </header>
);

const Hero = () => (
  <section id="top" className="relative min-h-svh overflow-hidden">
    <img
      src={skyHero}
      alt=""
      width={1920}
      height={1088}
      className="absolute inset-0 h-full w-full animate-drift object-cover"
    />
    {/* blend clouds into the page below */}
    <div className="absolute inset-x-0 bottom-0 h-48 bg-linear-to-b from-transparent to-[oklch(0.93_0.035_232)]" />

    <div className="relative z-10 mx-auto flex min-h-svh max-w-6xl flex-col items-center px-6 pt-36 pb-20 text-center sm:px-10">
      <h1 className="max-w-5xl font-display text-[clamp(3.2rem,9vw,8.5rem)] leading-[0.92] tracking-[-0.02em] text-on-sky text-sky-shadow">
        Know exactly
        <br />
        what to say, <span className="text-on-sky-soft italic">live.</span>
      </h1>

      <p className="mt-8 max-w-xl text-lg leading-relaxed text-on-sky text-sky-shadow">
        Lennette listens to your meeting, follows the conversation as it
        happens, and quietly hands you the next thing worth saying.
      </p>
      <a id="download" href="#how" className="glass btn-liquid mt-8 glass-pill">
        <span>Download for desktop</span>
      </a>

      <div className="glass mt-14 w-full max-w-5xl p-2.5 sm:p-3">
        <img
          src="https://swr2.itssingularity.com/lennette/gmeet.png"
          alt="Lennette answering live inside a Google Meet call"
          width={1672}
          height={941}
          className="w-full rounded-[22px]"
        />
      </div>
    </div>
  </section>
);

const SectionHeading = ({ title, sub }: { title: string; sub?: string }) => (
  <div className="mx-auto max-w-3xl text-center">
    <h2 className="font-display text-[clamp(2.4rem,5vw,4rem)] leading-[1.02] tracking-[-0.015em] text-ink">
      {title}
    </h2>
    {sub && (
      <p className="mx-auto mt-5 max-w-xl text-lg text-ink-soft">{sub}</p>
    )}
  </div>
);

const HowItHelps = () => (
  <section id="how" className="relative isolate px-6 py-28 sm:px-10">
    <img
      src={skyHero}
      alt=""
      aria-hidden="true"
      loading="lazy"
      width={1920}
      height={1088}
      className="pointer-events-none absolute inset-x-0 top-1/2 -z-10 h-[70%] w-full -translate-y-1/2 feather-y object-cover object-bottom opacity-70"
    />
    <SectionHeading title="How lennette helps during the meeting" />
    <div className="mx-auto mt-16 grid max-w-5xl gap-6 md:grid-cols-2">
      <article className="glass flex flex-col p-3 glass-blue sm:p-4">
        <img
          src="https://swr2.itssingularity.com/lennette/lenette_transcribe.png"
          alt="Lennette listening with a live transcript and response"
          width={1672}
          height={941}
          className="relative z-10 w-full rounded-[22px]"
        />
        <div className="relative z-10 flex flex-1 flex-col p-5 pt-6 sm:p-6">
          <span className="font-display text-2xl leading-none text-on-sky-soft">
            Always there
          </span>
          <h3 className="mt-4 font-display text-3xl leading-tight text-on-sky">
            lennette listens in to the conversation
          </h3>
          <p className="mt-3 text-on-sky-soft">
            It picks up the context of your meeting in real time, so it can help
            when you need it.
          </p>
        </div>
      </article>
      <article className="glass flex flex-col p-3 sm:p-4">
        <img
          src="https://swr2.itssingularity.com/lennette/lennette_shortcut.png"
          alt="Lennette's overlay beside a call, opened with a keyboard shortcut"
          width={1448}
          height={1086}
          className="relative z-10 w-full rounded-[22px]"
        />
        <div className="relative z-10 flex flex-1 flex-col p-5 pt-6 sm:p-6">
          <h3 className="font-display text-3xl leading-tight text-ink">
            When you need help, lennette assists you instantly
          </h3>
          <p className="mt-3 text-ink-soft">
            Just press Ctrl Shift Backslash and the right words appear, only on
            your screen.
          </p>
        </div>
      </article>
    </div>
  </section>
);

const abilities = [
  {
    title: "Feather-light, ultra fast",
    body: "Written in Rust with GPUI. Low memory usage and instant responses, even on older machines.",
    span: true,
    art: (
      <div className="flex items-center gap-4">
        <span className="glass flex h-14 w-14 items-center justify-center glass-pill glass-blue">
          <Feather className="relative z-10 h-6 w-6 -rotate-12 text-on-sky" />
        </span>
        <span className="flex gap-2">
          <span className="kbd-glass">Rust</span>
          <span className="kbd-glass">GPUI</span>
        </span>
      </div>
    ),
  },
  {
    title: "Listen to both sides",
    body: "Microphone and system-audio transcription with separate speaker context.",
    art: (
      <div className="flex items-center">
        <span className="glass flex h-14 w-14 items-center justify-center glass-pill glass-blue">
          <Mic className="relative z-10 h-6 w-6 text-on-sky" />
        </span>
        <span className="glass -ml-3 flex h-14 w-14 items-center justify-center glass-pill">
          <AudioLines className="relative z-10 h-6 w-6 text-sky-deep" />
        </span>
      </div>
    ),
  },
  {
    title: "Answer with context",
    body: "Streamed suggestions using your prompt, reusable profiles, previous conversations, screenshots, and PDF/text/image attachments.",
    art: (
      <span className="glass relative flex h-14 w-14 items-center justify-center rounded-[20px] glass-blue">
        <Layers className="relative z-10 h-6 w-6 text-on-sky" />
      </span>
    ),
  },
  {
    title: "Make it yours",
    body: "General and Interview modes, editable answer/follow-up prompts, a cached live model selector, and No context when you want a clean start.",
    art: (
      <span className="glass flex h-14 w-14 rotate-3 items-center justify-center rounded-[20px]">
        <SlidersHorizontal className="relative z-10 h-6 w-6 text-sky-deep" />
      </span>
    ),
  },
  {
    title: "Choose when it responds",
    body: "On questions or after substantive pauses, with Fast / Balanced / Relaxed pacing.",
    art: (
      <div className="flex items-center gap-3">
        <span className="glass flex h-14 w-14 items-center justify-center glass-pill">
          <Gauge className="relative z-10 h-6 w-6 text-sky-deep" />
        </span>
        <span className="flex items-end gap-1.5" aria-hidden="true">
          <span className="h-2 w-2 rounded-full bg-sky-deep/40" />
          <span className="h-3.5 w-2 rounded-full bg-sky-deep/70" />
          <span className="h-5 w-2 rounded-full bg-sky-deep" />
        </span>
      </div>
    ),
  },
  {
    title: "Hold to dictate",
    body: "Use the input microphone or Ctrl+Shift+Space; release inserts a draft for review.",
    span: true,
    art: (
      <div className="flex flex-wrap items-center gap-4">
        <span className="glass flex h-14 w-14 items-center justify-center glass-pill glass-blue">
          <Mic className="relative z-10 h-6 w-6 text-on-sky" />
        </span>
        <span className="flex items-center gap-1.5 text-ink-soft">
          <span className="kbd-glass">Ctrl</span>
          <span aria-hidden="true">+</span>
          <span className="kbd-glass">Shift</span>
          <span aria-hidden="true">+</span>
          <span className="kbd-glass">Space</span>
        </span>
      </div>
    ),
  },
  {
    title: "Stay out of the way",
    body: "Compact overlay, transparent glass theme, global shortcuts, local history, and exports.",
    art: (
      <span className="glass flex h-14 w-14 items-center justify-center rounded-[20px] border border-sky/30">
        <PictureInPicture2 className="relative z-10 h-6 w-6 text-sky-deep" />
      </span>
    ),
  },
];

const Abilities = () => (
  <section id="features" className="relative px-6 py-28 sm:px-10">
    <SectionHeading
      title="What lennette can do"
      sub="Everything it handles before, during and after your meeting."
    />
    <div className="mx-auto mt-16 grid max-w-6xl gap-6 md:grid-cols-2 lg:grid-cols-3">
      {abilities.map((it) => (
        <article
          key={it.title}
          className={`glass flex flex-col p-8 ${it.span ? "lg:col-span-2" : ""}`}
        >
          <div className="relative z-10 flex flex-1 flex-col">
            <div className="mb-6">{it.art}</div>
            <h3 className="font-display text-2xl text-ink">{it.title}</h3>
            <p className="mt-3 text-ink-soft">{it.body}</p>
          </div>
        </article>
      ))}
    </div>
  </section>
);

const Privacy = () => {
  const items = [
    {
      title: "Doesn't join meetings",
      body: "It never joins your meetings, so there are no bots and no extra people on the guest list.",
      src: "https://swr2.itssingularity.com/lennette/lennette_guest.png",
      alt: "A meeting guest list with no lennette bot in it",
    },
    {
      title: "Invisible to screen share",
      body: "It never shows up in shared screens, recordings, or external meeting tools.",
      src: "https://swr2.itssingularity.com/lennette/notvisible_lennette.png",
      alt: "A shared screen with lennette absent from the recording",
    },
  ];
  return (
    <section id="privacy" className="relative px-6 py-28 sm:px-10">
      <SectionHeading title="Present for you. Absent for everyone else." />
      <div className="mx-auto mt-16 grid max-w-5xl gap-6 md:grid-cols-2">
        {items.map((it) => (
          <article key={it.title} className="glass flex flex-col p-3 sm:p-4">
            <img
              src={it.src}
              alt={it.alt}
              width={1672}
              height={941}
              loading="lazy"
              className="relative z-10 w-full rounded-[22px]"
            />
            <div className="relative z-10 flex flex-1 flex-col p-5 pt-6 sm:p-6">
              <h3 className="font-display text-3xl text-ink">{it.title}</h3>
              <p className="mt-3 text-lg text-ink-soft">{it.body}</p>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
};

const Transcription = () => {
  const lines = [
    { who: "Priya", text: "So where are we on the billing fix?" },
    { who: "Arjun", text: "Proration is done, refunds land next week." },
    { who: "You", text: "Great, then March still works for us." },
  ];
  return (
    <section className="relative isolate px-6 py-28 sm:px-10">
      <img
        src={skyHero}
        alt=""
        aria-hidden="true"
        loading="lazy"
        width={1920}
        height={1088}
        className="pointer-events-none absolute inset-x-0 top-1/2 -z-10 h-[70%] w-full -translate-y-1/2 feather-y object-cover object-bottom opacity-70"
      />
      <div className="mx-auto grid max-w-5xl gap-6 md:grid-cols-[1.3fr_1fr]">
        <article className="glass p-9">
          <div className="relative z-10">
            <h3 className="font-display text-3xl text-ink">
              Realtime transcription
            </h3>
            <p className="mt-3 text-ink-soft">
              Every word, speaker by speaker, as it is spoken.
            </p>
            <div className="mt-8 space-y-4">
              {lines.map((l) => (
                <div key={l.text} className="grid grid-cols-[4.5rem_1fr] gap-3">
                  <span className="text-sm font-semibold text-sky-deep">
                    {l.who}
                  </span>
                  <span className="text-ink">{l.text}</span>
                </div>
              ))}
            </div>
          </div>
        </article>
        <article className="glass flex flex-col justify-between p-9 glass-blue">
          <div className="relative z-10">
            <h3 className="font-display text-3xl text-on-sky">
              Hyprland support
            </h3>
            <p className="mt-3 text-on-sky-soft">
              Runs natively on Linux with Hyprland. The overlay respects your
              tiling layout and stays out of your way.
            </p>
          </div>
          <p className="relative z-10 mt-10 font-display text-5xl text-on-sky-soft">
            Wayland ready
          </p>
        </article>
      </div>
    </section>
  );
};

const faqs = [
  {
    q: "Does lennette join my call as a bot?",
    a: "No. It runs on your computer and listens through your device audio, so nobody else sees it.",
  },
  {
    q: "Can others see lennette when I share my screen?",
    a: "No. The window is excluded from screen sharing and recordings.",
  },
  {
    q: "Which meeting apps does it work with?",
    a: "Any app that plays audio on your computer: Zoom, Google Meet, Microsoft Teams, Slack huddles and more.",
  },
  {
    q: "How do I ask for help mid-meeting?",
    a: "Press Ctrl + Shift + \\ and a suggestion appears instantly.",
  },
  {
    q: "Can I share my notes?",
    a: "Yes. Notes, minutes and transcripts can be copied or shared with a link right after the call.",
  },
];

const Faq = () => (
  <section id="faq" className="relative px-6 py-28 sm:px-10">
    <div className="mx-auto grid max-w-5xl items-start gap-12 lg:grid-cols-[0.85fr_1.15fr] lg:gap-16">
      <div className="lg:sticky lg:top-24">
        <h2 className="font-display text-[clamp(2.4rem,5vw,4rem)] leading-[1.02] tracking-[-0.015em] text-ink">
          Questions, answered.
        </h2>
        <p className="mt-5 max-w-sm text-lg text-ink-soft">
          What happens on a call, what the people around you see, and what
          becomes of your notes.
        </p>
        <a
          href={GITHUB_URL}
          target="_blank"
          rel="noreferrer"
          className="mt-7 inline-flex items-center gap-2 text-sm font-medium text-sky-deep transition-colors hover:text-ink"
        >
          Ask something else
          <span aria-hidden="true">&rarr;</span>
        </a>
      </div>

      <div className="glass px-7 py-2 sm:px-9">
        <div className="relative z-10 divide-y divide-ink/15">
          {faqs.map((f) => (
            <details key={f.q} className="group pt-6 pb-6 first:pt-9 last:pb-9">
              <summary className="flex cursor-pointer list-none items-start justify-between gap-6 text-lg font-medium text-ink transition-colors hover:text-sky-deep sm:text-xl">
                <span>{f.q}</span>
                <span
                  aria-hidden="true"
                  className="mt-1 shrink-0 text-2xl leading-none text-ink-soft transition-transform duration-300 group-open:rotate-45"
                >
                  +
                </span>
              </summary>
              <p className="mt-3 max-w-xl text-ink-soft">{f.a}</p>
            </details>
          ))}
        </div>
      </div>
    </div>
  </section>
);

const Footer = () => (
  <footer className="relative overflow-hidden pt-24">
    <img
      src={skyFooter}
      alt=""
      loading="lazy"
      width={1920}
      height={1088}
      className="absolute inset-0 h-full w-full feather-top object-cover"
    />
    <img
      src="https://swr2.itssingularity.com/lennette/lennette_footer.png"
      alt=""
      aria-hidden="true"
      loading="lazy"
      width={1983}
      height={793}
      className="pointer-events-none absolute inset-x-0 bottom-0 w-full object-cover object-bottom"
    />
    <div className="relative z-10 mx-auto max-w-6xl px-4 pb-6 sm:px-8 sm:pb-8">
      <div className="glass overflow-hidden rounded-[40px] px-6 pt-14 sm:px-12 sm:pt-16">
        <div className="relative z-10 flex flex-col items-center gap-10 text-center lg:flex-row lg:items-center lg:justify-between lg:gap-14 lg:text-left">
          <h2 className="max-w-xl font-display text-[clamp(2.4rem,5.5vw,4.5rem)] leading-none text-ink">
            Walk into every meeting ready.
          </h2>
          <div className="flex shrink-0 flex-col items-center gap-6 sm:flex-row lg:flex-col lg:items-end">
            <img
              src="https://swr2.itssingularity.com/lennette/lennette_app_icon.png"
              alt="lennette app icon"
              width={96}
              height={96}
              className="h-24 w-24 rounded-3xl"
            />
            <div className="flex flex-col gap-3">
              <a
                href="#top"
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

        <div className="relative z-10 mt-14 h-px bg-linear-to-r from-transparent via-[oklch(1_0_0/0.85)] to-transparent" />

        <div className="relative z-10 flex flex-col items-center gap-5 py-7 text-sm text-ink-soft sm:flex-row sm:justify-between">
          <span>Know exactly what to say, live.</span>
          <nav className="flex flex-wrap justify-center gap-x-7 gap-y-2">
            <a href="#how" className="transition-colors hover:text-ink">
              How it works
            </a>
            <a href="#features" className="transition-colors hover:text-ink">
              Features
            </a>
            <a href="#privacy" className="transition-colors hover:text-ink">
              Privacy
            </a>
            <a href="#faq" className="transition-colors hover:text-ink">
              FAQ
            </a>
            <a
              href={GITHUB_URL}
              target="_blank"
              rel="noreferrer"
              className="transition-colors hover:text-ink"
            >
              GitHub
            </a>
          </nav>
          <span>© 2026 lennette</span>
        </div>

        <p
          aria-hidden="true"
          className="relative z-10 pt-2 pb-8 text-center font-display text-[clamp(4.5rem,18vw,15rem)] leading-[1.1] tracking-[0.02em] text-on-sky select-none text-sky-shadow"
        >
          lennette
        </p>
      </div>
    </div>
  </footer>
);

const Index = () => (
  <main className="overflow-x-hidden bg-sky-wash">
    <Nav />
    <Hero />
    <HowItHelps />
    <Abilities />
    <Privacy />
    <Transcription />
    <Faq />
    <Footer />
  </main>
);

export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      { title: "Lennette - Know exactly what to say, live." },
      {
        name: "description",
        content:
          "Lennette listens to your meetings in real time and suggests exactly what to say next. No bots, invisible to screen share.",
      },
      {
        property: "og:title",
        content: "Lennette - Know exactly what to say, live.",
      },
      {
        property: "og:description",
        content:
          "A live meeting assistant that tells you what to say next, quietly and invisibly.",
      },
      { property: "og:type", content: "website" },
      { name: "twitter:card", content: "summary_large_image" },
    ],
  }),
  component: Index,
});
