import { Link, createFileRoute } from "@tanstack/react-router";

import { GITHUB_URL, PageShell } from "@/components/page-shell";

interface Plan {
  name: string;
  price: string;
  cadence: string;
  summary: string;
  features: string[];
  /** `to` navigates inside the app; `href` opens externally. */
  cta: { label: string; to?: "/downloads" | "/tutorial"; href?: string };
  emphasis: boolean;
}

const plans: Plan[] = [
  {
    name: "Bring your own key",
    price: "$0",
    cadence: "forever",
    summary:
      "Run the app on your own machine with your own model credentials. Transcription and answers go straight to the provider you pick.",
    features: [
      "Full app, no feature limits",
      "Any supported provider and model",
      "Local transcription and history",
      "Private overlay on Hyprland",
      "Windows and Linux builds",
    ],
    cta: {
      label: "Download Lennette",
      to: "/downloads",
    },
    emphasis: false,
  },
  {
    name: "Managed",
    price: "$35",
    cadence: "per month",
    summary:
      "Everything in the free plan, with keys, model routing and quotas handled for you. Nothing to set up beyond signing in.",
    features: [
      "2,000 AI responses",
      "Unlimited follow-up questions",
      "30+ hours of realtime transcription",
      "Speaker diarization",
      "Unlimited devices",
      "No API key to create or rotate",
    ],
    cta: {
      label: "Request an invite",
      href: GITHUB_URL,
    },
    emphasis: true,
  },
];

const PlanCard = ({ plan }: { plan: Plan }) => {
  const ctaClass = [
    "glass glass-pill btn-liquid mt-8 w-full text-[0.95rem]",
    plan.emphasis
      ? "glass-blue text-on-sky hover:bg-[oklch(0.4_0.14_252/0.25)]"
      : "",
  ]
    .filter(Boolean)
    .join(" ");

  const ctaLabel = <span>{plan.cta.label}</span>;

  return (
    <article
      className={`flex flex-col rounded-3xl p-7 sm:p-8 ${
        plan.emphasis ? "glass glass-blue text-on-sky" : "glass text-ink"
      }`}
    >
      <div className="relative z-10 flex flex-1 flex-col">
        <h2
          className={`font-display text-2xl ${
            plan.emphasis ? "text-on-sky" : "text-ink"
          }`}
        >
          {plan.name}
        </h2>

        <p className="mt-6 flex items-baseline gap-2">
          <span
            className={`font-display text-5xl leading-none ${
              plan.emphasis ? "text-on-sky" : "text-ink"
            }`}
          >
            {plan.price}
          </span>
          <span
            className={plan.emphasis ? "text-on-sky-soft" : "text-ink-soft"}
          >
            {plan.cadence}
          </span>
        </p>

        <p
          className={`mt-4 min-h-22 leading-relaxed ${
            plan.emphasis ? "text-on-sky-soft" : "text-ink-soft"
          }`}
        >
          {plan.summary}
        </p>

        <ul
          className={`mt-6 flex flex-1 flex-col gap-2.5 text-[0.95rem] ${
            plan.emphasis ? "text-on-sky-soft" : "text-ink-soft"
          }`}
        >
          {plan.features.map((feature) => (
            <li key={feature} className="flex items-start gap-2.5">
              <span
                aria-hidden="true"
                className={`mt-[0.45rem] size-1.5 shrink-0 rounded-full ${
                  plan.emphasis ? "bg-on-sky/70" : "bg-sky-deep/60"
                }`}
              />
              <span>{feature}</span>
            </li>
          ))}
        </ul>

        {plan.cta.to ? (
          <Link to={plan.cta.to} className={ctaClass}>
            {ctaLabel}
          </Link>
        ) : (
          <a
            href={plan.cta.href}
            target="_blank"
            rel="noreferrer"
            className={ctaClass}
          >
            {ctaLabel}
          </a>
        )}
      </div>
    </article>
  );
};

const PricingPage = () => (
  <PageShell
    titleIcon={["Start", "for free."]}
    intro="The app costs nothing either way. Bring your own model key, or let us host it for $35 a month."
  >
    <div className="grid gap-6 md:grid-cols-2">
      {plans.map((plan) => (
        <PlanCard key={plan.name} plan={plan} />
      ))}
    </div>

    <section className="mt-16">
      <h2 className="font-display text-2xl text-ink">
        Questions about pricing
      </h2>
      <div className="mt-6 flex flex-col gap-6">
        <div>
          <h3 className="font-medium text-ink">
            Do I need an account for the free plan?
          </h3>
          <p className="mt-1.5 max-w-2xl leading-relaxed text-ink-soft">
            No. You build or download the app, add your own provider key, and
            everything runs locally. Nothing is metered by us.
          </p>
        </div>
        <div>
          <h3 className="font-medium text-ink">
            What does managed actually cover?
          </h3>
          <p className="mt-1.5 max-w-2xl leading-relaxed text-ink-soft">
            Speech-to-text and suggestions through our relay, so you do not need
            a separate provider subscription. Follow-up questions are unlimited
            across unlimited devices, with 2,000 AI responses and 30 hours of
            realtime transcription a month. Your audio still comes from your own
            device, and the overlay stays local.
          </p>
        </div>
        <div>
          <h3 className="font-medium text-ink">
            Can I switch between the two?
          </h3>
          <p className="mt-1.5 max-w-2xl leading-relaxed text-ink-soft">
            Yes. The model selector lets you point the app at your own key or
            the managed route at any time.
          </p>
        </div>
      </div>
    </section>
  </PageShell>
);

export const Route = createFileRoute("/pricing")({
  head: () => ({
    meta: [
      { title: "Pricing - Lennette" },
      {
        name: "description",
        content:
          "Lennette is free with your own model API key, or $35 a month managed. The app itself costs nothing.",
      },
    ],
  }),
  component: PricingPage,
});
