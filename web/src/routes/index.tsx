import { createFileRoute } from "@tanstack/react-router";
import {
  AudioLines,
  Component,
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

// Rust crab mark, from Simple Icons (CC0).
const RustMark = ({ className = "" }: { className?: string }) => (
  <svg
    viewBox="0 0 24 24"
    aria-hidden="true"
    className={className}
    fill="currentColor"
  >
    <path d="M23.8346 11.7033l-1.0073-.6236a13.7268 13.7268 0 00-.0283-.2936l.8656-.8069a.3483.3483 0 00-.1154-.578l-1.1066-.414a8.4958 8.4958 0 00-.087-.2856l.6904-.9587a.3462.3462 0 00-.2257-.5446l-1.1663-.1894a9.3574 9.3574 0 00-.1407-.2622l.49-1.0761a.3437.3437 0 00-.0274-.3361.3486.3486 0 00-.3006-.154l-1.1845.0416a6.7444 6.7444 0 00-.1873-.2268l.2723-1.153a.3472.3472 0 00-.417-.4172l-1.1532.2724a14.0183 14.0183 0 00-.2278-.1873l.0415-1.1845a.3442.3442 0 00-.49-.328l-1.076.491c-.0872-.0476-.1742-.0952-.2623-.1407l-.1903-1.1673A.3483.3483 0 0016.256.955l-.9597.6905a8.4867 8.4867 0 00-.2855-.086l-.414-1.1066a.3483.3483 0 00-.5781-.1154l-.8069.8666a9.2936 9.2936 0 00-.2936-.0284L12.2946.1683a.3462.3462 0 00-.5892 0l-.6236 1.0073a13.7383 13.7383 0 00-.2936.0284L9.9803.3374a.3462.3462 0 00-.578.1154l-.4141 1.1065c-.0962.0274-.1903.0567-.2855.086L7.744.955a.3483.3483 0 00-.5447.2258L7.009 2.348a9.3574 9.3574 0 00-.2622.1407l-1.0762-.491a.3462.3462 0 00-.49.328l.0416 1.1845a7.9826 7.9826 0 00-.2278.1873L3.8413 3.425a.3472.3472 0 00-.4171.4171l.2713 1.1531c-.0628.075-.1255.1509-.1863.2268l-1.1845-.0415a.3462.3462 0 00-.328.49l.491 1.0761a9.167 9.167 0 00-.1407.2622l-1.1662.1894a.3483.3483 0 00-.2258.5446l.6904.9587a13.303 13.303 0 00-.087.2855l-1.1065.414a.3483.3483 0 00-.1155.5781l.8656.807a9.2936 9.2936 0 00-.0283.2935l-1.0073.6236a.3442.3442 0 000 .5892l1.0073.6236c.008.0982.0182.1964.0283.2936l-.8656.8079a.3462.3462 0 00.1155.578l1.1065.4141c.0273.0962.0567.1914.087.2855l-.6904.9587a.3452.3452 0 00.2268.5447l1.1662.1893c.0456.088.0922.1751.1408.2622l-.491 1.0762a.3462.3462 0 00.328.49l1.1834-.0415c.0618.0769.1235.1528.1873.2277l-.2713 1.1541a.3462.3462 0 00.4171.4161l1.153-.2713c.075.0638.151.1255.2279.1863l-.0415 1.1845a.3442.3442 0 00.49.327l1.0761-.49c.087.0486.1741.0951.2622.1407l.1903 1.1662a.3483.3483 0 00.5447.2268l.9587-.6904a9.299 9.299 0 00.2855.087l.414 1.1066a.3452.3452 0 00.5781.1154l.8079-.8656c.0972.0111.1954.0203.2936.0294l.6236 1.0073a.3472.3472 0 00.5892 0l.6236-1.0073c.0982-.0091.1964-.0183.2936-.0294l.8069.8656a.3483.3483 0 00.578-.1154l.4141-1.1066a8.4626 8.4626 0 00.2855-.087l.9587.6904a.3452.3452 0 00.5447-.2268l.1903-1.1662c.088-.0456.1751-.0931.2622-.1407l1.0762.49a.3472.3472 0 00.49-.327l-.0415-1.1845a6.7267 6.7267 0 00.2267-.1863l1.1531.2713a.3472.3472 0 00.4171-.416l-.2713-1.1542c.0628-.0749.1255-.1508.1863-.2278l1.1845.0415a.3442.3442 0 00.328-.49l-.49-1.076c.0475-.0872.0951-.1742.1407-.2623l1.1662-.1893a.3483.3483 0 00.2258-.5447l-.6904-.9587.087-.2855 1.1066-.414a.3462.3462 0 00.1154-.5781l-.8656-.8079c.0101-.0972.0202-.1954.0283-.2936l1.0073-.6236a.3442.3442 0 000-.5892zm-6.7413 8.3551a.7138.7138 0 01.2986-1.396.714.714 0 11-.2997 1.396zm-.3422-2.3142a.649.649 0 00-.7715.5l-.3573 1.6685c-1.1035.501-2.3285.7795-3.6193.7795a8.7368 8.7368 0 01-3.6951-.814l-.3574-1.6684a.648.648 0 00-.7714-.499l-1.473.3158a8.7216 8.7216 0 01-.7613-.898h7.1676c.081 0 .1356-.0141.1356-.088v-2.536c0-.074-.0536-.0881-.1356-.0881h-2.0966v-1.6077h2.2677c.2065 0 1.1065.0587 1.394 1.2088.0901.3533.2875 1.5044.4232 1.8729.1346.413.6833 1.2381 1.2685 1.2381h3.5716a.7492.7492 0 00.1296-.0131 8.7874 8.7874 0 01-.8119.9526zM6.8369 20.024a.714.714 0 11-.2997-1.396.714.714 0 01.2997 1.396zM4.1177 8.9972a.7137.7137 0 11-1.304.5791.7137.7137 0 011.304-.579zm-.8352 1.9813l1.5347-.6824a.65.65 0 00.33-.8585l-.3158-.7147h1.2432v5.6025H3.5669a8.7753 8.7753 0 01-.2834-3.348zm6.7343-.5437V8.7836h2.9601c.153 0 1.0792.1772 1.0792.8697 0 .575-.7107.7815-1.2948.7815zm10.7574 1.4862c0 .2187-.008.4363-.0243.651h-.9c-.09 0-.1265.0586-.1265.1477v.413c0 .973-.5487 1.1846-1.0296 1.2382-.4576.0517-.9648-.1913-1.0275-.4717-.2704-1.5186-.7198-1.8436-1.4305-2.4034.8817-.5599 1.799-1.386 1.799-2.4915 0-1.1936-.819-1.9458-1.3769-2.3153-.7825-.5163-1.6491-.6195-1.883-.6195H5.4682a8.7651 8.7651 0 014.907-2.7699l1.0974 1.151a.648.648 0 00.9182.0213l1.227-1.1743a8.7753 8.7753 0 016.0044 4.2762l-.8403 1.8982a.652.652 0 00.33.8585l1.6178.7188c.0283.2875.0425.577.0425.8717zm-9.3006-9.5993a.7128.7128 0 11.984 1.0316.7137.7137 0 01-.984-1.0316zm8.3389 6.71a.7107.7107 0 01.9395-.3625.7137.7137 0 11-.9405.3635z" />
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
      <div className="flex items-center gap-3">
        <span
          className="glass flex aspect-square h-14 w-14 shrink-0 items-center justify-center rounded-full glass-blue"
          title="Rust"
        >
          <RustMark className="relative z-10 h-7 w-7 text-on-sky" />
          <span className="sr-only">Rust</span>
        </span>
        <span
          className="glass flex aspect-square h-14 w-14 shrink-0 items-center justify-center rounded-full"
          title="GPUI"
        >
          <Component className="relative z-10 h-6 w-6 text-sky-deep" />
          <span className="sr-only">GPUI</span>
        </span>
      </div>
    ),
  },
  {
    title: "Listen to both sides",
    body: "Microphone and system-audio transcription with separate speaker context.",
    art: (
      <div className="flex items-center">
        <span className="glass flex aspect-square h-14 w-14 shrink-0 items-center justify-center glass-pill glass-blue">
          <Mic className="relative z-10 h-6 w-6 text-on-sky" />
        </span>
        <span className="glass -ml-3 flex aspect-square h-14 w-14 shrink-0 items-center justify-center glass-pill">
          <AudioLines className="relative z-10 h-6 w-6 text-sky-deep" />
        </span>
      </div>
    ),
  },
  {
    title: "Answer with context",
    body: "Streamed suggestions using your prompt, reusable profiles, previous conversations, screenshots, and PDF/text/image attachments.",
    art: (
      <span className="glass relative flex aspect-square h-14 w-14 shrink-0 items-center justify-center rounded-[20px] glass-blue">
        <Layers className="relative z-10 h-6 w-6 text-on-sky" />
      </span>
    ),
  },
  {
    title: "Make it yours",
    body: "General and Interview modes, editable answer/follow-up prompts, a cached live model selector, and No context when you want a clean start.",
    art: (
      <span className="glass flex aspect-square h-14 w-14 shrink-0 rotate-3 items-center justify-center rounded-[20px]">
        <SlidersHorizontal className="relative z-10 h-6 w-6 text-sky-deep" />
      </span>
    ),
  },
  {
    title: "Choose when it responds",
    body: "On questions or after substantive pauses, with Fast / Balanced / Relaxed pacing.",
    art: (
      <div className="flex items-center gap-3">
        <span className="glass flex aspect-square h-14 w-14 shrink-0 items-center justify-center rounded-full glass-blue">
          <Gauge className="relative z-10 h-6 w-6 text-on-sky" />
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
        <span className="glass flex aspect-square h-14 w-14 shrink-0 items-center justify-center glass-pill glass-blue">
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
      <span className="glass flex aspect-square h-14 w-14 shrink-0 items-center justify-center rounded-[20px] border border-sky/30">
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
        className="pointer-events-none absolute inset-x-0 top-1/2 -z-10 h-[92%] w-full -translate-y-1/2 feather-y object-cover object-bottom opacity-70"
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
    <div className="mx-auto max-w-5xl">
      <h2 className="font-display text-[clamp(2.4rem,5vw,4rem)] leading-[1.02] tracking-[-0.015em] text-ink">
        Questions, answered.
      </h2>
      <p className="mt-5 max-w-xl text-lg text-ink-soft">
        What happens on a call, what the people around you see, and what becomes
        of your notes.
      </p>

      <div className="mt-14 border-t border-ink/10">
        {faqs.map((f, index) => (
          <details
            key={f.q}
            className="group border-b border-ink/10"
            open={index === 0}
          >
            <summary className="flex cursor-pointer list-none items-center justify-between gap-8 py-5 text-left text-lg font-medium text-ink transition-colors hover:text-sky-deep">
              <span>{f.q}</span>
              <svg
                aria-hidden="true"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
                className="size-5 shrink-0 text-ink-soft transition-transform duration-300 group-open:rotate-180"
              >
                <path d="m6 9 6 6 6-6" />
              </svg>
            </summary>
            <p className="max-w-2xl pb-6 text-ink-soft">{f.a}</p>
          </details>
        ))}
      </div>

      <a
        href={GITHUB_URL}
        target="_blank"
        rel="noreferrer"
        className="mt-10 inline-flex items-center gap-2 text-sm font-medium text-sky-deep transition-colors hover:text-ink"
      >
        Ask something else
        <span aria-hidden="true">&rarr;</span>
      </a>
    </div>
  </section>
);

const Footer = () => (
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
      src="https://swr2.itssingularity.com/lennette/lennette_footer_with_text.png"
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
            src="https://swr2.itssingularity.com/lennette/lennette_app_icon.png"
            alt="lennette app icon"
            width={96}
            height={96}
            className="h-20 w-20 rounded-3xl"
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

      <div className="mt-14 flex flex-wrap items-center justify-center gap-x-8 gap-y-3 text-sm text-ink-soft">
        <a href="/pricing" className="transition-colors hover:text-ink">
          Pricing
        </a>
        <a href="/downloads" className="transition-colors hover:text-ink">
          Downloads
        </a>
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
