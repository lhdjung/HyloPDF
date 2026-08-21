/* Searching finds words that are spelled the way a reader would type them
   rather than the way a typesetter stored them. */

import test from "node:test";
import assert from "node:assert/strict";
import { load } from "./helpers.mjs";

const { fold, locate } = await load("src/search.ts", ["fold", "locate"]);

/** A page built from its text runs, the way `textFor` builds one. */
function page(items) {
  const starts = [];
  let raw = "";
  for (const run of items) {
    starts.push(raw.length);
    raw += run;
  }
  const folded = fold(raw);
  return { items, text: folded.text, origin: folded.origin, starts };
}

test("folding", async (t) => {
  await t.test("splits ligatures", () => {
    assert.equal(fold("ﬁnd").text, "find");
    assert.equal(fold("oﬃce").text, "office");
    assert.equal(fold("ﬂow").text, "flow");
  });

  await t.test("drops accents", () => {
    assert.equal(fold("résumé").text, "resume");
    assert.equal(fold("Ångström").text, "angstrom");
  });

  await t.test("folds case", () => {
    assert.equal(fold("The QUICK").text, "the quick");
  });

  await t.test("removes what is in the text but not in the word", () => {
    assert.equal(fold("typo­graphy").text, "typography"); // soft hyphen
    assert.equal(fold("a​b").text, "ab"); // zero-width space
  });

  await t.test("records where every character came from", () => {
    const { text, origin } = fold("ﬁnd");
    // One entry per output character, plus one past the end.
    assert.equal(origin.length, text.length + 1);
    // Both halves of the ligature came from the same source character.
    assert.deepEqual(origin, [0, 0, 1, 2, 3]);
  });
});

test("locating a match in the page's own text", async (t) => {
  const document = page(["The ", "ﬁrst ", "résumé"]);

  await t.test("the folded page reads as typed", () => {
    assert.equal(document.text, "the first resume");
  });

  await t.test("a word written with a ligature", () => {
    const [hit] = locate(document, fold("first").text, 1);
    assert.ok(hit, "expected a match");
    assert.deepEqual([hit.itemStart, hit.itemEnd], [1, 1]);
    assert.deepEqual([hit.offsetStart, hit.offsetEnd], [0, 4]);
  });

  await t.test("an accented word typed plainly", () => {
    const [hit] = locate(document, fold("resume").text, 1);
    assert.ok(hit, "expected a match");
    assert.deepEqual([hit.itemStart, hit.itemEnd], [2, 2]);
    assert.deepEqual([hit.offsetStart, hit.offsetEnd], [0, 6]);
  });

  await t.test("a match ending inside a ligature still covers something", () => {
    const [hit] = locate(document, fold("f").text, 1);
    assert.ok(hit.offsetEnd > hit.offsetStart, "the highlight would be empty");
  });

  await t.test("a word broken across a line", () => {
    const broken = page(["typo­", "graphy"]);
    const [hit] = locate(broken, fold("typography").text, 1);
    assert.ok(hit, "expected a match");
    assert.deepEqual([hit.itemStart, hit.itemEnd], [0, 1]);
  });

  await t.test("a word that is not there", () => {
    assert.equal(locate(document, fold("second").text, 1).length, 0);
  });
});
