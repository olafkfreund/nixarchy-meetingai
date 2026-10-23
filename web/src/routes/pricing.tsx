import { createFileRoute } from "@tanstack/react-router";

import { GITHUB_URL, PageShell } from "@/components/page-shell";

interface Plan {
  name: string;
  price: string;
  cadence: string;
  summary: string;
  features: string[];
  cta: { label: string; href: string; external: boolean };
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
      label: "Get started",
      href: `${GITHUB_URL}#building-from-source`,
      external: true,
    },
    emphasis: false,
  },
  {
    name: "Managed",
    price: "$12",
    cadence: "per month",
    summary:
      "Everything in the free plan, with keys, model routing and quotas handled for you. No provider account needed.",
    features: [
      "No API key to create or rotate",
      "Managed speech and language models",
      "Usage included up to a fair-use cap",
      "Same private overlay and local audio",
      "Email support",
    ],
    cta: {
      label: "Request an invite",
      href: GITHUB_URL,
      external: true,
    },
    emphasis: true,
  },
];

const PricingPage = () => (
  <PageShell
    title="Pricing"
    intro="The application is free either way. The only question is who holds the model API key, you or us."
  >
    <div className="grid gap-6 md:grid-cols-2">
      {plans.map((plan) => (
        <article
          key={plan.name}
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
              className={`mt-4 min-h-[5.5rem] leading-relaxed ${
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

            <a
              href={plan.cta.href}
              target={plan.cta.external ? "_blank" : undefined}
              rel={plan.cta.external ? "noreferrer" : undefined}
              className={`mt-8 inline-flex h-13 items-center justify-center rounded-full px-6 text-[0.95rem] font-semibold transition-colors ${
                plan.emphasis
                  ? "bg-white/90 text-ink hover:bg-white"
                  : "bg-ink text-white hover:bg-ink/90"
              }`}
            >
              {plan.cta.label}
            </a>
          </div>
        </article>
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
            a separate provider subscription. Your audio still comes from your
            own device, and the overlay stays local.
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
          "Lennette is free with your own model API key, or $12 a month managed. The app itself costs nothing.",
      },
    ],
  }),
  component: PricingPage,
});
