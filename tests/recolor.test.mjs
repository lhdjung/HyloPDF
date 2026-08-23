/* Recolouring a page, both ways round.
 *
 * The fast path is a chain of canvas blend modes; `saturation` among them is
 * non-separable and not every engine really implements it on a canvas, so
 * there is a pixel-by-pixel fallback. These tests say the two agree, and that
 * the fallback stays inside the part of the page it was asked about — which
 * the fast path gets from a clipping path and the fallback cannot, because
 * `putImageData` is the one drawing operation that ignores clipping. */

import test from "node:test";
import assert from "node:assert/strict";
import { webkit } from "playwright";
import { sourceFor } from "./helpers.mjs";

const source = await sourceFor(
  "src/themes.ts",
  ["recolor"],
  "setBlend: (on) => { blendable = on; }",
);

const THEME = {
  id: "t", name: "t", built_in: false, recolor: true,
  text: "#e9eaee", background: "#24272f", accent: null, link: null, selection: null,
};

let browser;
let page;

test.before(async () => {
  browser = await webkit.launch();
  page = await browser.newPage();
  await page.setContent("<div></div>");
  // Indirect eval, so the module's own `let` bindings are not shadowed by the
  // names already in scope here.
  await page.evaluate((src) => (0, eval)(src), source);
});

test.after(async () => {
  await browser?.close();
});

test("the pixel fallback matches the blend modes", async () => {
  const { worst, mean } = await page.evaluate((theme) => {
    const W = 64, H = 8;
    const paint = () => {
      const canvas = document.createElement("canvas");
      canvas.width = W;
      canvas.height = H;
      const ctx = canvas.getContext("2d", { alpha: false });
      for (let x = 0; x < W; x++) {
        const v = Math.round((x / (W - 1)) * 255);
        ctx.fillStyle = `rgb(${v},${v},${v})`;       // a grey ramp
        ctx.fillRect(x, 0, 1, 4);
        ctx.fillStyle = `rgb(${v},${255 - v},128)`;  // and some saturated ink
        ctx.fillRect(x, 4, 1, 4);
      }
      return ctx;
    };
    const run = (blend) => {
      globalThis.T.setBlend(blend);
      const ctx = paint();
      globalThis.T.recolor(ctx, W, H, theme);
      return [...ctx.getImageData(0, 0, W, H).data];
    };

    const blended = run(true);
    const pixelled = run(false);
    let worst = 0, sum = 0, n = 0;
    for (let i = 0; i < blended.length; i++) {
      if (i % 4 === 3) continue; // alpha
      const d = Math.abs(blended[i] - pixelled[i]);
      worst = Math.max(worst, d);
      sum += d;
      n++;
    }
    return { worst, mean: sum / n };
  }, THEME);

  // The two arrive by different arithmetic, so they round differently; one
  // level out of 255 is as close as that can get.
  assert.ok(worst <= 1, `channels differ by up to ${worst}`);
  assert.ok(mean < 0.5, `average difference ${mean}`);
});

test("near-white ink lands on the background, and ink does not", async () => {
  const seen = await page.evaluate((theme) => {
    // Every level, both ways round, measured against the two ends of the ramp.
    const levels = [...Array(256).keys()];
    const run = (blend) => {
      globalThis.T.setBlend(blend);
      const canvas = document.createElement("canvas");
      canvas.width = levels.length;
      canvas.height = 1;
      const ctx = canvas.getContext("2d", { alpha: false });
      for (const v of levels) {
        ctx.fillStyle = `rgb(${v},${v},${v})`;
        ctx.fillRect(v, 0, 1, 1);
      }
      globalThis.T.recolor(ctx, levels.length, 1, theme);
      const data = ctx.getImageData(0, 0, levels.length, 1).data;
      return levels.map((v) => [data[v * 4], data[v * 4 + 1], data[v * 4 + 2]]);
    };
    return { blend: run(true), pixel: run(false) };
  }, THEME);

  const bg = [0x24, 0x27, 0x2f];
  const text = [0xe9, 0xea, 0xee];
  const away = (rgb, from) => Math.max(...rgb.map((v, i) => Math.abs(v - from[i])));

  for (const [path, ramp] of Object.entries(seen)) {
    // Paper is the background, and so is anything close enough to paper to
    // have been invisible on it — the hyperref boxes this white point exists
    // for sit around level 230.
    assert.equal(away(ramp[255], bg), 0, `${path}: paper is not the background`);
    assert.equal(away(ramp[240], bg), 0, `${path}: near-white ink is not the background`);
    assert.ok(away(ramp[230], bg) <= 5, `${path}: a 90% grey is ${away(ramp[230], bg)} off`);

    // What was actually printed still is. Black is the dodge's fixed point, so
    // full-strength ink keeps the whole of the text colour, and a mid grey
    // stays unmistakably a mid grey.
    assert.equal(away(ramp[0], text), 0, `${path}: black ink is not the text colour`);
    assert.ok(away(ramp[128], bg) > 70, `${path}: mid grey collapsed to ${away(ramp[128], bg)}`);

    // Monotone throughout: no level is darker than a darker one, which is what
    // would show as a band across a gradient.
    for (let level = 1; level < 256; level++) {
      assert.ok(
        away(ramp[level], text) >= away(ramp[level - 1], text),
        `${path}: the ramp turns back at ${level}`,
      );
    }
  }
});

test("the fallback colours only the region it was given", async () => {
  const seen = await page.evaluate((theme) => {
    globalThis.T.setBlend(false);
    const W = 40, H = 10;
    const canvas = document.createElement("canvas");
    canvas.width = W;
    canvas.height = H;
    const ctx = canvas.getContext("2d", { alpha: false });
    ctx.fillStyle = "#000000";
    ctx.fillRect(0, 0, W, H);

    // Two rectangles that overlap and do not fill their own bounding box —
    // the shape a couple of links on a line actually make.
    globalThis.T.recolor(ctx, W, H, { ...theme, text: "#ff0000", background: "#ffffff" }, [
      { x: 5, y: 2, w: 10, h: 4 },
      { x: 12, y: 4, w: 10, h: 4 },
    ]);

    const at = (x, y) => {
      const [r, g, b] = ctx.getImageData(x, y, 1, 1).data;
      return `${r},${g},${b}`;
    };
    return {
      outside: at(1, 1),
      belowTheBoundingBox: at(6, 8),
      gapInsideTheBoundingBox: at(20, 2),
      firstRectangle: at(7, 3),
      whereTheyOverlap: at(14, 5),
      secondRectangle: at(20, 6),
    };
  }, THEME);

  assert.equal(seen.outside, "0,0,0", "reached outside the rectangles");
  assert.equal(seen.belowTheBoundingBox, "0,0,0", "reached below the rectangles");
  assert.equal(seen.gapInsideTheBoundingBox, "0,0,0", "filled the bounding box, not the rectangles");
  // Black ink maps to the text colour.
  assert.equal(seen.firstRectangle, "255,0,0");
  assert.equal(seen.secondRectangle, "255,0,0");
  // Applied once, not twice: a second pass would move it off the ramp.
  assert.equal(seen.whereTheyOverlap, "255,0,0", "the overlap was coloured twice");
});


/* A page of links, on the path without blend modes.
 *
 * A bibliography's links run from the top of the page to the bottom, so their
 * bounding box is the whole page — and the fallback used to ask, for every
 * pixel in that box, whether it was inside any of the rectangles. Rectangles
 * times pixels. This is that shape at a size a test can afford, and it checks
 * the answer rather than the clock: what must be true is that only the
 * rectangles moved, however many of them there are. */
test("many rectangles colour only themselves", async () => {
  const seen = await page.evaluate((theme) => {
    globalThis.T.setBlend(false);
    const W = 300, H = 400;
    const canvas = document.createElement("canvas");
    canvas.width = W;
    canvas.height = H;
    const ctx = canvas.getContext("2d", { alpha: false });
    ctx.fillStyle = "#000000";
    ctx.fillRect(0, 0, W, H);

    // Two hundred link-shaped rectangles down the page, none of them touching.
    const rects = [];
    for (let i = 0; i < 200; i++) rects.push({ x: 10, y: i * 2, w: 60, h: 1 });

    const started = performance.now();
    globalThis.T.recolor(ctx, W, H, { ...theme, text: "#ff0000", background: "#ffffff" }, rects);
    const ms = performance.now() - started;

    const at = (x, y) => ctx.getImageData(x, y, 1, 1).data.slice(0, 3).join(",");
    return {
      ms,
      insideFirst: at(20, 0),
      insideLast: at(20, 398),
      betweenTwo: at(20, 1),
      rightOfThem: at(200, 100),
    };
  }, THEME);

  assert.equal(seen.insideFirst, "255,0,0");
  assert.equal(seen.insideLast, "255,0,0");
  assert.equal(seen.betweenTwo, "0,0,0", "coloured the gap between two links");
  assert.equal(seen.rightOfThem, "0,0,0", "coloured beyond the rectangles");
});
