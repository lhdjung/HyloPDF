/* A document with nothing in it to read.
 *
 * A scan that never went through OCR draws pictures of words, and three
 * things go quiet at once: search finds nothing, selection selects nothing,
 * and the contents are empty. Two of those look like a broken app, and
 * "None" — the answer to "is this word in the document" — is the wrong answer
 * to the question actually being asked. */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { MOD, openApp } from "../scripts/ui-harness.mjs";

const PDF = "tests/fixtures/notext.pdf";

if (!existsSync(PDF)) {
  throw new Error(`missing ${PDF} — run: node tests/fixtures/make-pdf.mjs ${PDF} 6 notext`);
}

test("a document with no text says so rather than saying None", async () => {
  const app = await openApp({ pdf: PDF });
  try {
    await app.press(`${MOD}+KeyF`);
    await app.page.waitForTimeout(150);
    await app.page.keyboard.type("the");
    await app.page
      .waitForFunction(
        () => {
          const status = document.getElementById("find-status")?.textContent ?? "";
          return status.length > 0 && status !== "…";
        },
        null,
        { timeout: 15_000, polling: 50 },
      )
      .catch(() => {});

    const state = await app.state();
    assert.equal(state.findStatus, "No text");

    // And once, out loud, because search is only one of the three things that
    // just went quiet.
    const said = await app.page.evaluate(
      () => document.getElementById("notice")?.textContent ?? "",
    );
    assert.match(said, /no text in this document/i);
  } finally {
    await app.close();
  }
});

test("a document that does have text still says None when it has no match", async () => {
  const app = await openApp({ pdf: "tests/fixtures/labelled.pdf" });
  try {
    await app.press(`${MOD}+KeyF`);
    await app.page.waitForTimeout(150);
    await app.page.keyboard.type("zzzznothing");
    await app.page
      .waitForFunction(
        () => {
          const status = document.getElementById("find-status")?.textContent ?? "";
          return status.length > 0 && status !== "…";
        },
        null,
        { timeout: 15_000, polling: 50 },
      )
      .catch(() => {});
    assert.equal((await app.state()).findStatus, "None");
  } finally {
    await app.close();
  }
});
