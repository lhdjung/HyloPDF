/* What a theme's five colours turn into.
 *
 * Two of them are derived unless the file says otherwise, and both derivations
 * are the reason a theme file can be five lines long. The selection pair is
 * the interesting one: the ink follows the area under it, so the editor can
 * show the reader what they are about to get before they have chosen it. */

import test from "node:test";
import assert from "node:assert/strict";
import { load } from "./helpers.mjs";

const { selectionArea, selectionInk, toHex, contrastRatio, parseColor, readColor, unreadableColors } =
  await load("src/themes.ts", [
    "selectionArea",
    "selectionInk",
    "toHex",
    "contrastRatio",
    "parseColor",
    "readColor",
    "unreadableColors",
  ]);

/** A theme with nothing named but the two colours every theme has. */
const theme = (over = {}) => ({
  id: "t",
  name: "t",
  text: "#e9eaee",
  background: "#24272f",
  accent: null,
  link: null,
  selection: null,
  selection_text: null,
  recolor: true,
  built_in: false,
  ...over,
});

test("a theme that names nothing still has both selection colours", () => {
  const area = toHex(selectionArea(theme()));
  const ink = toHex(selectionInk(theme()));
  assert.match(area, /^#[0-9a-f]{6}$/);
  assert.match(ink, /^#[0-9a-f]{6}$/);
  assert.notEqual(area, ink);
});

test("the ink on a selection is the inverse of the area under it", () => {
  const ink = selectionInk(theme({ selection: "#802020" }));
  assert.deepEqual(ink, [255 - 0x80, 255 - 0x20, 255 - 0x20]);
});

test("an inverse too close to its own ground gives way to black or white", () => {
  // A middle grey inverts to another middle grey, which is the one case the
  // rule above cannot serve.
  const ink = toHex(selectionInk(theme({ selection: "#808080" })));
  assert.equal(ink, "#000000");
  assert.equal(toHex(selectionInk(theme({ selection: "#6b6b6b" }))), "#ffffff");
});

test("every derived pair is legible on itself", () => {
  for (const selection of [null, "#4a2f6b", "#44475a", "#504945", "#c0d1d4"]) {
    for (const background of ["#24272f", "#f2f1ed", "#010105"]) {
      const it = theme({ selection, background });
      const ratio = contrastRatio(selectionInk(it), selectionArea(it));
      assert.ok(ratio >= 3, `${selection} on ${background} came out at ${ratio.toFixed(2)}:1`);
    }
  }
});

test("a theme that names both is taken at its word", () => {
  const it = theme({ selection: "#123456", selection_text: "#fedcba" });
  assert.deepEqual(selectionArea(it), parseColor("#123456"));
  assert.deepEqual(selectionInk(it), parseColor("#fedcba"));
});


/* ------------------------------------------------------------ reading a colour

   The three ways a hand-written theme goes wrong, and the one that used to be
   worst: a hex string with a stray character in it came back as a colour
   rather than as a refusal, because `parseInt` stops at what it cannot read
   and keeps what it had. A theme that renders wrong is a bug report; a theme
   that renders black is a support question. */

test("a stray character is refused rather than half-read", () => {
  // parseInt("12345g", 16) is 0x12345 — a perfectly plausible colour, from a
  // string that is not one.
  assert.equal(readColor("#12345g"), null);
  assert.equal(readColor("#00zzzz"), null);
  assert.deepEqual(parseColor("#12345g", [1, 2, 3]), [1, 2, 3]);
});

test("hex is read in all four lengths, and an alpha is dropped", () => {
  assert.deepEqual(readColor("#1e2a3b"), [30, 42, 59]);
  assert.deepEqual(readColor("#0f0"), [0, 255, 0]);
  // A theme naming a colour with an alpha means the colour.
  assert.deepEqual(readColor("#1e2a3bff"), [30, 42, 59]);
  assert.deepEqual(readColor("#0f08"), [0, 255, 0]);
  // The hash is optional, and whitespace either side is not an error.
  assert.deepEqual(readColor("  1e2a3b "), [30, 42, 59]);
  assert.deepEqual(readColor("#1E2A3B"), [30, 42, 59]);
});

test("anything that is not hex is refused", () => {
  for (const value of ["steelblue", "rgb(30, 42, 59)", "", "#", "#12", "#1234567"]) {
    assert.equal(readColor(value), null, value);
  }
});

test("a theme says which of its colours could not be read", () => {
  assert.deepEqual(unreadableColors(theme()), []);
  assert.deepEqual(unreadableColors(theme({ text: "steelblue" })), ["text"]);
  assert.deepEqual(
    unreadableColors(theme({ background: "rgb(1,2,3)", link: "#12345g" })),
    ["background", "link"],
  );
  // An absent optional colour is derived, not wrong.
  assert.deepEqual(unreadableColors(theme({ accent: null, selection: null })), []);
});
