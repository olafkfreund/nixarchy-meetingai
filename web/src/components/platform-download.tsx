import { Link } from "@tanstack/react-router";
import { useSyncExternalStore } from "react";

import type { Platform } from "./platform-marks";
import { platformMarks } from "./platform-marks";

const subscribe = () => () => {
  // User agent never changes during a session, so no listener is needed.
};

const detect = (): Platform | "unknown" => {
  if (typeof navigator === "undefined") {
    return "unknown";
  }
  // Prefer Client Hints when the browser exposes them; they are more accurate
  // than the UA string on platforms that freeze it.
  const uaData = (
    navigator as Navigator & { userAgentData?: { platform?: string } }
  ).userAgentData;
  const hint = uaData?.platform?.toLowerCase();
  const ua = navigator.userAgent.toLowerCase();
  const haystack = `${hint ?? ""} ${ua}`;

  if (haystack.includes("win")) {
    return "windows";
  }
  if (
    haystack.includes("mac") ||
    haystack.includes("iphone") ||
    haystack.includes("ipad")
  ) {
    return "macos";
  }
  if (
    haystack.includes("linux") ||
    haystack.includes("android") ||
    haystack.includes("x11")
  ) {
    return "linux";
  }
  return "unknown";
};

const getSnapshot = () => detect();
// The server has no user agent, so it renders the neutral label. The client
// swaps in the real platform after hydration.
const getServerSnapshot = (): Platform | "unknown" => "unknown";

const labels: Record<Platform | "unknown", string> = {
  linux: "Download for Linux",
  windows: "Download for Windows",
  macos: "Download for macOS",
  unknown: "Download for desktop",
};

const marks: Record<
  Platform | "unknown",
  ((props: { className?: string }) => React.ReactElement) | null
> = {
  ...platformMarks,
  unknown: null,
};

export const PlatformDownload = () => {
  const platform = useSyncExternalStore(
    subscribe,
    getSnapshot,
    getServerSnapshot
  );
  const Mark = marks[platform];

  return (
    <Link
      to="/downloads"
      id="download"
      className="glass btn-liquid mt-8 glass-pill"
    >
      {Mark ? <Mark className="size-5 shrink-0" /> : null}
      <span>{labels[platform]}</span>
    </Link>
  );
};
