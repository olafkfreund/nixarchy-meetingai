import { Hr, Link, Section, Text } from "@react-email/components";
import * as React from "react";

import { EmailLayout, colors, paragraph, strong } from "./layout";

export const NotifyEmail = ({
  email,
  plan = "Managed",
  price = "$35",
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
        margin: "0 0 4px 0",
        color: colors.faint,
        fontSize: "13px",
      }}
    >
      NEW WAITLIST SIGNUP
    </Text>

    <Text
      style={{
        margin: "0 0 18px 0",
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
        backgroundColor: "#f7fafc",
        border: `1px solid ${colors.line}`,
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

    <Hr
      style={{
        borderColor: colors.line,
        borderStyle: "solid",
        borderWidth: "1px 0 0 0",
        margin: "20px 0 16px 0",
      }}
    />

    <Text style={{ ...paragraph, margin: "0 0 8px 0", fontSize: "14px" }}>
      Reply to this email and it goes straight to them, since they are set as
      the reply-to address.
    </Text>
    <Text style={{ ...paragraph, margin: 0, fontSize: "14px" }}>
      <Link
        href={`mailto:${email}`}
        style={{ color: "#2f6f9f", textDecoration: "underline" }}
      >
        {email}
      </Link>
    </Text>
  </EmailLayout>
);

export default NotifyEmail;
