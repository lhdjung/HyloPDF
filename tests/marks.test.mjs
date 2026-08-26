/* Marks: the reader's own note of where they were going back to.
 *
 * Not annotations. Nothing is written into the document and nothing appears
 * on the page — a mark is a page number in `library.toml`, beside the page
 * each document was left on. */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { MOD, openApp } from "../scripts/ui-harness.mjs";

const PDF = "tests/fixtures/labelled.pdf";

if (!existsSync(PDF)) {
  throw new Error(`missing ${PDF} — run: node tests/fixtures/make-pdf.mjs ${PDF} 12 labels`);
}

const marks = (app) =>
  app.page.evaluate(() =>
    [...document.querySelectorAll("#outline-panel .mark .mark-go")].map((el) => el.textContent),
  );

test("a page can be marked and found again", async (t) => {
  const app = await openApp({ pdf: PDF });
  try {
    await app.page.waitForTimeout(1200);
    await app.page.click("#contents");
    await app.page.waitForTimeout(200);
    assert.deepEqual(await marks(app), [], "a fresh document has no marks");

    await t.test("marking says so and lists it", async () => {
      await app.page.keyboard.press(`${MOD}+Shift+KeyB`);
      await app.page.waitForTimeout(400);
      // The document numbers its own pages, and a mark is named for one.
      assert.deepEqual(await marks(app), ["Page i"]);
      const said = await app.page.evaluate(
        () => document.getElementById("notice")?.textContent ?? "",
      );
      assert.match(said, /Marked page i/);
    });

    await t.test("a mark leads back to its page", async () => {
      await app.press("End");
      await app.page.waitForTimeout(500);
      assert.notEqual((await app.state()).page, "i");
      await app.page.click("#outline-panel .mark .mark-go");
      await app.page
        .waitForFunction(() => document.getElementById("page-number")?.value === "i", null, {
          timeout: 15_000,
          polling: 50,
        })
        .catch(() => {});
      assert.equal((await app.state()).page, "i");
    });

    await t.test("and the same key takes it off again", async () => {
      await app.page.keyboard.press(`${MOD}+Shift+KeyB`);
      await app.page.waitForTimeout(400);
      assert.deepEqual(await marks(app), []);
      const said = await app.page.evaluate(
        () => document.getElementById("notice")?.textContent ?? "",
      );
      assert.match(said, /Took the mark off page i/);
    });

    await t.test("marks are listed in page order, whatever order they were made in", async () => {
      await app.press("End");
      await app.page.waitForTimeout(500);
      await app.page.keyboard.press(`${MOD}+Shift+KeyB`);
      await app.page.waitForTimeout(300);
      await app.press("Home");
      await app.page.waitForTimeout(500);
      await app.page.keyboard.press(`${MOD}+Shift+KeyB`);
      await app.page.waitForTimeout(400);
      assert.deepEqual(await marks(app), ["Page i", "Page 8"]);
    });
  } finally {
    await app.close();
  }
});
