/* The panel on the left, driven headlessly.
 *
 * It had no tests of any kind, and it was where the worst of what a review
 * turned up was living: it drew a thumbnail for every page that scrolled past,
 * kept the page proxy and the canvas forever, and started a second render into
 * a canvas that already had one whenever the theme changed. What follows is
 * the observable half of all three.
 *
 * Needs a dev server, like `reader.test.mjs`. `npm test` starts one.
 */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { MOD, openApp } from "../scripts/ui-harness.mjs";

const PDF = "tests/fixtures/book.pdf";

if (!existsSync(PDF)) {
  throw new Error(`missing ${PDF} — run: node tests/fixtures/make-pdf.mjs ${PDF} 400`);
}

let app;

test.before(async () => {
  app = await openApp({ pdf: PDF, settings: { show_sidebar: true } });
  // The Pages tab, which is the half with the pictures in it.
  await app.page.click("#sidebar .tab[data-tab='pages']");
});

test.after(async () => {
  await app?.close();
});

/** Every thumbnail that has been drawn, by page, with the size of its bitmap.
    A placeholder is 168 wide; anything drawn is wider than that. */
const drawnThumbs = () =>
  app.page.evaluate(() =>
    [...document.querySelectorAll("#pages-panel .thumb")]
      .map((button) => ({
        page: Number(button.dataset.page),
        width: button.querySelector("canvas")?.width ?? 0,
      }))
      .filter((thumb) => thumb.width > 168),
  );

test("thumbnails are drawn for the pages in view", async () => {
  await app.page
    .waitForFunction(
      () =>
        [...document.querySelectorAll("#pages-panel .thumb canvas")].filter((c) => c.width > 168)
          .length > 0,
      null,
      { timeout: 20_000, polling: 100 },
    )
    .catch(() => {});

  const drawn = await drawnThumbs();
  assert.ok(drawn.length > 0, "no thumbnail was ever drawn");
  // Lazily: four hundred pages, and only the ones near the top of the column.
  assert.ok(drawn.length < 60, `drew ${drawn.length} of 400 thumbnails at once`);
});

test("scrolling the column does not keep every picture it has drawn", async () => {
  // Far enough that the ones at the top are well out of view and past the cap.
  for (let i = 0; i < 12; i++) {
    await app.page.evaluate(() => {
      document.getElementById("pages-panel").scrollBy(0, 4000);
    });
    await app.page.waitForTimeout(120);
  }
  await app.page.waitForTimeout(1500);

  const drawn = await drawnThumbs();
  assert.ok(drawn.length > 0, "nothing is drawn after scrolling");
  // The cap is 40. Without one this was every page the column had passed.
  assert.ok(
    drawn.length <= 45,
    `${drawn.length} thumbnails still hold a bitmap; the cap is 40`,
  );
});

test("a theme change redraws the thumbnails that are on screen", async () => {
  await app.page.evaluate(() => {
    document.getElementById("pages-panel").scrollTo(0, 0);
  });
  await app.page.waitForTimeout(800);

  /** What the middle of the first drawn thumbnail looks like. */
  const sample = () =>
    app.page.evaluate(() => {
      const canvas = [...document.querySelectorAll("#pages-panel .thumb canvas")].find(
        (c) => c.width > 168,
      );
      if (!canvas) return null;
      const ctx = canvas.getContext("2d");
      return [...ctx.getImageData(2, 2, 1, 1).data.slice(0, 3)].join(",");
    });

  await app.page
    .waitForFunction(
      () =>
        [...document.querySelectorAll("#pages-panel .thumb canvas")].some((c) => c.width > 168),
      null,
      { timeout: 20_000, polling: 100 },
    )
    .catch(() => {});
  const light = await sample();
  assert.ok(light, "no thumbnail to sample");

  // Dark mode three times in quick succession — which is what used to leave a
  // second render going into a canvas that already had one, get refused by
  // pdf.js, and strand the thumbnail in the theme before last.
  //
  // Honest note: this passes against the old code too. Headless WebKit
  // finishes a thumbnail faster than the presses arrive, so the overlap does
  // not reproduce here. It is a guard on the behaviour, not a reproduction of
  // the bug — the bug is in `redrawVisible` starting a render without calling
  // off the one in flight, and that is read rather than measured.
  await app.press(`${MOD}+d`);
  await app.press(`${MOD}+d`);
  await app.press(`${MOD}+d`);

  await app.page
    .waitForFunction(
      (was) => {
        const canvas = [...document.querySelectorAll("#pages-panel .thumb canvas")].find(
          (c) => c.width > 168,
        );
        if (!canvas) return false;
        const ctx = canvas.getContext("2d");
        return [...ctx.getImageData(2, 2, 1, 1).data.slice(0, 3)].join(",") !== was;
      },
      light,
      { timeout: 20_000, polling: 100 },
    )
    .catch(() => {});

  const dark = await sample();
  assert.notEqual(dark, light, "the thumbnail kept the theme it was drawn in");
});

test("putting the document down clears the column", async () => {
  await app.page.click("#close-doc");
  await app.page.waitForTimeout(500);
  const thumbs = await app.page.evaluate(
    () => document.querySelectorAll("#pages-panel .thumb").length,
  );
  assert.equal(thumbs, 0);
});
