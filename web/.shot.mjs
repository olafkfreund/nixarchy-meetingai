// Screenshot helper: node shot.mjs <url> <out.png> <width> <height> [dpr]
import { chromium } from "playwright";

const [url, out, width = "1440", height = "900", dpr = "1"] =
  process.argv.slice(2);

const browser = await chromium.launch({
  executablePath: "/run/current-system/sw/bin/google-chrome",
  args: ["--no-sandbox"],
});
const page = await browser.newPage({
  viewport: { width: Number(width), height: Number(height) },
  deviceScaleFactor: Number(dpr),
  reducedMotion: "reduce",
});
page.on(
  "console",
  (m) => m.type() === "error" && console.log("console:", m.text())
);
page.on("pageerror", (e) => console.log("pageerror:", e.message));
const response = await page.goto(url, { waitUntil: "networkidle" });
await page.waitForTimeout(600);
await page.screenshot({ path: out, fullPage: false });
console.log(`${out} status=${response?.status()}`);
console.log("title:", await page.title());
await browser.close();
