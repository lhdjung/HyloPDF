/* The app icon, as geometry rather than as a bitmap.
 *
 * `src-tauri/app-icon.png` is what `tauri icon` expands into every size the
 * three platforms want, and it was the only copy of the design there was — so
 * changing it meant measuring the old PNG back into numbers first. This script
 * is those numbers. It writes the SVG beside the PNG and renders it through
 * WebKit, which is the engine the app itself draws in and the one already
 * installed for the test harness.
 *
 * Rendered at 4× and resampled down: WebKit dithers a gradient, and a ±1 level
 * of noise over 964 rows costs more in PNG than it does in fidelity.
 *
 *   node scripts/make-icon.mjs           # rewrite src-tauri/app-icon.png
 *   node scripts/make-icon.mjs out.png   # somewhere else, to look at first
 *
 * Then `npm run tauri icon` to expand it.
 */

import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { webkit } from "playwright";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = process.argv[2] ?? join(root, "src-tauri/app-icon.png");

/** The rounded square, its gradient, and the page's coral. */
const GROUND = ["#333f59", "#232834"];
const PAGE = "#de7c6e";
/** The rules on the page: warm yellow, and the short last one a shade lighter,
    which is the one thing on the icon that is not one of two colours. */
const RULE = "#ffffff";
const RULE_LAST = "#ffffff";

/* Everything below is in the 1024 grid. The page and its rules were measured
   off the icon as it stood and are then scaled about the centre, so the one
   number that decides how much of the icon the page takes up is this one. */
const PAGE_SCALE = 1.1;
const CENTRE = 512;
/** @param {number} v */
const at = (v) => +(CENTRE + (v - CENTRE) * PAGE_SCALE).toFixed(2);
/** @param {number} v */
const by = (v) => +(v * PAGE_SCALE).toFixed(2);

const rules = [392.5, 469.5, 546.5, 623.5].map((top, i) => {
  const last = i === 3;
  return `<rect x="${at(358.5)}" y="${at(top)}" width="${by(last ? 184.5 : 307)}"`
    + ` height="${by(23)}" rx="${by(11.5)}" fill="${last ? RULE_LAST : RULE}"/>`;
});

const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="ground" x1="0" y1="30" x2="0" y2="994" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="${GROUND[0]}"/>
      <stop offset="1" stop-color="${GROUND[1]}"/>
    </linearGradient>
  </defs>
  <rect x="30" y="30" width="964" height="964" rx="228" fill="url(#ground)"/>
  <rect x="${at(286.5)}" y="${at(225.5)}" width="${by(451)}" height="${by(573)}" rx="${by(34.5)}" fill="${PAGE}"/>
  ${rules.join("\n  ")}
</svg>
`;

writeFileSync(join(root, "src-tauri/app-icon.svg"), svg);

const browser = await webkit.launch();
const page = await browser.newPage({ viewport: { width: 1024, height: 1024 }, deviceScaleFactor: 4 });
await page.setContent(`<style>html,body{margin:0;background:transparent}</style>${svg}`);
const big = join(mkdtempSync(join(tmpdir(), "hylo-icon-")), "4x.png");
await page.screenshot({ path: big, omitBackground: true });
await browser.close();

execFileSync("magick", [big, "-filter", "Lanczos", "-resize", "1024x1024", "-strip", out]);
console.log(`wrote ${out}`);
