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
 * `HYLOPDF_NO_BLEND=1` reads the whole suite down) this rides on. */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import path from "node:path";
import { openApp } from "../scripts/ui-harness.mjs";

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
    // Read out of the file, not drawn by this app — nothing here knows the
    // words underneath the quad or when it was originally made.
    assert.equal(highlight.quote, "");
    assert.equal(typeof highlight.annotation_id, "string");
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
