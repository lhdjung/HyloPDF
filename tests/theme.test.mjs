/* What a theme's five colours turn into.
 *
 * Two of them are derived unless the file says otherwise, and both derivations
 * are the reason a theme file can be five lines long. The selection pair is
 * the interesting one: the ink follows the area under it, so the editor can
 * show the reader what they are about to get before they have chosen it. */

import test from "node:test";
import assert from "node:assert/strict";
import { load } from "./helpers.mjs";

const { selectionArea, selectionInk, toHex, contrastRatio, parseColor } = await load(
  "src/themes.ts",
  ["selectionArea", "selectionInk", "toHex", "contrastRatio", "parseColor"],
);

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
