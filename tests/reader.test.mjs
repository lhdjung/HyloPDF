/* The reader itself, driven headlessly.
 *
 * These go through `ui-harness.mjs`, which is the browser fallback in `api.ts`
 * — no Rust, no window. That covers everything the interface does and nothing
 * about the window it sits in; full screen, the title bar and the drag regions
 * still have to be looked at in the real app.
 *
 * Needs a dev server. `npm test` starts one; `node --test "tests/*.test.mjs"` on its own
 * expects `npm run dev` to already be running. */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { openApp } from "../scripts/ui-harness.mjs";

const PDF = "tests/fixtures/book.pdf";
const PAGES = 400;

if (!existsSync(PDF)) {
  throw new Error(`missing ${PDF} — run: node tests/fixtures/make-pdf.mjs ${PDF} ${PAGES}`);
}

let app;

test.before(async () => {
  app = await openApp({ pdf: PDF });
});

test.after(async () => {
  await app?.close();
});

test("a document opens and knows how long it is", async () => {
  const state = await app.state();
  assert.equal(state.pages, `of ${PAGES}`);
  assert.equal(state.page, "1");
});

test("the scrollbar tells the truth about the whole book", async () => {
  // Pages beyond the first are measured in the background, so the layout
  // settles shortly after the document opens rather than before it appears.
  await app.page.waitForTimeout(1500);
  const height = await app.page.evaluate(
    () => document.getElementById("pages").offsetHeight,
  );
  // Four hundred pages of a page each; the exact height depends on the fit.
  assert.ok(height > PAGES * 500, `scroll height was only ${height}px`);
});

test("pages are drawn", async () => {
  const drawn = await app.page.evaluate(
    () => [...document.querySelectorAll(".page canvas")].filter((c) => c.width > 1).length,
  );
  assert.ok(drawn > 0, "no page was painted");
});

test("only the pages near the viewport exist", async () => {
  const mounted = await app.page.evaluate(
    () => document.querySelectorAll("#pages .page").length,
  );
  assert.ok(mounted > 0 && mounted < 12, `${mounted} pages were in the DOM`);
});

test("moving around", async (t) => {
  await t.test("End reaches the last page", async () => {
    await app.press("End");
    assert.equal((await app.state()).page, String(PAGES));
  });

  await t.test("Home comes back", async () => {
    await app.press("Home");
    assert.equal((await app.state()).page, "1");
  });

  await t.test("the arrow keys turn pages", async () => {
    await app.press("ArrowRight");
    assert.equal((await app.state()).page, "2");
    await app.press("ArrowLeft");
    assert.equal((await app.state()).page, "1");
  });
});

test("ctrl+wheel zooms", async () => {
  const before = (await app.state()).zoom;
  await app.wheel(4, -40, { ctrl: true });
  const after = (await app.state()).zoom;
  assert.notEqual(after, before);
  assert.match(after, /%$/);
});

test("search", async (t) => {
  await t.test("finds matches and highlights them", async () => {
    await app.page.keyboard.press("Meta+f");
    await app.page.waitForTimeout(150);
    await app.page.fill("#find-input", "quick brown");
    await app.page.waitForTimeout(2500);

    const state = await app.state();
    assert.match(state.findStatus ?? "", /\d+ of \d+/);

    const marks = await app.page.evaluate(
      () => document.querySelectorAll(".find-highlight").length,
    );
    assert.ok(marks > 0, "matches were counted but not shown");
  });

  await t.test("Escape puts it away", async () => {
    await app.page.keyboard.press("Escape");
    assert.equal((await app.state()).findOpen, false);
  });
});

test("menus answer the keyboard", async (t) => {
  await t.test("opening one from the keyboard moves the focus into it", async () => {
    await app.page.locator("#theme").focus();
    await app.page.keyboard.press("Enter");
    await app.page.waitForTimeout(200);

    assert.equal((await app.state()).menuOpen, true);
    const inside = await app.page.evaluate(
      () => document.getElementById("popovers").contains(document.activeElement),
    );
    assert.ok(inside, "the menu opened without taking the keyboard");
  });

  await t.test("the arrow keys move through it", async () => {
    const before = await app.page.evaluate(() => document.activeElement?.textContent);
    await app.page.keyboard.press("ArrowDown");
    const after = await app.page.evaluate(() => document.activeElement?.textContent);
    assert.notEqual(after, before);
  });

  await t.test("Escape closes it", async () => {
    await app.page.keyboard.press("Escape");
    assert.equal((await app.state()).menuOpen, false);
  });
});

test("nothing went wrong on the way", () => {
  const noise = app.logs.filter(
    (line) =>
      /pageerror|error:/i.test(line) &&
      // pdf.js says this when a document leans on a font it has to fetch and
      // the fixture, which embeds nothing, does. Not the app's doing.
      !/standardFontDataUrl/.test(line),
  );
  assert.deepEqual(noise, []);
});
