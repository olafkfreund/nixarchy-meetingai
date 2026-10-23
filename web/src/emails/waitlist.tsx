import { Column, Link, Row, Section, Text } from "@react-email/components";
import * as React from "react";

import { MANAGED_PLAN } from "../lib/site";
import {
  EmailLayout,
  Icon,
  SITE_URL,
  Signature,
  colors,
  footerNote,
  link,
  paragraph,
  strong,
} from "./layout";
import type { IconName } from "./layout";

// The allowance, at a glance.
const numbers: { icon: IconName; label: string; value: string }[] = [
  { icon: "chart", label: "AI responses a month", value: "2,000" },
  { icon: "clock", label: "Live transcription", value: "30 hours" },
  { icon: "users", label: "Any device", value: "Same allowance" },
];

const benefits: { icon: IconName; title: string; detail: string }[] = [
  {
    icon: "refresh",
    title: "Follow-ups are included",
    detail:
      "Ask it to expand, rephrase or dig deeper. Follow-ups come out of the same allowance, so there is no second quota.",
  },
  {
    icon: "paperclip",
    title: "Bring your own context",
    detail:
      "Attach a CV, a brief, a PDF or a screenshot. It answers against your material, not a generic script.",
  },
  {
    icon: "unlock",
    title: "Nothing to configure",
    detail:
      "The free version asks you to wire up your own API keys and pay a second bill. Managed removes all of that.",
  },
];

const cell = { verticalAlign: "top" as const };

export const WaitlistEmail = ({
  email,
  plan = MANAGED_PLAN.name,
  price = MANAGED_PLAN.price,
}: {
  email: string;
  plan?: string;
  price?: string;
}) => (
  <EmailLayout preview="Your seat is saved. 2,000 AI responses and 30 hours of live transcription each month.">
    <Text style={{ ...paragraph, marginBottom: "10px" }}>Hey,</Text>

    <Text
      style={{
        margin: "0 0 16px 0",
        fontSize: "19px",
        lineHeight: "28px",
        color: colors.ink,
        fontWeight: 600,
      }}
    >
      Your seat is held for Lennette {plan}.
    </Text>

    <Text style={paragraph}>
      Every signup so far is someone I get to build this for directly, so thank
      you. Here is what is on the table when it opens.
    </Text>

    <Section
      style={{
        backgroundColor: "#f4f8fc",
        borderRadius: "12px",
        padding: "18px 16px",
        margin: "4px 0 26px 0",
      }}
    >
      <Row>
        {numbers.map((item) => (
          <Column key={item.label} style={{ ...cell, width: "33.33%" }}>
            <Icon name={item.icon} size={24} />
            <Text
              style={{
                margin: "10px 0 2px 0",
                fontSize: "16px",
                lineHeight: "21px",
                color: colors.ink,
                fontWeight: 600,
              }}
            >
              {item.value}
            </Text>
            <Text
              style={{
                margin: 0,
                fontSize: "12px",
                lineHeight: "17px",
                color: colors.faint,
              }}
            >
              {item.label}
            </Text>
          </Column>
        ))}
      </Row>
    </Section>

    {benefits.map((benefit) => (
      <Row key={benefit.title} style={{ margin: "0 0 18px 0" }}>
        <Column style={{ ...cell, width: "32px" }}>
          <Icon name={benefit.icon} size={22} />
        </Column>
        <Column style={{ ...cell, paddingLeft: "10px" }}>
          <Text
            style={{
              margin: "0 0 3px 0",
              fontSize: "15px",
              lineHeight: "22px",
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
              lineHeight: "22px",
              color: colors.inkSoft,
            }}
          >
            {benefit.detail}
          </Text>
        </Column>
        <Column />
      </Row>
    ))}

    <Text
      style={{
        ...paragraph,
        margin: "8px 0 14px 0",
        fontSize: "15px",
        color: colors.inkSoft,
      }}
    >
      <span style={strong}>{email}</span> is first in line, before anything goes
      public. One email when it opens, at{" "}
      <span style={strong}>{price}/month</span>. That is the whole list.
    </Text>

    <Text style={paragraph}>
      Want to try it now? The free version runs on your own keys, so it costs
      nothing but the API usage:{" "}
      <Link href="https://github.com/parazeeknova/lennette" style={link}>
        github.com/parazeeknova/lennette
      </Link>
    </Text>

    <Text style={paragraph}>
      Hit reply if you have a question. It reaches me, not a support desk.
    </Text>

    <Signature />

    <Text style={footerNote}>
      You are getting this because you joined the Lennette waitlist.{" "}
      <Link href={SITE_URL} style={link}>
        lennette.itssingularity.com
      </Link>
    </Text>
  </EmailLayout>
);

export default WaitlistEmail;
