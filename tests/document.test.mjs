/* What a document says about itself.
 *
 * A file named `2310.06825v3.pdf` is not a name, and a shelf of them is
 * unreadable — but the file usually knows better. And "get info", which every
 * other reader has, did not exist here at all. */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { openApp } from "../scripts/ui-harness.mjs";

const PDF = "tests/fixtures/titled.pdf";
const PLAIN = "tests/fixtures/notext.pdf";

if (!existsSync(PDF)) {
  throw new Error(`missing ${PDF} — run: node tests/fixtures/make-pdf.mjs ${PDF} 3 titled`);
}

const titleShown = (app) =>
  app.page.evaluate(() => document.getElementById("doc-title")?.textContent ?? "");

test("a document with a title of its own is called by it", async () => {
  const app = await openApp({ pdf: PDF });
  try {
    await app.page
      .waitForFunction(
        () => document.getElementById("doc-title")?.textContent?.startsWith("On the Quiet"),
        null,
        { timeout: 15_000, polling: 50 },
      )
      .catch(() => {});
    assert.equal(await titleShown(app), "On the Quiet Reading of Documents");
    // The file name is still there for whoever needs it.
    const tip = await app.page.evaluate(() => document.getElementById("doc-title")?.title ?? "");
    assert.equal(tip, "titled.pdf");
  } finally {
    await app.close();
  }
});

test("a document without one keeps its file name", async () => {
  const app = await openApp({ pdf: PLAIN });
  try {
    await app.page.waitForTimeout(1200);
    assert.equal(await titleShown(app), "notext.pdf");
  } finally {
    await app.close();
  }
});

test("what the document says about itself, in a window", async () => {
  const app = await openApp({ pdf: PDF });
  try {
    await app.page.click("#doc-title");
    await app.page.click('#popovers .popover-item:has-text("says about itself")');
    await app.page.waitForSelector("#windows .window", { timeout: 10_000 });

    const shown = await app.page.evaluate(() => ({
      title: document.querySelector("#windows .pane-title")?.textContent ?? "",
      rows: [...document.querySelectorAll("#windows .field")].map((field) => [
        field.querySelector(".field-label")?.textContent,
        field.querySelector(".field-control")?.textContent,
      ]),
    }));

    assert.equal(shown.title, "On the Quiet Reading of Documents");
    const rows = Object.fromEntries(shown.rows);
    assert.equal(rows.Author, "A. Reader");
    assert.equal(rows.Pages, "3");
    assert.match(rows["Page size"], /216 × 279 mm/);
    // A PDF date is `D:20240131120000Z` and nobody reads that.
    assert.match(rows.Created, /2024/);
    assert.ok(!rows.Created.startsWith("D:"), `left as ${rows.Created}`);
    // Fields the document does not fill in are not shown as empty rows.
    assert.equal(rows.Subject, undefined);
  } finally {
    await app.close();
  }
});
