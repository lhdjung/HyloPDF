/* Reading markup out of the file, and drawing it back under a theme.
 *
 * A highlight, an underline, a strike-out or a squiggly is a standard PDF
 * annotation — `/Subtype /Highlight` and friends, with `/QuadPoints` and
 * `/C` — and pdf.js already reads it back whoever wrote it. `markupOf` in
 * viewer.ts is what turns that into the journal's own shape, and `App`'s
 * `syncMarkup` is what puts it in `library.toml` (or, here, the browser
 * fallback's in-memory twin) the moment a document is opened. There is no UI
 * for this yet — see `markup-assessment.md`, step 3 — so this reads the
 * journal the same way `settings-window.test.mjs` reads live module state:
 * off the exact module instance the page is already running.
 *
 * The second half is step 4, "the trap": a highlighter wash is translucent
 * paint a shade or two off white, `WHITE_POINT` in `themes.ts` calls anything
 * that pale paper, and a page's own recolouring would otherwise flatten a
 * saved highlight into the theme's plain background. `tintMarkup` in
 * viewer.ts redraws it from the pristine copy towards `markupWashColor`
 * instead — see `markup-assessment.md`'s "the trap" for the reasoning, and
 * `recolor.test.mjs` for the two paths (a blend chain, and the pixel fallback
 * `HYLOPDF_NO_BLEND=1` reads the whole suite down) this rides on.
 *
 * The third part is step 5, the gesture: selecting text, pressing ⌘⇧H,
 * picking a colour from the popover that opens, and having a real
 * `/Highlight` land in the file — `Viewer.markSelection` builds it,
 * `writeDocument` saves it, and the reload that write causes reads it back
 * into the journal the same way the first test here reads one nobody's own
 * gesture wrote.
 *
 * Step 6 is the UI this was all for: a "Markup" section in the Contents
 * panel, below the marks, built by `Sidebar.showHighlights` and shown by
 * `App.showHighlights` — a row per highlight, its colour and its quote, a
 * click to jump to its page — and "Copy all as Markdown" in the section's own
 * heading. There is deliberately no way to remove one from here; see the doc
 * comment on `showHighlights` and the corrections above step 6 in
 * `markup-assessment.md` for why. */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { openApp, MOD } from "../scripts/ui-harness.mjs";

const PDF = "tests/fixtures/notes.pdf";
const DOC_PATH = path.basename(PDF);

if (!existsSync(PDF)) {
  throw new Error(`missing ${PDF} — run: node tests/fixtures/make-pdf.mjs ${PDF} 3 notes`);
}

/** The journal for one document, off the live `api.ts` the page is running.
 *  `addHighlight` is the only exported way to see what the browser
 *  fallback's in-memory map currently holds — it hands back the whole list
 *  after adding to it — so a harmless probe row is added and taken off again
 *  around the read. */
async function journalOf(app, docPath) {
  return app.page.evaluate(async (docPath) => {
    const loaded = performance
      .getEntriesByType("resource")
      .map((entry) => entry.name)
      .filter((name) => /\/src\/api\.ts(\?|$)/.test(name));
    const api = await import(loaded.at(-1) ?? "/src/api.ts");
    const probe = {
      id: "markup-test-probe",
      page: 0,
      quads: [],
      color: "#000000",
      opacity: 1,
      style: "highlight",
      quote: "",
      at: 0,
      annotation_id: null,
    };
    const highlights = await api.addHighlight(docPath, probe);
    await api.removeHighlight(docPath, "markup-test-probe");
    return highlights.filter((h) => h.id !== "markup-test-probe");
  }, docPath);
}

/** `syncMarkup` reads the whole document in the background; nothing in the
 *  interface says when it lands, so this polls for the journal to gain an
 *  entry rather than sleeping a fixed amount. */
async function waitForJournal(app, docPath, timeout = 10_000) {
  const start = Date.now();
  for (;;) {
    const highlights = await journalOf(app, docPath);
    if (highlights.length > 0 || Date.now() - start > timeout) return highlights;
    await app.page.waitForTimeout(100);
  }
}

test("a highlight already in the document is read into the journal on open", async () => {
  const app = await openApp({ pdf: PDF });
  try {
    const highlights = await waitForJournal(app, DOC_PATH);
    assert.equal(highlights.length, 1);

    const [highlight] = highlights;
    assert.equal(highlight.page, 1);
    assert.equal(highlight.style, "highlight");
    assert.equal(highlight.color, "#ffff00");
    assert.equal(highlight.opacity, 0.25);
    assert.deepEqual(highlight.quads, [54, 726, 460, 726, 54, 700, 460, 700]);
    // This fixture's whole line is one text item — a single `Tj` — wider than
    // the highlight drawn over part of it, which `quoteFor` (viewer.ts) is
    // built to recognise and leave empty rather than credit with the rest of
    // the line. See "quote.pdf" below for the case it does resolve.
    assert.equal(highlight.quote, "");
    assert.equal(typeof highlight.annotation_id, "string");
  } finally {
    await app.close();
  }
});

const QUOTE = "tests/fixtures/quote.pdf";
const QUOTE_PATH = path.basename(QUOTE);

if (!existsSync(QUOTE)) {
  throw new Error(`missing ${QUOTE} — run: node tests/fixtures/make-pdf.mjs ${QUOTE} 1 quote`);
}

test("a highlight's quote is the words under it, not the whole line they sit on", async () => {
  const app = await openApp({ pdf: QUOTE });
  try {
    const highlights = await waitForJournal(app, QUOTE_PATH);
    assert.equal(highlights.length, 1);
    // "quick" and "brown" are their own text items, each wholly inside the
    // highlight's box; "The" and "fox" sit either side of it and are not.
    assert.equal(highlights[0].quote, "quick brown");
  } finally {
    await app.close();
  }
});

/* Drawing: the highlight lands on the page's own /Rect [54 700 460 726], on a
 * 612×792 point page. The text it marks is one line, baseline at y=720 — so
 * y=703 is below every descender on it, and sampling there reads pure wash
 * with no ink on top of it. y=650 is further down the same otherwise blank
 * page, which is plain paper under every theme. */
const WASH_AT = { fx: 200 / 612, fy: 1 - 703 / 792 };
const PLAIN_AT = { fx: 200 / 612, fy: 1 - 650 / 792 };
/* The fixture's own colour and opacity — `/C [1 1 0] /CA 0.25` above. */
const RAW = [0xff, 0xff, 0x00];
const OPACITY = 0.25;

/** `a` at `t` = 0, `b` at `t` = 1 — kept independent of `themes.ts`'s own
 *  `mix`, the way `recolor.test.mjs` keeps its own `hslOf` and `luma`: this is
 *  the contract being checked, not the implementation. */
function mix(a, b, t) {
  return a.map((v, i) => Math.round(v + (b[i] - v) * t));
}

async function pixelsOn(app) {
  return app.page.evaluate(
    ({ washAt, plainAt }) => {
      const canvas = document.querySelector("#pages canvas");
      if (!canvas) return null;
      const ctx = canvas.getContext("2d");
      const at = ({ fx, fy }) =>
        [...ctx.getImageData(Math.round(fx * canvas.width), Math.round(fy * canvas.height), 1, 1).data].slice(0, 3);
      return { wash: at(washAt), plain: at(plainAt) };
    },
    { washAt: WASH_AT, plainAt: PLAIN_AT },
  );
}

/** Waited for, not slept on: recolouring a page is a whole canvas of work,
 *  and on the pixel fallback it is measured a byte at a time. Polls the plain
 *  page, away from the highlight, as the signal that the repaint has landed —
 *  the same shape `reader.test.mjs`'s own `settlesOn` polls a page's paper. */
async function settled(app, want) {
  await app.page
    .waitForFunction(
      ({ plainAt, want }) => {
        const canvas = document.querySelector("#pages canvas");
        if (!canvas) return false;
        const ctx = canvas.getContext("2d");
        const [r, g, b] = ctx.getImageData(
          Math.round(plainAt.fx * canvas.width),
          Math.round(plainAt.fy * canvas.height),
          1,
          1,
        ).data;
        return r === want[0] && g === want[1] && b === want[2];
      },
      { plainAt: PLAIN_AT, want },
      { timeout: 15_000, polling: 100 },
    )
    .catch(() => {});
}

test("a saved highlight's wash is adapted to the theme instead of vanishing as paper", async () => {
  // Both recolour the document; High Contrast runs the ramp to its full
  // extremes, which is where a rounding difference would show up first.
  const THEMES = {
    "hylo-dark": [0x24, 0x27, 0x2f],
    "high-contrast": [0x00, 0x00, 0x00],
  };

  for (const [theme, bg] of Object.entries(THEMES)) {
    const app = await openApp({
      pdf: PDF,
      settings: { theme, sidebar: false, follow_system_theme: false },
    });
    try {
      await settled(app, bg);
      const seen = await pixelsOn(app);
      assert.ok(seen, `${theme}: no page was drawn`);
      assert.deepEqual(seen.plain, bg, `${theme}: plain paper is not the theme's background`);

      // The same paint, on this theme's paper instead of white: the raw
      // colour composited at the annotation's own opacity over the theme's
      // background — see `markupWashColor` in themes.ts.
      const want = mix(RAW, bg, 1 - OPACITY);
      const off = Math.max(...seen.wash.map((v, i) => Math.abs(v - want[i])));
      assert.ok(
        off <= 2,
        `${theme}: wash came back as ${seen.wash}, wanted ${want} (within 2 of the composite)`,
      );
    } finally {
      await app.close();
    }
  }
});

test("a theme that leaves the document alone leaves the highlight as pdf.js drew it", async () => {
  // Hylo Light does not recolour — `recolor = false` in the theme file — so
  // this page never goes near `tintMarkup` at all. What is worth checking
  // here is only that the coordinates above are right and that something is
  // actually drawn where the highlight is: a wash distinctly unlike the
  // paper around it, whatever exact colour pdf.js chose to draw it in.
  const app = await openApp({
    pdf: PDF,
    settings: { theme: "hylo-light", sidebar: false, follow_system_theme: false },
  });
  try {
    await settled(app, [0xff, 0xff, 0xff]);
    const seen = await pixelsOn(app);
    assert.ok(seen, "no page was drawn");
    assert.deepEqual(seen.plain, [0xff, 0xff, 0xff], "plain paper is not white");
    assert.notDeepEqual(seen.wash, seen.plain, "the highlight is not visible on its own page");
  } finally {
    await app.close();
  }
});

/* ------------------------------------------------------------- the gesture */

const BOOK = "tests/fixtures/book.pdf";
const BOOK_PATH = path.basename(BOOK);

if (!existsSync(BOOK)) {
  throw new Error(`missing ${BOOK} — run: node tests/fixtures/make-pdf.mjs ${BOOK}`);
}

test("selecting text and choosing a colour saves a real highlight into the file", async () => {
  const app = await openApp({ pdf: BOOK });
  try {
    await app.page.waitForSelector("#pages .textLayer span", { timeout: 10_000 });
    // A short, known run of text on page one — the same shape
    // reader.test.mjs selects to test the selection-repaint path.
    const selected = await app.page.evaluate(() => {
      const span = [...document.querySelectorAll("#pages .textLayer span")].find(
        (candidate) => (candidate.firstChild?.textContent?.length ?? 0) >= 12,
      );
      const range = document.createRange();
      range.setStart(span.firstChild, 0);
      range.setEnd(span.firstChild, 12);
      const selection = getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
      return range.toString();
    });
    assert.ok(selected.length > 0, "nothing was selected to mark");

    await app.press(`${MOD}+Shift+KeyH`);
    await app.page.waitForSelector(".markup-popover .markup-swatch", { timeout: 5_000 });
    // The first swatch is `markup_color_1`'s default — see settings.rs.
    await app.page.click(".markup-popover .markup-swatch >> nth=0");

    await app.page
      .waitForFunction(
        () => (document.getElementById("notice")?.textContent ?? "") === "Marked.",
        null,
        { timeout: 10_000 },
      )
      .catch(() => {});

    const highlights = await waitForJournal(app, BOOK_PATH);
    assert.equal(highlights.length, 1, "the highlight did not round-trip through the file");
    const [highlight] = highlights;
    assert.equal(highlight.page, 1);
    assert.equal(highlight.style, "highlight");
    assert.equal(highlight.color, "#ffd60a");
    assert.equal(highlight.quads.length, 8);
    // The selection is cleared once it is saved, and the reload that saving
    // causes would have cleared it anyway.
    const stillSelected = await app.page.evaluate(() => getSelection()?.toString() ?? "");
    assert.equal(stillSelected, "");
  } finally {
    await app.close();
  }
});

/* ------------------------------------------------------------ the sidebar */

test("the Contents panel lists a document's markup, below its marks", async () => {
  const app = await openApp({ pdf: QUOTE });
  try {
    await waitForJournal(app, QUOTE_PATH);
    await app.page.click("#contents");

    const row = await app.page
      .waitForSelector("#outline-panel .highlight-row .highlight-quote", { timeout: 10_000 })
      .catch(() => null);
    assert.ok(row, "no markup row appeared in the Contents panel");
    assert.equal(await row.textContent(), "quick brown");

    // The section heading names what it is, and offers to copy the lot.
    const heading = await app.page.evaluate(
      () => document.querySelector("#outline-panel .highlights-heading .marks-title")?.textContent ?? "",
    );
    assert.equal(heading, "Markup");

    await app.page.click("#outline-panel .highlights-copy");
    await app.page
      .waitForFunction(
        () => (document.getElementById("notice")?.textContent ?? "") === "Copied all markup.",
        null,
        { timeout: 10_000 },
      )
      .catch(() => {});
    const said = await app.page.evaluate(() => document.getElementById("notice")?.textContent ?? "");
    assert.equal(said, "Copied all markup.");
  } finally {
    await app.close();
  }
});

test("clicking a piece of markup jumps to its page", async () => {
  const app = await openApp({ pdf: PDF }); // notes.pdf: three pages, a highlight on the first
  try {
    await waitForJournal(app, DOC_PATH);
    await app.page.click("#contents");
    // This fixture's highlight sits on a line too wide for `quoteFor` to
    // resolve (see the first test above), so the row falls back to naming
    // the page — which is still enough to jump by.
    await app.page.waitForSelector("#outline-panel .highlight-row .highlight-quote", {
      timeout: 10_000,
    });

    await app.press("End");
    await app.page.waitForTimeout(500);
    assert.notEqual((await app.state()).page, "1");

    await app.page.click("#outline-panel .highlight-row .highlight-go");
    await app.page
      .waitForFunction(() => document.getElementById("page-number")?.value === "1", null, {
        timeout: 15_000,
        polling: 50,
      })
      .catch(() => {});
    assert.equal((await app.state()).page, "1");
  } finally {
    await app.close();
  }
});

/* ---------------------------------------------------------------- the edges

   Step 7. Everything above assumes the ordinary case: a document this app can
   open, write and read back. These are the ones where it cannot, and each is
   one branch and one line of notice — which together are the difference
   between markup being trustworthy and being a thing people turn off.

   The disk is seeded rather than made: `openApp({ writability })` writes what
   Rust's `document_writability` would have answered into the same
   `localStorage` the browser twin of every other door reads from, so a
   read-only document needs no read-only file. See `documentWritability` in
   api.ts. */

/** Select a run of text on the page and ask for the markup popover. */
async function selectAndAsk(app, length = 12) {
  await app.page.waitForSelector("#pages .textLayer span", { timeout: 10_000 });
  const selected = await app.page.evaluate((length) => {
    const span = [...document.querySelectorAll("#pages .textLayer span")].find(
      (candidate) => (candidate.firstChild?.textContent?.length ?? 0) >= length,
    );
    const range = document.createRange();
    range.setStart(span.firstChild, 0);
    range.setEnd(span.firstChild, length);
    const selection = getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
    return range.toString();
  }, length);
  assert.ok(selected.length > 0, "nothing was selected to mark");
  await app.press(`${MOD}+Shift+KeyH`);
  await app.page.waitForSelector(".markup-popover .markup-swatch", { timeout: 5_000 });
  return selected;
}

/** What the notice line says, once it says anything but this. */
async function noticeAfter(app, before = "") {
  await app.page
    .waitForFunction(
      (before) => {
        const said = document.getElementById("notice")?.textContent ?? "";
        return said.length > 0 && said !== before;
      },
      before,
      { timeout: 15_000, polling: 100 },
    )
    .catch(() => {});
  return app.page.evaluate(() => document.getElementById("notice")?.textContent ?? "");
}

test("markup on a document that cannot be written is kept beside it, and said once", async () => {
  const app = await openApp({
    pdf: BOOK,
    writability: { writable: false, reason: "book.pdf is read only." },
  });
  try {
    await selectAndAsk(app);
    await app.page.click(".markup-popover .markup-swatch >> nth=0");

    const said = await noticeAfter(app);
    assert.match(said, /read only/, `the notice did not say why: ${said}`);
    assert.match(said, /Contents panel/, `the notice did not say where it went: ${said}`);

    const highlights = await waitForJournal(app, BOOK_PATH);
    assert.equal(highlights.length, 1, "the mark was not kept anywhere");
    // Never in the file, which is exactly what an absent annotation id means:
    // every highlight that has been through `markupOf` carries one.
    assert.equal(highlights[0].annotation_id, null);
    assert.equal(highlights[0].quads.length, 8);
    assert.ok(highlights[0].quote.length > 0, "a kept mark carries the words it marked");

    // Listed, which is the other half of what the notice promised.
    await app.page.click("#contents");
    const row = await app.page
      .waitForSelector("#outline-panel .highlight-row .highlight-quote", { timeout: 10_000 })
      .catch(() => null);
    assert.ok(row, "the kept markup is not listed in the Contents panel");

    // And the second mark does not repeat the explanation.
    await selectAndAsk(app, 8);
    await app.page.click(".markup-popover .markup-swatch >> nth=1");
    const again = await noticeAfter(app, said);
    assert.equal(again, "Kept beside the document.");
  } finally {
    await app.close();
  }
});

test("a scan says there is nothing here to mark, not that nothing is selected", async () => {
  const app = await openApp({ pdf: "tests/fixtures/notext.pdf" });
  try {
    await app.press(`${MOD}+Shift+KeyH`);
    const said = await noticeAfter(app);
    assert.match(said, /it is a scan/, `wrong notice for a scan: ${said}`);
    assert.equal(
      await app.page.locator(".markup-popover").count(),
      0,
      "the colour popover opened over a document with nothing to mark",
    );
  } finally {
    await app.close();
  }
});

test("a signed document is asked about before it is written to, once", async () => {
  const app = await openApp({ pdf: "tests/fixtures/signed.pdf" });
  try {
    await selectAndAsk(app);
    await app.page.click(".markup-popover .markup-swatch >> nth=0");

    const asked = await app.page
      .waitForSelector("#windows .window", { timeout: 10_000 })
      .catch(() => null);
    assert.ok(asked, "a signed document was written to without asking");
    const title = await app.page.evaluate(
      () => document.querySelector("#windows .window .window-title")?.textContent ?? "",
    );
    assert.equal(title, "This document is signed");

    // Cancel: nothing is written and nothing is kept.
    await app.page.click("#windows .window .btn:not(.btn-primary)");
    await app.page.waitForTimeout(300);
    assert.equal((await journalOf(app, "signed.pdf")).length, 0);

    // Again, and this time yes.
    await selectAndAsk(app);
    await app.page.click(".markup-popover .markup-swatch >> nth=0");
    await app.page.waitForSelector("#windows .window .btn-primary", { timeout: 10_000 });
    await app.page.click("#windows .window .btn-primary");

    const highlights = await waitForJournal(app, "signed.pdf");
    assert.equal(highlights.length, 1, "the highlight did not reach the file");
    assert.equal(typeof highlights[0].annotation_id, "string");

    // And the question is not asked twice about the same document.
    await selectAndAsk(app, 8);
    await app.page.click(".markup-popover .markup-swatch >> nth=1");
    await app.page
      .waitForFunction(
        () => (document.getElementById("notice")?.textContent ?? "") === "Marked.",
        null,
        { timeout: 15_000 },
      )
      .catch(() => {});
    assert.equal(
      await app.page.evaluate(() => document.querySelectorAll("#windows .window").length),
      0,
      "the signed-document question was asked a second time",
    );
  } finally {
    await app.close();
  }
});

test("markup a rebuilt document lost is offered back, and re-anchored by its words", async () => {
  const app = await openApp({ pdf: QUOTE });
  try {
    // The fixture's own highlight, plus one of this app's own over a word
    // whose text item stands alone — which is what gives it a quote to be
    // found again by.
    await waitForJournal(app, QUOTE_PATH);
    await app.page.waitForSelector("#pages .textLayer span", { timeout: 10_000 });
    const marked = await app.page.evaluate(() => {
      const span = [...document.querySelectorAll("#pages .textLayer span")].find(
        (candidate) => candidate.textContent?.trim() === "jumps",
      );
      if (!span) return "";
      const range = document.createRange();
      range.selectNodeContents(span);
      const selection = getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
      return range.toString().trim();
    });
    assert.equal(marked, "jumps", "the fixture's words are not one item each any more");

    await app.press(`${MOD}+Shift+KeyH`);
    await app.page.waitForSelector(".markup-popover .markup-swatch", { timeout: 5_000 });
    await app.page.click(".markup-popover .markup-swatch >> nth=2");
    await app.page
      .waitForFunction(
        () => (document.getElementById("notice")?.textContent ?? "") === "Marked.",
        null,
        { timeout: 15_000 },
      )
      .catch(() => {});

    let highlights = await journalOf(app, QUOTE_PATH);
    for (let tries = 0; tries < 100 && highlights.length < 2; tries++) {
      await app.page.waitForTimeout(100);
      highlights = await journalOf(app, QUOTE_PATH);
    }
    assert.equal(highlights.length, 2, "the second highlight never reached the file");
    const mine = highlights.find((held) => held.quote === "jumps");
    assert.ok(mine, `no highlight quoting "jumps": ${JSON.stringify(highlights)}`);

    // The rebuild: the file goes back to exactly the bytes it shipped with,
    // which is what a recompile does — same words, no annotations of ours.
    const original = readFileSync(QUOTE).toString("base64");
    await app.page.evaluate(async ({ docPath, original }) => {
      const loaded = performance
        .getEntriesByType("resource")
        .map((entry) => entry.name)
        .filter((name) => /\/src\/api\.ts(\?|$)/.test(name));
      const api = await import(loaded.at(-1) ?? "/src/api.ts");
      const binary = atob(original);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
      await api.writeDocument(docPath, bytes);
    }, { docPath: QUOTE_PATH, original });

    // The journal keeps what the file has lost, and the panel offers it back.
    await app.page.click("#contents");
    const offer = await app.page
      .waitForSelector("#outline-panel .highlights-restore", { timeout: 15_000 })
      .catch(() => null);
    assert.ok(offer, "nothing offered to put the lost markup back");
    assert.equal(await offer.textContent(), "Put 1 back");

    await offer.click();
    await app.page
      .waitForFunction(
        () => /Put 1 highlight back/.test(document.getElementById("notice")?.textContent ?? ""),
        null,
        { timeout: 20_000, polling: 100 },
      )
      .catch(() => {});
    const said = await app.page.evaluate(
      () => document.getElementById("notice")?.textContent ?? "",
    );
    assert.match(said, /Put 1 highlight back/, `the restore did not report: ${said}`);

    // Back in the file, at the words it was on — and the offer is gone,
    // because there is nothing left the document does not carry.
    let back = await journalOf(app, QUOTE_PATH);
    for (let tries = 0; tries < 100; tries++) {
      back = await journalOf(app, QUOTE_PATH);
      if (back.length === 2 && back.every((held) => held.annotation_id)) break;
      await app.page.waitForTimeout(100);
    }
    const restored = back.find((held) => held.quote === "jumps");
    assert.ok(restored, `the passage was not found again: ${JSON.stringify(back)}`);
    assert.equal(restored.page, 1);
    assert.equal(typeof restored.annotation_id, "string");
    assert.equal(restored.color, mine.color);
    assert.equal(
      await app.page.locator("#outline-panel .highlights-restore").count(),
      0,
      "the offer to put markup back is still there with nothing left to put",
    );
  } finally {
    await app.close();
  }
});
