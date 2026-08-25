/* Recolouring a page, both ways round and both mappings.
 *
 * There are two. `duotone` puts everything onto the theme's two colours, which
 * is what a link and a selected word want; `recolor` puts a page onto the
 * theme and leaves the colours on it alone, which is what a figure wants. They
 * are the same mapping where there is no colour, and these tests hold them to
 * that.
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
  ["recolor", "duotone"],
  "setBlend: (on) => { blendable = on; }",
);

const THEME = {
  id: "t", name: "t", built_in: false, recolor: true,
  text: "#e9eaee", background: "#24272f", accent: null, link: null, selection_area: null,
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
      globalThis.T.duotone(ctx, W, H, theme);
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
      globalThis.T.duotone(ctx, levels.length, 1, theme);
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
    globalThis.T.duotone(ctx, W, H, { ...theme, text: "#ff0000", background: "#ffffff" }, [
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
    globalThis.T.duotone(ctx, W, H, { ...theme, text: "#ff0000", background: "#ffffff" }, rects);
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


/* The page mapping: `recolor`, which keeps the colours the page has.
 *
 * These say what "keeps" means — hue and saturation as they were printed,
 * lightness wherever the theme puts it — and that a page with nothing coloured
 * on it comes out exactly as the two-colour mapping would leave it, whichever
 * path drew it. */

/** HSL, for asking what became of a colour rather than what its channels are. */
function hslOf([r, g, b]) {
  const high = Math.max(r, g, b), low = Math.min(r, g, b);
  const l = (high + low) / 2 / 255;
  const c = (high - low) / 255;
  const s = c === 0 ? 0 : c / (1 - Math.abs(2 * l - 1));
  let h = 0;
  if (c !== 0) {
    const [R, G, B] = [r / 255, g / 255, b / 255];
    h = high === r ? ((G - B) / c + 6) % 6 : high === g ? (B - R) / c + 2 : (R - G) / c + 4;
    h *= 60;
  }
  return { h, s, l };
}

/** Rec. 601 luma, the weight the ramp reads a pixel by. */
const luma = ([r, g, b]) => (r * 77 + g * 151 + b * 28 + 128) >> 8;

/** How far apart two hues are, the short way round the circle. */
const apart = (a, b) => Math.min(Math.abs(a - b), 360 - Math.abs(a - b));

test("a page of type is recoloured exactly as the two-colour mapping would", async () => {
  const seen = await page.evaluate((theme) => {
    const W = 120, H = 40;
    const paint = () => {
      const canvas = document.createElement("canvas");
      canvas.width = W;
      canvas.height = H;
      const ctx = canvas.getContext("2d", { alpha: false });
      for (let x = 0; x < W; x++) {
        const v = Math.round((x / (W - 1)) * 255);
        ctx.fillStyle = `rgb(${v},${v},${v})`;
        ctx.fillRect(x, 0, 1, H);
      }
      return ctx;
    };
    const run = (blend, how) => {
      globalThis.T.setBlend(blend);
      const ctx = paint();
      globalThis.T[how](ctx, W, H, theme);
      return [...ctx.getImageData(0, 0, W, H).data];
    };
    const compare = (a, b) => {
      let worst = 0;
      for (let i = 0; i < a.length; i++) if (i % 4 !== 3) worst = Math.max(worst, Math.abs(a[i] - b[i]));
      return worst;
    };
    const flat = run(true, "duotone");
    return {
      blended: compare(flat, run(true, "recolor")),
      pixelled: compare(flat, run(false, "recolor")),
    };
  }, THEME);

  assert.ok(seen.blended <= 1, `the blend path differs by ${seen.blended}`);
  assert.ok(seen.pixelled <= 1, `the pixel path differs by ${seen.pixelled}`);
});

test("a printed colour keeps its hue and its saturation", async () => {
  const INKS = [
    [31, 119, 180],  // a plot's blue
    [255, 127, 14],  // its orange
    [44, 160, 44],   // its green
    [148, 103, 189], // its purple
  ];
  // A light theme that recolours, for the other direction: sepia moves a page
  // a little, where a dark theme turns it over.
  const SEPIA = { ...THEME, text: "#3b3228", background: "#f4ecd8" };

  const seen = await page.evaluate(
    ({ themes, inks }) => {
      const W = 400, H = 300;
      const run = (blend, theme) => {
        globalThis.T.setBlend(blend);
        const canvas = document.createElement("canvas");
        canvas.width = W;
        canvas.height = H;
        const ctx = canvas.getContext("2d", { alpha: false });
        ctx.fillStyle = "#ffffff";
        ctx.fillRect(0, 0, W, H);
        inks.forEach(([r, g, b], i) => {
          ctx.fillStyle = `rgb(${r},${g},${b})`;
          ctx.fillRect(i * 80 + 20, 20, 40, 40);
        });
        ctx.fillStyle = "#000000";
        ctx.fillRect(20, 120, 40, 40);
        globalThis.T.recolor(ctx, W, H, theme);
        const data = ctx.getImageData(0, 0, W, H).data;
        const at = (x, y) => {
          const i = (y * W + x) * 4;
          return [data[i], data[i + 1], data[i + 2]];
        };
        return {
          paper: at(W - 5, H - 5),
          ink: at(40, 140),
          drawn: inks.map((_, i) => at(i * 80 + 40, 40)),
        };
      };
      return {
        blend: run(true, themes.dark),
        pixel: run(false, themes.dark),
        sepia: run(true, themes.sepia),
      };
    },
    { themes: { dark: THEME, sepia: SEPIA }, inks: INKS },
  );

  for (const [path, got] of Object.entries(seen)) {
    const theme = path === "sepia" ? SEPIA : THEME;
    const hex = (s) => [1, 3, 5].map((i) => parseInt(s.slice(i, i + 2), 16));
    // The page around the colours is untouched by any of this: paper is the
    // theme's background and ink is its text colour, to the level.
    assert.deepEqual(got.paper, hex(theme.background), `${path}: paper is not the background`);
    assert.deepEqual(got.ink, hex(theme.text), `${path}: black ink is not the text colour`);

    got.drawn.forEach((out, i) => {
      const was = hslOf(INKS[i]);
      const now = hslOf(out);
      assert.ok(apart(was.h, now.h) < 8, `${path}: hue ${was.h} became ${now.h}`);
      assert.ok(Math.abs(was.s - now.s) < 0.2, `${path}: saturation ${was.s} became ${now.s}`);
    });

    // What the theme does move is lightness, and it moves it the way it moves
    // the type: a dark theme turns the page over, so the darker of two inks
    // comes back the lighter one, and a light theme leaves the order alone.
    const [blue, orange] = [luma(got.drawn[0]), luma(got.drawn[1])];
    if (path === "sepia") assert.ok(blue < orange, `${path}: ${blue} is not below ${orange}`);
    else assert.ok(blue > orange, `${path}: ${blue} did not come back above ${orange}`);
  }
});

test("a hair-thin coloured line is found on a whole page of paper", async () => {
  const seen = await page.evaluate((theme) => {
    globalThis.T.setBlend(true);
    const W = 800, H = 1000;
    const canvas = document.createElement("canvas");
    canvas.width = W;
    canvas.height = H;
    const ctx = canvas.getContext("2d", { alpha: false });
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, W, H);
    // One pixel of red across the page: a curve on a plot, at the width the
    // downscale the probe reads has the least of to go on.
    ctx.fillStyle = "rgb(214,39,40)";
    ctx.fillRect(0, 500, W, 1);
    globalThis.T.recolor(ctx, W, H, theme);
    const data = ctx.getImageData(0, 500, W, 1).data;
    return [data[400 * 4], data[400 * 4 + 1], data[400 * 4 + 2]];
  }, THEME);

  const [r, g, b] = seen;
  assert.ok(r - Math.max(g, b) > 40, `the line came back as ${seen.join(",")}`);
});

test("a scan's warm paper is still the theme's paper", async () => {
  const seen = await page.evaluate((theme) => {
    const run = (blend) => {
      globalThis.T.setBlend(blend);
      const W = 200, H = 200;
      const canvas = document.createElement("canvas");
      canvas.width = W;
      canvas.height = H;
      const ctx = canvas.getContext("2d", { alpha: false });
      // The off-white of a scanned page, and a hint of the same cast in a grey.
      ctx.fillStyle = "rgb(250,246,236)";
      ctx.fillRect(0, 0, W, H);
      ctx.fillStyle = "rgb(132,129,126)";
      ctx.fillRect(0, 0, W, 20);
      globalThis.T.recolor(ctx, W, H, theme);
      const data = ctx.getImageData(0, 0, W, H).data;
      const at = (x, y) => {
        const i = (y * W + x) * 4;
        return [data[i], data[i + 1], data[i + 2]];
      };
      return { paper: at(100, 100), grey: at(100, 10) };
    };
    return { blend: run(true), pixel: run(false) };
  }, THEME);

  for (const [path, got] of Object.entries(seen)) {
    assert.deepEqual(got.paper, [0x24, 0x27, 0x2f], `${path}: the paper kept its cast`);
    // A grey that is barely off neutral is a grey, not a colour: it stays on
    // the ramp rather than carrying its cast across.
    const [r, g, b] = got.grey;
    assert.ok(Math.max(r, g, b) - Math.min(r, g, b) < 12, `${path}: the grey came back as ${got.grey}`);
  }
});
