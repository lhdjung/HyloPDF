/* An encrypted document asks rather than fails — and lets you say no.
 *
 * Saying no is the half that was broken and is the reason this file exists.
 * Declining used to answer pdf.js with an empty password, which pdf.js reads
 * as another wrong attempt rather than as giving up, so the question came
 * straight back and there was no way out of it at all. */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { openApp } from "../scripts/ui-harness.mjs";

const PDF = "tests/fixtures/locked.pdf";
const PASSWORD = "hylo";

if (!existsSync(PDF)) {
  throw new Error(`missing ${PDF} — run: node tests/fixtures/make-encrypted-pdf.mjs ${PDF}`);
}

/** Type into the password field and press Open. */
async function answer(app, password) {
  await app.page.fill("#windows .window input[type=password]", password);
  await app.page.keyboard.press("Enter");
  await app.page.waitForTimeout(1200);
}

test("an encrypted document asks for its password", async () => {
  const app = await openApp({ pdf: PDF, expect: "password" });
  try {
    const state = await app.state();
    assert.equal(state.windowTitle, "This document is locked");
    assert.match(state.windowText ?? "", /needs a password/);
    // Nothing was opened behind it.
    assert.equal(state.onStartScreen, true);
  } finally {
    await app.close();
  }
});

test("a wrong password says so and asks again", async () => {
  const app = await openApp({ pdf: PDF, expect: "password" });
  try {
    await answer(app, "not the password");
    const state = await app.state();
    assert.equal(state.windowTitle, "This document is locked");
    assert.match(state.windowText ?? "", /not right/);
  } finally {
    await app.close();
  }
});

test("the right password opens it", async () => {
  const app = await openApp({ pdf: PDF, expect: "password" });
  try {
    await answer(app, PASSWORD);
    await app.page.waitForFunction(
      () => (document.getElementById("page-count")?.textContent ?? "").length > 0,
      null,
      { timeout: 15_000 },
    );
    const state = await app.state();
    assert.equal(state.windowTitle, null, "the window should be gone");
    assert.equal(state.pages, "of 1");
    assert.equal(state.onStartScreen, false);
  } finally {
    await app.close();
  }
});

test("declining gives up rather than asking forever", async () => {
  const app = await openApp({ pdf: PDF, expect: "password" });
  try {
    await app.page.keyboard.press("Escape");
    // Long enough that a re-ask would have arrived by now.
    await app.page.waitForTimeout(2000);

    const state = await app.state();
    assert.equal(state.windowTitle, null, "the question came back");
    assert.equal(state.onStartScreen, true);

    // And it said nothing, because there is nothing to report.
    const notice = await app.page.evaluate(() => {
      const line = document.getElementById("notice");
      return line?.hidden ? null : line?.textContent;
    });
    assert.equal(notice, null, `unexpected notice: ${notice}`);
  } finally {
    await app.close();
  }
});
