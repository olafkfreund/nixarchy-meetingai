import { Link, createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { PageShell } from "@/components/page-shell";
import { WaitlistDialog } from "@/components/waitlist-dialog";
import { PRICING_FAQS } from "@/lib/faqs";
import { pageHead } from "@/lib/seo";
import { MANAGED_PLAN } from "@/lib/site";

const MANAGED_MONTHLY_PRICE = MANAGED_PLAN.price;

interface Plan {
  name: string;
  price: string;
  cadence: string;
  summary: string;
  note: string;
  features: string[];
  cta: {
    label: string;
    to?: "/downloads" | "/tutorial";
    href?: string;
    waitlist?: boolean;
  };
  emphasis: boolean;
}

const plans: Plan[] = [
  {
    name: "Free · Bring your own keys",
    price: "$0",
    cadence: "forever",
    summary:
      "Use Lennette with your own AI and transcription API keys. Choose your providers and pay them directly for what you use.",
    note: "Lennette is free. Your providers may charge for API usage.",
    features: [
      "Full desktop app",
      "Your choice of supported AI models",
      "Live transcription with your Deepgram key",
      "Chat and meeting history stored locally",
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
    price: MANAGED_MONTHLY_PRICE,
    cadence: "per month",
    summary:
      "The full Lennette experience, with AI answers and live transcription included in one monthly plan. No provider keys to manage.",
    note: "Planned pricing in USD. Managed access is coming soon.",
    features: [
      "2,000 AI responses per month",
      "30 hours of live transcription per month",
      "Follow-up answers use your response allowance",
      "Speaker labels in your transcripts",
      "Use your allowance across your devices",
      "AI and transcription API costs included",
    ],
    cta: {
      label: "Join the waitlist",
      waitlist: true,
    },
    emphasis: true,
  },
];

const PlanCard = ({
  plan,
  onWaitlist,
}: {
  plan: Plan;
  onWaitlist: () => void;
}) => {
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

        <p className="mt-6 flex flex-wrap items-baseline gap-x-2 gap-y-2">
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

        <p
          className={`mt-6 text-sm leading-relaxed ${
            plan.emphasis ? "text-on-sky-soft" : "text-ink-soft"
          }`}
        >
          {plan.note}
        </p>

        {plan.cta.waitlist ? (
          <button type="button" onClick={onWaitlist} className={ctaClass}>
            {ctaLabel}
          </button>
        ) : null}
        {!plan.cta.waitlist && plan.cta.to ? (
          <Link to={plan.cta.to} className={ctaClass}>
            {ctaLabel}
          </Link>
        ) : null}
        {!plan.cta.waitlist && !plan.cta.to ? (
          <a
            href={plan.cta.href}
            target="_blank"
            rel="noreferrer"
            className={ctaClass}
          >
            {ctaLabel}
          </a>
        ) : null}
      </div>
    </article>
  );
};

const PricingPage = () => {
  const [waitlistOpen, setWaitlistOpen] = useState(false);

  return (
    <PageShell
      breadcrumb="Pricing"
      titleIcon={["Simple", "pricing."]}
      intro={`Bring your own keys and use Lennette for free. Prefer everything included? Managed is coming soon at ${MANAGED_MONTHLY_PRICE} a month.`}
    >
      <div className="grid gap-6 md:grid-cols-2">
        {plans.map((plan) => (
          <PlanCard
            key={plan.name}
            plan={plan}
            onWaitlist={() => setWaitlistOpen(true)}
          />
        ))}
      </div>

      <section className="mt-16">
        <h2 className="font-display text-2xl text-ink">
          Questions about pricing
        </h2>
        <div className="mt-6 flex flex-col gap-6">
          {PRICING_FAQS.map((faq) => (
            <div key={faq.question}>
              <h3 className="font-medium text-ink">{faq.question}</h3>
              <p className="mt-1.5 max-w-2xl leading-relaxed text-ink-soft">
                {faq.answer}
              </p>
            </div>
          ))}
        </div>
      </section>

      <WaitlistDialog
        open={waitlistOpen}
        onClose={() => setWaitlistOpen(false)}
      />
    </PageShell>
  );
};

export const Route = createFileRoute("/pricing")({
  head: () => pageHead("/pricing", PRICING_FAQS),
  component: PricingPage,
});
