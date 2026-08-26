/* Taking the margins off.
 *
 * The fixture is six pages of 612×792 with a 312×442pt block of ink on each,
 * sitting fifty points higher on every other one — so no single page shows
 * where the ink on this document begins and the union of a sample does. It
 * comes out at x 150..462 and y 150..642, which is a page with margins a
 * quarter of it wide: a scanned octavo, in other words, and the shape this
 * exists for. */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { openApp } from "../scripts/ui-harness.mjs";

const PDF = "tests/fixtures/notext.pdf";

if (!existsSync(PDF)) {
  throw new Error(`missing ${PDF} — run: node tests/fixtures/make-pdf.mjs ${PDF} 6 notext`);
}

/** The shape of the first page on screen, and of the bitmap in it. */
const shape = (app) =>
  app.page.evaluate(() => {
    const page = document.querySelector("#pages .page");
    const canvas = document.querySelector("#pages canvas");
    if (!page || !canvas) return null;
    return {
      page: page.offsetWidth / page.offsetHeight,
      canvas: canvas.width / canvas.height,
    };
  });

const settled = async (app, want) => {
  await app.page
    .waitForFunction(
      (target) => {
        const page = document.querySelector("#pages .page");
        const canvas = document.querySelector("#pages canvas");
        if (!page || !canvas) return false;
        const ratio = page.offsetWidth / page.offsetHeight;
        return (
          Math.abs(ratio - target) < 0.02 &&
          Math.abs(canvas.width / canvas.height - target) < 0.02
        );
      },
      want,
      { timeout: 20_000, polling: 100 },
    )
    .catch(() => {});
};

// 612 × 792 is 0.773 wide to tall. The ink is 312 × 492, which with the
// padding around it comes out at 0.64.
const WHOLE = 612 / 792;
const INK = 0.639;

test("the whole page, margins and all, is what you get by default", async () => {
  const app = await openApp({ pdf: PDF });
  try {
    await settled(app, WHOLE);
    const now = await shape(app);
    assert.ok(Math.abs(now.page - WHOLE) < 0.02, `page ratio was ${now.page.toFixed(3)}`);
  } finally {
    await app.close();
  }
});

test("trimming gives the room to the ink", async () => {
  const app = await openApp({ pdf: PDF, settings: { trim_margins: true } });
  try {
    await settled(app, INK);
    const now = await shape(app);
    assert.ok(
      Math.abs(now.page - INK) < 0.03,
      `the page kept its margins — ratio ${now.page.toFixed(3)}`,
    );
    // The bitmap is the cropped part of the page, not the whole of it
    // scaled down: a trimmed document costs less to draw, not the same.
    assert.ok(
      Math.abs(now.canvas - now.page) < 0.05,
      `the canvas is a different shape from its box: ${now.canvas.toFixed(3)}`,
    );

    // And the text layer hangs out past the box it is clipped by, because its
    // percentages are fractions of a whole page.
    const text = await app.page.evaluate(() => {
      const layer = document.querySelector("#pages .textLayer");
      return layer ? { left: parseFloat(layer.style.left), width: parseFloat(layer.style.width) } : null;
    });
    assert.ok(text.left < 0, `the text layer was not offset: ${text.left}`);
    const box = await app.page.evaluate(
      () => document.querySelector("#pages .page").offsetWidth,
    );
    assert.ok(text.width > box, "the text layer should be a whole page wide");
  } finally {
    await app.close();
  }
});

test("and it can be put back", async () => {
  const app = await openApp({ pdf: PDF, settings: { trim_margins: true } });
  try {
    await settled(app, INK);
    await app.page.click("#settings");
    await app.page.click('#popovers .popover-row:has(label:text-is("Trim the margins")) .switch');
    await settled(app, WHOLE);
    const now = await shape(app);
    assert.ok(Math.abs(now.page - WHOLE) < 0.02, `page ratio was ${now.page.toFixed(3)}`);
  } finally {
    await app.close();
  }
});
