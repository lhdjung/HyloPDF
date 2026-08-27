/* The colour control in the theme editor, which must not lie about a colour.
 *
 * `<input type="color">` takes `#rrggbb` and nothing else: hand it `#fff`,
 * `#aabbccdd` or `steelblue` and it answers `#000000`, silently and with no
 * error anywhere. That is the same fault the theme menu's swatch and the theme
 * card were fixed for, and it matters more here — the editor is the one place
 * in the app whose whole job is to show you what you are about to get.
 *
 * The control is DOM, so it runs in a page. `ui.ts` reads its colours out of
 * `themes.ts`, and the import stripping takes that away, so both modules are
 * evaluated in turn: the first leaves `parseColor`, `readColor` and `toHex` in
 * the page's global scope, where the second finds them.
 */

import test from "node:test";
import assert from "node:assert/strict";
import { webkit } from "playwright";
import { sourceFor } from "./helpers.mjs";

const themes = await sourceFor("src/themes.ts", ["parseColor", "readColor", "toHex"]);
const ui = await sourceFor("src/ui.ts", ["colorField"]);

let browser;
let page;

test.before(async () => {
  browser = await webkit.launch();
  page = await browser.newPage();
  await page.setContent("<div id='into'></div>");
  // Indirect eval, so the modules' own `let` bindings land in global scope and
  // the second can reach the first's.
  await page.evaluate((source) => (0, eval)(source), themes);
  await page.evaluate((source) => {
    const { parseColor, readColor, toHex } = globalThis.T;
    Object.assign(globalThis, { parseColor, readColor, toHex });
    (0, eval)(source);
  }, ui);
});

test.after(async () => {
  await browser?.close();
});

/** Build a field and report what its two halves ended up showing, plus
    whatever it handed back to its caller. */
const shown = (value, fallback) =>
  page.evaluate(
    ([value, fallback]) => {
      const changes = [];
      const field = globalThis.T.colorField(value, (next) => changes.push(next), fallback);
      document.getElementById("into").replaceChildren(field);
      const [picker, text] = field.querySelectorAll("input");
      return { picker: picker.value, text: text.value, changes };
    },
    [value, fallback ?? "#000000"],
  );

test("every notation a theme file may use survives the picker", async () => {
  // The four lengths `readColor` accepts, all of them the same colour.
  for (const written of ["#aabbcc", "#abc", "#abcd", "#aabbccff"]) {
    const { picker, text } = await shown(written);
    assert.equal(picker, "#aabbcc", `${written} reached the picker as ${picker}`);
    assert.equal(text, "#aabbcc", `${written} reached the field as ${text}`);
  }
});

test("the two halves of the control always agree", async () => {
  for (const written of ["#abc", "steelblue", "rgb(30, 42, 59)", ""]) {
    const { picker, text } = await shown(written);
    assert.equal(picker, text, `${written} showed two different colours`);
  }
});

test("a colour that cannot be read takes the fallback the renderer would use", async () => {
  // Black for ink, white for paper — the same pair `parseColor`'s callers pass
  // in `applyTheme`. A background that showed black would be the picker
  // disagreeing with the page about the one colour behind everything.
  assert.equal((await shown("steelblue")).picker, "#000000");
  assert.equal((await shown("steelblue", "#ffffff")).picker, "#ffffff");
  assert.equal((await shown("rgb(30, 42, 59)", "#ffffff")).picker, "#ffffff");
});

test("nothing is reported to the caller from merely being shown", async () => {
  // The draft keeps whatever the file said until somebody actually picks a
  // colour: reading a theme must not rewrite it.
  const { changes } = await shown("#abc");
  assert.deepEqual(changes, []);
});

test("typing a short hex is taken, and reported as the long one", async () => {
  const reported = await page.evaluate(() => {
    const changes = [];
    const field = globalThis.T.colorField("#000000", (next) => changes.push(next));
    document.getElementById("into").replaceChildren(field);
    const [picker, text] = field.querySelectorAll("input");
    // Half a colour first: nothing may be reported from an incomplete one.
    text.value = "#ab";
    text.dispatchEvent(new Event("input"));
    const midway = [...changes];
    text.value = "#abc";
    text.dispatchEvent(new Event("input"));
    return { midway, changes, picker: picker.value };
  });
  assert.deepEqual(reported.midway, [], "half a colour was reported");
  assert.deepEqual(reported.changes, ["#aabbcc"], "what is written to a file is six digits");
  assert.equal(reported.picker, "#aabbcc");
});

test("show() puts a derived colour up the same way", async () => {
  // The selection ink follows the area under it, so one field writes to
  // another — and that route must read the colour too.
  const after = await page.evaluate(() => {
    const field = globalThis.T.colorField("#000000", () => {});
    document.getElementById("into").replaceChildren(field);
    field.show("#abc");
    const [picker, text] = field.querySelectorAll("input");
    return { picker: picker.value, text: text.value };
  });
  assert.equal(after.picker, "#aabbcc");
  assert.equal(after.text, "#aabbcc");
});
