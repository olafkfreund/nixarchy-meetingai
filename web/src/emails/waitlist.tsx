import {
  Button,
  Column,
  Heading,
  Img,
  Row,
  Section,
  Text,
} from "@react-email/components";
import * as React from "react";

import { MANAGED_PLAN, SITE_URL } from "../lib/site";
import { emailIcons } from "./icons";
import { EmailLayout, colors, paragraph, strong } from "./layout";

export const WAITLIST_SUBJECT =
  "You're on the list. Your next meeting has backup.";
export const WAITLIST_PREVIEW =
  "Welcome to Lennette. A note from Harsh, and a look at what's coming with Managed.";

const benefits: {
  icon: keyof typeof emailIcons;
  title: string;
  detail: string;
}[] = [
  {
    icon: "sparkles",
    title: "2,000 AI responses a month",
    detail: "Help finding the words, while the conversation is happening.",
  },
  {
    icon: "audio",
    title: "30 hours of live transcription",
    detail: "Follow the conversation without taking every note yourself.",
  },
  {
    icon: "message",
    title: "Room for follow-up questions",
    detail: "Expand or rephrase an answer using the same response allowance.",
  },
  {
    icon: "users",
    title: "Keep track of who said what",
    detail: "Speaker labels make your transcripts easier to follow.",
  },
  {
    icon: "laptop",
    title: "Your allowance, across your devices",
    detail: "Use it wherever you run Lennette on Windows or Linux.",
  },
  {
    icon: "key",
    title: "The API costs are already included",
    detail: "AI and transcription in one plan. No provider keys to manage.",
  },
];

export const waitlistText = (
  email: string,
  plan: string = MANAGED_PLAN.name,
  price: string = MANAGED_PLAN.price
) =>
  `You're on the list.\n\nHey, thanks for being here. You're on the waitlist for Lennette ${plan}. I'll email ${email} when access opens.\n\nWHAT'S COMING WITH ${plan.toUpperCase()}\nPlanned pricing: ${price}/month (USD). Managed is not open yet.\n\n${benefits.map(({ title, detail }) => `${title}\n${detail}`).join("\n\n")}\n\nTry Lennette while you wait: ${SITE_URL}/downloads\nThe desktop app is free with your own AI and transcription keys. Your providers bill you for usage.\n\nWhat would you love a hand with in your next call? Hit reply. I read every message.\n\nHarsh\nBuilding Lennette\n\nYou received this because you joined the Lennette waitlist. One launch email, no drip sequence. Reply if you'd like to leave the list.`;

export const WaitlistEmail = ({
  email = "you@example.com",
  plan = MANAGED_PLAN.name,
  price = MANAGED_PLAN.price,
  attachedIcons = false,
}: {
  email?: string;
  plan?: string;
  price?: string;
  attachedIcons?: boolean;
}) => (
  <EmailLayout preview={WAITLIST_PREVIEW}>
    <Text
      style={{
        margin: "0 0 18px",
        color: "#39765f",
        fontSize: "11px",
        lineHeight: "18px",
        fontWeight: 700,
        letterSpacing: "1.4px",
      }}
    >
      YOU’RE ON THE LIST
    </Text>
    <Heading
      className="email-heading"
      style={{
        margin: "0 0 20px",
        fontFamily: 'Georgia, "Times New Roman", serif',
        fontSize: "40px",
        lineHeight: "44px",
        fontWeight: 400,
        letterSpacing: "-1.2px",
        color: colors.ink,
      }}
    >
      Your next meeting
      <br />
      has a little backup.
    </Heading>
    <Text style={paragraph}>Hey, thanks for being here.</Text>
    <Text style={paragraph}>
      You’re on the waitlist for <span style={strong}>Lennette {plan}</span>.
      I’m Harsh, the person building it. The idea is simple: less scrambling for
      the right words, more being there for the conversation.
    </Text>
    <Text style={{ ...paragraph, marginBottom: "28px" }}>
      I’ll email you at{" "}
      <span style={{ ...strong, wordBreak: "break-all" }}>{email}</span> when
      access opens. For now, here’s what I’m working on for you.
    </Text>

    <Section
      className="email-benefits"
      style={{
        backgroundColor: "#f2f7fb",
        border: "1px solid #e0eaf2",
        borderRadius: "16px",
        padding: "24px",
        marginBottom: "28px",
      }}
    >
      <Text
        style={{
          margin: "0 0 10px",
          color: colors.inkSoft,
          fontSize: "10px",
          lineHeight: "16px",
          fontWeight: 700,
          letterSpacing: "1.5px",
        }}
      >
        WHAT YOU GET WITH {plan.toUpperCase()}
      </Text>
      <Text
        style={{
          margin: "0 0 4px",
          color: colors.ink,
          fontSize: "28px",
          lineHeight: "36px",
          fontWeight: 600,
        }}
      >
        {price}
        <span
          style={{ color: colors.inkSoft, fontSize: "14px", fontWeight: 400 }}
        >
          {" "}
          / month
        </span>
      </Text>
      <Text
        style={{
          margin: "0 0 24px",
          color: colors.faint,
          fontSize: "12px",
          lineHeight: "18px",
        }}
      >
        Planned pricing in USD · Coming soon
      </Text>
      {benefits.map((benefit, index) => (
        <Row
          key={benefit.icon}
          style={{ marginBottom: index === benefits.length - 1 ? 0 : "20px" }}
        >
          <Column width="46" style={{ width: "46px", verticalAlign: "top" }}>
            <Section
              style={{
                width: "34px",
                height: "34px",
                backgroundColor: "#e3edf6",
                borderRadius: "10px",
              }}
            >
              <Img
                alt=""
                src={
                  attachedIcons
                    ? `cid:lennette-${benefit.icon}`
                    : `data:image/png;base64,${emailIcons[benefit.icon]}`
                }
                width="20"
                height="20"
                style={{ margin: "7px", display: "block" }}
              />
            </Section>
          </Column>
          <Column style={{ verticalAlign: "top" }}>
            <Text
              style={{
                margin: "0 0 3px",
                color: colors.ink,
                fontSize: "14px",
                lineHeight: "20px",
                fontWeight: 600,
              }}
            >
              {benefit.title}
            </Text>
            <Text
              style={{
                margin: 0,
                color: colors.inkSoft,
                fontSize: "13px",
                lineHeight: "20px",
              }}
            >
              {benefit.detail}
            </Text>
          </Column>
        </Row>
      ))}
    </Section>

    <Heading
      as="h2"
      style={{
        margin: "0 0 10px",
        color: colors.ink,
        fontSize: "18px",
        lineHeight: "26px",
        fontWeight: 600,
      }}
    >
      A little impatient? Me too.
    </Heading>
    <Text style={paragraph}>
      The free desktop app is ready to try. Bring your own AI and transcription
      keys and take Lennette into your next call.
    </Text>
    <Button
      href={`${SITE_URL}/downloads`}
      style={{
        backgroundColor: "#285f8b",
        borderRadius: "9999px",
        color: "#ffffff",
        fontSize: "14px",
        fontWeight: 600,
        padding: "15px 24px",
        textDecoration: "none",
      }}
    >
      Try Lennette for free&nbsp; ↗
    </Button>
    <Text
      style={{
        margin: "10px 0 28px",
        color: colors.faint,
        fontSize: "11px",
        lineHeight: "18px",
      }}
    >
      Windows &amp; Linux · Your providers bill you for API usage.
    </Text>
    <Text style={paragraph}>
      What would you love a hand with in your next call? Hit reply. I read every
      message.
    </Text>
    <Text
      style={{
        ...paragraph,
        margin: "24px 0 2px",
        color: colors.ink,
        fontWeight: 600,
      }}
    >
      Harsh
    </Text>
    <Text
      style={{
        ...paragraph,
        margin: "0 0 28px",
        color: colors.faint,
        fontSize: "13px",
      }}
    >
      Building Lennette
    </Text>
  </EmailLayout>
);

export default WaitlistEmail;
