import { Link, Section, Text } from "@react-email/components";
import * as React from "react";

import { EmailLayout, colors, paragraph, strong } from "./layout";

const benefits: { title: string; detail: string }[] = [
  {
    title: "It remembers the last meeting",
    detail:
      "Not just this call. Lennette carries the prior conversations forward, so the answer already knows what was agreed last week.",
  },
  {
    title: "2,000 AI responses a month",
    detail: "Plenty for real interview and sales days.",
  },
  {
    title: "30 hours of live transcription",
    detail: "Roughly two hours a working day, transcribed as it happens.",
  },
  {
    title: "Follow-ups come out of the same allowance",
    detail: "Ask it to expand, rephrase or dig deeper without a second quota.",
  },
  {
    title: "Speaker labels",
    detail: "Your transcripts separate who said what, so you can skim after.",
  },
  {
    title: "Any device, same allowance",
    detail: "Start on the laptop, pick it up elsewhere. No per-seat maths.",
  },
  {
    title: "Bring your own context",
    detail:
      "Attach a CV, a brief, a PDF or a screenshot when you need it to speak to something specific.",
  },
];

export const WaitlistEmail = ({
  email,
  plan = "Managed",
  price = "$35",
}: {
  email: string;
  plan?: string;
  price?: string;
}) => (
  <EmailLayout preview="You're on the list. Here's what you're getting.">
    <Text style={{ ...paragraph, marginBottom: "18px" }}>Hey,</Text>

    <Text style={paragraph}>
      Thanks for putting your name down for{" "}
      <span style={strong}>
        Lennette {plan} ({price}/month)
      </span>
      . I am building this mostly alone, so every early signup genuinely
      matters.
    </Text>

    <Text style={paragraph}>
      You are on the list for that plan. When it opens,{" "}
      <span style={strong}>{email}</span> gets the first invite, before anything
      goes public. You can unsubscribe from any email I send, no questions
      asked.
    </Text>

    <Section
      style={{
        backgroundColor: "#f7fafc",
        border: `1px solid ${colors.line}`,
        borderRadius: "12px",
        padding: "20px 22px",
        margin: "8px 0 22px 0",
      }}
    >
      <Text
        style={{
          ...paragraph,
          margin: "0 0 14px 0",
          fontSize: "13px",
          letterSpacing: "0.02em",
          color: colors.faint,
        }}
      >
        WHAT YOU GET
      </Text>
      {benefits.map((benefit, index) => (
        <Section
          key={benefit.title}
          style={{
            margin: index === benefits.length - 1 ? 0 : "0 0 14px 0",
          }}
        >
          <Text
            style={{
              margin: "0 0 3px 0",
              fontSize: "14px",
              lineHeight: "21px",
              color: colors.ink,
              fontWeight: 600,
            }}
          >
            {benefit.title}
          </Text>
          <Text
            style={{
              margin: 0,
              fontSize: "14px",
              lineHeight: "21px",
              color: colors.inkSoft,
            }}
          >
            {benefit.detail}
          </Text>
        </Section>
      ))}
    </Section>

    <Text style={paragraph}>
      One honest caveat. Hiding the overlay from a screen share only works on
      Hyprland right now, and needs a special build. On other desktops you still
      get everything above, the overlay is just visible if you share your whole
      screen. I would rather tell you that now than have you find out later.
    </Text>

    <Text style={paragraph}>
      If you want to poke at the free version while you wait, it is here:{" "}
      <Link
        href="https://github.com/parazeeknova/lennette"
        style={{ color: "#2f6f9f", textDecoration: "underline" }}
      >
        github.com/parazeeknova/lennette
      </Link>
      . Bring your own key and it costs nothing.
    </Text>

    <Text style={{ ...paragraph, margin: "22px 0 4px 0" }}>
      Just hit reply if you have a question. It reaches me, not a support desk.
    </Text>

    <Text style={{ ...paragraph, margin: 0, color: colors.ink }}>
      — Parazeek
    </Text>
  </EmailLayout>
);

export default WaitlistEmail;
