/* The app's own recolouring, over a fixed page of pixels, written down.
 *
 * `src/recolor.rs` says it is a faithful port of `recolorByPixel` in
 * `themes.ts` and `tests/recolor.rs` holds the shader to it — but the thing
 * it is faithful *to* was never in the comparison. Both sides could have been
 * wrong together, and the one place it would show is a page: a link a shade
 * off the colour the app paints it, on every document, with nothing saying so.
 *
 * So this runs the app's function in WebKit, over pixels chosen to reach every
 * branch of it, and writes what comes out. `tests/parity.rs` reads the file
 * and asks the port for the same page.
 *
 * Regenerate it — no dev server needed, this compiles the module in memory:
 *
 *   node experiments/dioxus-reader/tests/parity/take-recolor.mjs
 */
import { webkit } from "playwright";
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { sourceFor } from "../../../../tests/helpers.mjs";

const OUT = path.join(path.dirname(fileURLToPath(import.meta.url)), "recolor-fixture.json");

/* The two ramps the app runs, named by what wants them.
 *
 * `keepColour: false` is `duotone` — a link, a selected word, and a page with
 * no colour on it. The colours are Hylo Light's real link case: the theme's
 * copper against the white a page that is not recoloured is printed on.
 * `keepColour: true` is `recolor` — a page put onto a theme with the colours
 * on it left alone, in Hylo Dark's two. */
const RAMPS = [
  { name: "duotone", text: "#9c5a2c", bg: "#ffffff", keepColour: false },
  { name: "recolor", text: "#e9eaee", bg: "#24272f", keepColour: true },
];

const source = await sourceFor("src/themes.ts", ["recolorByPixel", "parseColor"]);

const browser = await webkit.launch();
const page = await browser.newPage();
await page.setContent("<div></div>");
await page.evaluate((src) => (0, eval)(src), source);

const result = await page.evaluate(
  ({ ramps }) => {
    /* Pixels chosen to reach every branch: the whole grey ramp, which is a
     * page of type; saturated colours, which is a figure; the near-neutrals
     * either side of the colour floor; the pale washes above the white point;
     * and a few colours a plotting library actually emits. */
    const pixels = [];
    const push = (r, g, b) => pixels.push(r, g, b, 255);
    for (let level = 0; level <= 255; level++) push(level, level, level);
    for (let level = 0; level <= 255; level += 5) {
      push(level, Math.min(255, level + 6), level);
      push(level, Math.min(255, level + 20), Math.min(255, level + 40));
      push(level, 40, 200);
      push(200, level, 40);
      push(40, 200, level);
    }
    for (const [r, g, b] of [
      [255, 255, 255], [0, 0, 0], [250, 248, 240], [236, 236, 236], [234, 234, 234],
      [31, 119, 180], [255, 127, 14], [44, 160, 44], [214, 39, 40],
    ]) push(r, g, b);

    const width = pixels.length / 4;
    // Hex rather than base64 or an array of numbers: the file stays a
    // quarter the size of the array and the Rust side needs three lines to
    // read it instead of a dependency.
    const hex = (bytes) => bytes.map((byte) => byte.toString(16).padStart(2, "0")).join("");
    const run = ({ text, bg, keepColour }) => {
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = 1;
      // `alpha: false` for the reason the app renders pages that way: a canvas
      // with an alpha channel premultiplies, and a byte that has been through
      // that and back is not the byte that went in.
      const ctx = canvas.getContext("2d", { alpha: false });
      const image = ctx.createImageData(width, 1);
      image.data.set(pixels);
      ctx.putImageData(image, 0, 0);
      globalThis.T.recolorByPixel(
        ctx, width, 1,
        globalThis.T.parseColor(text, [0, 0, 0]),
        globalThis.T.parseColor(bg, [255, 255, 255]),
        undefined,
        keepColour,
      );
      return hex([...ctx.getImageData(0, 0, width, 1).data]);
    };
    return {
      width,
      pixels: hex(pixels),
      ramps: Object.fromEntries(ramps.map((ramp) => [ramp.name, run(ramp)])),
    };
  },
  { ramps: RAMPS },
);

await browser.close();
writeFileSync(OUT, JSON.stringify({ ...result, ramps: RAMPS.map((r) => ({ ...r, out: result.ramps[r.name] })) }, null, 2) + "\n");
console.log(`wrote ${OUT} (${result.width} pixels)`);
