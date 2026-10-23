// Renders the waitlist email glyphs from real Satoshi-era geometry into PNGs.
//
// Email clients cannot be trusted with inline SVG or data URIs, so every glyph
// ships as a CID attachment. Keeping them as small PNGs also means the letter
// looks the same in Gmail, Outlook and Apple Mail.
//
//   node scripts/build-email-icons.mjs
//
// Output: src/emails/assets/*.png, committed so sending needs no native deps.

import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";

const root = path.join(import.meta.dirname, "..");
const out = path.join(root, "src/emails/assets");
const tmp = path.join(root, "node_modules/.cache/email-icons");

// From the site palette (styles.css --sky / --sky-deep / --ink).
const TILE = "#e8f2fc";
const RIM = "#cfe2f5";
const GLYPH = "#1d2b3a";

// Css pixels. The retinas get twice the pixels, the glyph matches the text.
const TILE_SIZE = 44;
const SCALE = 2;
const GLYPH_SIZE = 24;

/** Lucide geometry, the set the site already uses. */
const LUCIDE = {
  clock: ["M12 6v6l4 2", "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20"],
  chart: ["M3 3v16a2 2 0 0 0 2 2h16", "M18 17V9", "M13 17V5", "M8 17v-3"],
  users: [
    "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2",
    "M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8",
    "M22 21v-2a4 4 0 0 0-3-3.87",
    "M16 3.13a4 4 0 0 1 0 7.75",
  ],
  refresh: ["M21 12a9 9 0 1 1-2.64-6.36", "M21 3v6h-6"],
  paperclip: [
    "M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48",
  ],
  unlock: ["M13 4h3a2 2 0 0 1 2 2v2", "M2 14h20", "M6 10V7a2 2 0 0 1 2-2h3"],
  bell: [
    "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9",
    "M10.3 21a1.94 1.94 0 0 0 3.4 0",
  ],
};

/**
 * The bubble holds a whole sentence, which no icon set ships. Drawn on the
 * same 24-unit grid and stroke weight as the Lucide marks above, with the
 * two-dot face from the header art.
 */
const BUBBLE = {
  paths: [
    "M4 5h16a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H9l-4 4v-4H4a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Z",
  ],
  dots: [
    [9.2, 11],
    [14.8, 11],
  ],
};

const icons = {
  bubble: BUBBLE,
  clock: { paths: LUCIDE.clock },
  chart: { paths: LUCIDE.chart },
  users: { paths: LUCIDE.users },
  refresh: { paths: LUCIDE.refresh },
  paperclip: { paths: LUCIDE.paperclip },
  unlock: { paths: LUCIDE.unlock },
  bell: { paths: LUCIDE.bell },
};

const px = (n) => `${n * SCALE}px`;

const svg = ({ paths, dots = [] }) =>
  [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="${px(GLYPH_SIZE)}" height="${px(GLYPH_SIZE)}">`,
    `<g fill="none" stroke="${GLYPH}" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">`,
    paths.map((d) => `<path d="${d}"/>`).join(""),
    `</g>`,
    dots
      .map(
        ([cx, cy]) => `<circle cx="${cx}" cy="${cy}" r="1.1" fill="${GLYPH}"/>`
      )
      .join(""),
    `</svg>`,
  ].join("");

const run = (cmd, args) =>
  execFileSync(cmd, args, { stdio: ["ignore", "ignore", "inherit"] });

mkdirSync(out, { recursive: true });
mkdirSync(tmp, { recursive: true });

for (const bin of ["rsvg-convert", "magick"]) {
  try {
    execFileSync("sh", ["-c", `command -v ${bin}`], { stdio: "ignore" });
  } catch {
    console.error(`Missing ${bin}. Install it and re-run.`);
    process.exit(1);
  }
}

const size = TILE_SIZE * SCALE;
const half = 0.5 * SCALE;

for (const [name, icon] of Object.entries(icons)) {
  const src = path.join(tmp, `${name}.svg`);
  const glyph = path.join(tmp, `${name}.png`);
  writeFileSync(src, svg(icon));
  run("rsvg-convert", ["-o", glyph, src]);

  run("magick", [
    "-size",
    `${size}x${size}`,
    "xc:none",
    "-fill",
    TILE,
    "-stroke",
    RIM,
    "-strokewidth",
    `${SCALE}`,
    "-draw",
    `roundrectangle ${half},${half} ${size - half},${size - half} 26,26`,
    "(",
    glyph,
    ")",
    "-gravity",
    "center",
    "-composite",
    "-strip",
    path.join(out, `${name}.png`),
  ]);
}

rmSync(tmp, { recursive: true, force: true });
console.log(`Wrote ${Object.keys(icons).length} icons to src/emails/assets`);
