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
  <Section style={{ padding: "12px 12px 0", margin: 0 }}>
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
  footer = "You received this because you joined the Lennette waitlist. One launch email, no drip sequence. Reply if you'd like to leave the list.",
}: {
  preview: string;
  children: ReactNode;
  footer?: string;
}) => (
  <Html lang="en">
    <Head>
      <meta name="color-scheme" content="light" />
      <meta name="supported-color-schemes" content="light" />
      <style>{`
        @media only screen and (max-width: 480px) {
          .email-content { padding: 24px 22px 0 !important; }
          .email-footer { padding: 0 22px 24px !important; }
          .email-heading { font-size: 33px !important; line-height: 38px !important; }
          .email-benefits { padding: 20px 16px !important; }
        }
      `}</style>
    </Head>
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
        <Section className="email-content" style={{ padding: "24px 40px 0" }}>
          {children}
        </Section>
        <Section className="email-footer" style={{ padding: "0 40px 28px" }}>
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
            {footer}
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
