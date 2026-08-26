/* The notes a document already carries.
 *
 * pdf.js paints an annotation's own appearance into the page, so a sticky
 * note arrives as the icon it was drawn as and a highlight arrives
 * highlighted. What does not arrive is the text behind either of them — that
 * lives in a popup this app does not build — so the icon sat there looking
 * like a button and was not one. Reading a note is not annotating. */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { openApp } from "../scripts/ui-harness.mjs";

const PDF = "tests/fixtures/notes.pdf";

if (!existsSync(PDF)) {
  throw new Error(`missing ${PDF} — run: node tests/fixtures/make-pdf.mjs ${PDF} 3 notes`);
}

test("a document's own notes can be read", async (t) => {
  const app = await openApp({ pdf: PDF });
  try {
    await app.page
      .waitForFunction(() => document.querySelectorAll("#pages .note-layer button").length >= 2, null, {
        timeout: 15_000,
        polling: 100,
      })
      .catch(() => {});

    await t.test("both kinds get somewhere to press", async () => {
      const spots = await app.page.evaluate(() =>
        [...document.querySelectorAll("#pages .note-layer button")].map((el) => ({
          kind: el.className,
          label: el.getAttribute("aria-label"),
        })),
      );
      assert.equal(spots.length, 2);
      // The sticky note is icon-sized and gets the whole of itself; the
      // comment on a highlighted line gets a strip at its right edge, so the
      // words underneath stay in reach of a pointer that wants to select them.
      assert.deepEqual(spots.map((spot) => spot.kind).sort(), ["note-edge", "note-spot"]);
      assert.ok(spots.every((spot) => spot.label.startsWith("Note. A. Reviewer:")));
    });

    await t.test("and pressing one says what it says", async () => {
      await app.page.click("#pages .note-spot");
      await app.page.waitForSelector("#windows .window", { timeout: 10_000 });
      const shown = await app.page.evaluate(() => ({
        title: document.querySelector("#windows .pane-title")?.textContent,
        where: document.querySelector("#windows .pane-lede")?.textContent,
        text: document.querySelector("#windows .note-text")?.textContent,
      }));
      assert.equal(shown.title, "A. Reviewer");
      assert.equal(shown.where, "On page 1.");
      assert.equal(shown.text, "Check this figure against the appendix.");
      await app.press("Escape");
    });
  } finally {
    await app.close();
  }
});
