import {
  Body,
  Column,
  Container,
  Head,
  Html,
  Preview,
  Row,
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
  skyDeep: "#2f6f9f",
  skyBand: "#2f7fd4",
};

const SITE_URL = "https://lennette.itssingularity.com";

// Satoshi is the site face. Mail clients that cannot load it (Outlook on
// Windows) fall back to the same neutral sans the rest of the letter uses.
const FONT_STACK =
  '"Satoshi", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif';

// Kept in step with scripts/build-email-icons.mjs, which renders these to PNG.
export type IconName =
  | "bubble"
  | "bell"
  | "clock"
  | "chart"
  | "refresh"
  | "paperclip"
  | "unlock"
  | "users";

/**
 * Glyphs arrive as CID attachments (see lib/waitlist.tsx) because Gmail drops
 * inline SVG and most clients refuse data URIs. A raw <img> is deliberate:
 * react-email's <Img> would add its own sizing defaults to a 24px mark.
 */
export const Icon = ({
  name,
  size = 20,
}: {
  name: IconName;
  size?: number;
}) => (
  <img
    src={`cid:${name}`}
    alt=""
    width={size}
    height={size}
    style={{
      display: "inline-block",
      width: `${size}px`,
      height: `${size}px`,
      border: 0,
      verticalAlign: "middle",
    }}
  />
);

/**
 * A typographic band rather than the PNG header. Two reasons: the art is 1.6MB
 * and most inboxes now block remote images by default, so a remote header
 * shows up as a blank strip; and text this much letter spacing renders sharp
 * at any density.
 */
const Header = () => (
  <Section style={{ padding: 0, margin: 0 }}>
    <table
      width="100%"
      cellPadding={0}
      cellSpacing={0}
      role="presentation"
      style={{
        width: "100%",
        backgroundColor: colors.skyBand,
        borderTopLeftRadius: "14px",
        borderTopRightRadius: "14px",
      }}
    >
      <tbody>
        <tr>
          <td
            style={{
              padding: "26px 32px",
              textAlign: "center",
              fontFamily: FONT_STACK,
              fontSize: "19px",
              lineHeight: "27px",
              color: "#ffffff",
              fontWeight: 500,
              letterSpacing: "-0.01em",
            }}
          >
            Know exactly what to say, live.
          </td>
        </tr>
      </tbody>
    </table>
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
        fontFamily: FONT_STACK,
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
        <Section style={{ padding: "32px 36px 28px 36px" }}>{children}</Section>
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

export const link = { color: colors.skyDeep, textDecoration: "underline" };

export const footerNote = {
  margin: "26px 0 0 0",
  paddingTop: "16px",
  borderTop: `1px solid ${colors.line}`,
  fontSize: "12px",
  lineHeight: "19px",
  color: colors.faint,
};

// Closes the letter: the little leaf from the header art, then the name.
export const Signature = () => (
  <Row style={{ margin: "26px 0 0 0" }}>
    <Column style={{ width: "30px", verticalAlign: "top" }}>
      <Icon name="bubble" size={24} />
    </Column>
    <Column style={{ paddingLeft: "10px", verticalAlign: "top" }}>
      <Text
        style={{
          margin: 0,
          fontSize: "15px",
          lineHeight: "22px",
          color: colors.ink,
          fontWeight: 600,
        }}
      >
        Harsh
      </Text>
      <Text
        style={{
          margin: 0,
          fontSize: "13px",
          lineHeight: "19px",
          color: colors.faint,
        }}
      >
        building Lennette
      </Text>
    </Column>
    <Column />
  </Row>
);

export { SITE_URL, colors };
