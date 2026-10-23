import { Link, Section, Text } from "@react-email/components";
import * as React from "react";

import { MANAGED_PLAN } from "../lib/site";
import {
  EmailLayout,
  colors,
  footerNote,
  link,
  paragraph,
  strong,
} from "./layout";

export const NotifyEmail = ({
  email,
  plan = MANAGED_PLAN.name,
  price = MANAGED_PLAN.price,
  receivedAt,
  userAgent,
}: {
  email: string;
  plan?: string;
  price?: string;
  receivedAt: string;
  userAgent?: string;
}) => (
  <EmailLayout preview={`New waitlist signup: ${email}`}>
    <Text
      style={{
        ...paragraph,
        margin: "0 0 6px 0",
        color: colors.faint,
        fontSize: "12px",
        letterSpacing: "0.04em",
        textTransform: "uppercase",
      }}
    >
      New signup
    </Text>

    <Text
      style={{
        margin: "0 0 20px 0",
        fontSize: "20px",
        lineHeight: "28px",
        color: colors.ink,
        fontWeight: 600,
      }}
    >
      {email}
    </Text>

    <Section
      style={{
        backgroundColor: "#f4f8fc",
        borderRadius: "12px",
        padding: "16px 18px",
      }}
    >
      <Text style={{ ...paragraph, margin: "0 0 8px 0", fontSize: "14px" }}>
        Plan: <span style={strong}>{plan}</span> ({price}/month)
      </Text>
      <Text style={{ ...paragraph, margin: "0 0 8px 0", fontSize: "14px" }}>
        Received: <span style={strong}>{receivedAt}</span>
      </Text>
      {userAgent ? (
        <Text
          style={{
            ...paragraph,
            margin: 0,
            fontSize: "13px",
            color: colors.faint,
          }}
        >
          Client: {userAgent}
        </Text>
      ) : null}
    </Section>

    <Text style={{ ...paragraph, margin: "20px 0 0 0", fontSize: "14px" }}>
      Reply and it lands in their inbox:{" "}
      <Link href={`mailto:${email}`} style={link}>
        {email}
      </Link>
    </Text>

    <Text style={footerNote}>
      They were sent a note with what Managed includes and when it opens.
    </Text>
  </EmailLayout>
);

export default NotifyEmail;
