import {
  Body,
  Container,
  Head,
  Hr,
  Html,
  Img,
  Preview,
  Section,
  Text,
} from "@react-email/components";
import * as React from "react";
import type { ReactNode } from "react";

// Mirrors the site palette so the email feels like the same product.
const colors = {
  ink: "#1d2b3a",
  inkSoft: "#4a5b6b",
  faint: "#7c8a97",
  line: "#e3e9ee",
  wash: "#f4f8fb",
};

const ASSETS = "https://swr2.itssingularity.com/lennette";

const Header = () => (
  <Section style={{ padding: 0, margin: 0 }}>
    <Img
      src={`${ASSETS}/lennette_header.png`}
      alt="Lennette"
      width="100%"
      style={{
        display: "block",
        width: "100%",
        maxWidth: "600px",
        height: "auto",
        borderTopLeftRadius: "14px",
        borderTopRightRadius: "14px",
      }}
    />
  </Section>
);

const Footer = () => (
  <Section style={{ padding: 0, margin: 0 }}>
    <Img
      src={`${ASSETS}/lennette_footer.png`}
      alt=""
      width="100%"
      style={{
        display: "block",
        width: "100%",
        maxWidth: "600px",
        height: "auto",
        marginTop: "8px",
        borderBottomLeftRadius: "14px",
        borderBottomRightRadius: "14px",
      }}
    />
  </Section>
);

export const EmailLayout = ({
  preview,
  children,
}: {
  preview: string;
  children: ReactNode;
}) => (
  <Html lang="en">
    <Head />
    <Preview>{preview}</Preview>
    <Body
      style={{
        margin: 0,
        padding: "24px 12px",
        backgroundColor: colors.wash,
        fontFamily:
          '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif',
      }}
    >
      <Container
        style={{
          width: "100%",
          maxWidth: "600px",
          margin: "0 auto",
          backgroundColor: "#ffffff",
          border: `1px solid ${colors.line}`,
          borderRadius: "16px",
          overflow: "hidden",
        }}
      >
        <Header />
        <Section style={{ padding: "32px 36px 8px 36px" }}>{children}</Section>
        <Section style={{ padding: "0 36px 28px 36px" }}>
          <Hr
            style={{
              borderColor: colors.line,
              borderStyle: "solid",
              borderWidth: "1px 0 0 0",
              margin: "0 0 16px 0",
            }}
          />
          <Text
            style={{
              margin: 0,
              fontSize: "12px",
              lineHeight: "20px",
              color: colors.faint,
            }}
          >
            You are getting this because you joined the Lennette waitlist. Reply
            to this email and it comes straight to me.
          </Text>
        </Section>
        <Footer />
      </Container>
    </Body>
  </Html>
);

export const paragraph = {
  margin: "0 0 16px 0",
  fontSize: "15px",
  lineHeight: "25px",
  color: colors.inkSoft,
};

export const strong = { color: colors.ink, fontWeight: 600 };

export { colors };
