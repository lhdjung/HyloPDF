/* A book that numbers its own pages.
 *
 * Front matter runs i, ii, iii and the body starts again at 1, so the twelfth
 * page of the file is page 8 and "go to page 314" from an index does not mean
 * the 314th thing in the file. The fixture is that shape in miniature: four
 * roman pages, then eight arabic ones. */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { openApp } from "../scripts/ui-harness.mjs";

const PDF = "tests/fixtures/labelled.pdf";

if (!existsSync(PDF)) {
  throw new Error(`missing ${PDF} — run: node tests/fixtures/make-pdf.mjs ${PDF} 12 labels`);
}

let app;

test.before(async () => {
  app = await openApp({ pdf: PDF });
  // The labels are asked for without waiting, so they land a moment after the
  // first page is drawn — which is the whole reason the toolbar is told again
  // when they do.
  await app.page
    .waitForFunction(() => document.getElementById("page-number")?.value === "i", null, {
      timeout: 15_000,
      polling: 50,
    })
    .catch(() => {});
});

test.after(async () => {
  await app?.close();
});

const reaches = async (label) => {
  await app.page
    .waitForFunction((l) => document.getElementById("page-number")?.value === l, label, {
      timeout: 15_000,
      polling: 50,
    })
    .catch(() => {});
  assert.equal((await app.state()).page, label);
};

test("the toolbar shows the page the document prints, not its place in the file", async () => {
  const state = await app.state();
  assert.equal(state.page, "i");
  assert.equal(state.pages, "of 8", "the count should be the last label, not the page count");
});

test("the pill says both, because only it can", async () => {
  const pill = await app.page.evaluate(
    () => document.getElementById("page-pill")?.textContent,
  );
  assert.equal(pill, "i (1 of 12)");
});

test("a page label is what the go-to field takes", async (t) => {
  await t.test("a roman numeral finds the front matter", async () => {
    await app.press("p");
    await app.page.keyboard.type("iii");
    await app.page.keyboard.press("Enter");
    await reaches("iii");
  });

  await t.test("an arabic number finds the body, not the file's own count", async () => {
    await app.press("p");
    await app.page.keyboard.type("2");
    await app.page.keyboard.press("Enter");
    await reaches("2");
    // Page 2 of the body is the sixth page of the file.
    const pill = await app.page.evaluate(
      () => document.getElementById("page-pill")?.textContent,
    );
    assert.equal(pill, "2 (6 of 12)");
  });

  await t.test("something that names no page leaves the reader where they are", async () => {
    await app.press("p");
    await app.page.keyboard.type("qqq");
    await app.page.keyboard.press("Enter");
    await reaches("2");
  });
});

test("the thumbnails are numbered the same way", async () => {
  await app.page.click("#contents");
  await app.page.click('.tab[data-tab="pages"]');
  await app.page.waitForTimeout(300);
  const numbers = await app.page.evaluate(() =>
    [...document.querySelectorAll("#pages-panel .thumb .thumb-number")]
      .slice(0, 6)
      .map((el) => el.textContent),
  );
  assert.deepEqual(numbers, ["i", "ii", "iii", "iv", "1", "2"]);
  await app.page.click("#contents");
});
