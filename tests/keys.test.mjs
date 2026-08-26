/* The keyboard: the table, the file it can be rewritten from, and a key
 * actually pressed in a running app.
 *
 * Most of this is `keys.ts` on its own, which is the right level for it: a
 * chord is a pure function of an event, and the whole point of moving the
 * shortcuts out of `wireKeyboard`'s tower of branches was that the answer no
 * longer depends on which branch ran first. The two things that cannot be
 * tested that way are at the end — the shipped `keys.toml` agreeing with the
 * defaults it claims to be showing, and a rebound key reaching the document.
 */

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { load } from "./helpers.mjs";
import { openApp } from "../scripts/ui-harness.mjs";

const NAMES = [
  "ACTIONS",
  "GROUPS",
  "parseChord",
  "parseBinding",
  "chordsOf",
  "buildKeymap",
  "defaultKeys",
  "needsDocument",
  "describeChord",
  "describeBinding",
];

// `isMac` comes from api.ts, which `load` strips along with every other
// import. Both answers are worth having: `mod` is the only thing in a chord
// that means something different on each platform, and the shortcut scheme is
// built on it. HYLOPDF_PLATFORM=other does the same for the app itself.
const mac = await load("src/keys.ts", NAMES, "const isMac = true;");
const pc = await load("src/keys.ts", NAMES, "const isMac = false;");

/** A keydown, as the app would see it. */
const press = (key, { code = "", mod = false, ctrl = false, alt = false, shift = false } = {}) => ({
  key,
  code: code || (/^[a-z]$/i.test(key) ? `Key${key.toUpperCase()}` : ""),
  metaKey: mod,
  ctrlKey: ctrl,
  altKey: alt,
  shiftKey: shift,
});

/* ---------------------------------------------------------------- chords */

test("a chord is spelled one way however it was written", () => {
  assert.equal(mac.parseChord("Shift+Mod+F"), "mod+shift+f");
  assert.equal(mac.parseChord("cmd+alt+g"), "mod+alt+g");
  assert.equal(mac.parseChord("option+ArrowLeft"), "alt+left");
  assert.equal(mac.parseChord("ESC"), "escape");
  assert.equal(mac.parseChord("Plus"), "+");
  // `+` is a key as well as a separator, which is why a chord is peeled from
  // the front rather than split.
  assert.equal(mac.parseChord("mod++"), "mod++");
  assert.equal(mac.parseChord("mod+-"), "mod+-");
});

test("Control is its own key on a Mac and is the modifier everywhere else", () => {
  assert.equal(mac.parseChord("ctrl+d"), "ctrl+d");
  assert.notEqual(mac.parseChord("ctrl+d"), mac.parseChord("mod+d"));
  // Not a second thing it could mean: pretending otherwise would be a file
  // that quietly does nothing on Windows.
  assert.equal(pc.parseChord("ctrl+d"), "mod+d");
});

test("a key HyloPDF cannot read says so rather than guessing", () => {
  assert.equal(mac.parseChord("mod+wibble"), null);
  assert.equal(mac.parseChord(""), null);
  assert.equal(mac.parseChord("mod+"), null);
  assert.equal(mac.parseChord("f13"), null);
  // Half a sequence is not a shorter sequence.
  assert.equal(mac.parseBinding("g wibble"), null);
  assert.equal(mac.parseBinding("g g"), "g g");
});

/* ---------------------------------------------------------------- events */

test("Option is not a letter on a Mac, and the physical key still is", () => {
  // ⌥G arrives as ©: the G is not there to compare any more, which used to
  // need `event.code` by hand in the one branch that knew about it.
  const chords = mac.chordsOf(press("©", { code: "KeyG", mod: true, alt: true }));
  assert.ok(chords.includes("mod+alt+g"), chords.join(" "));
});

test("Shift is kept for a letter and offered both ways for anything else", () => {
  assert.deepEqual(mac.chordsOf(press("G", { shift: true })), ["shift+g"]);
  // ⇧Space must be able to mean something of its own, and must still fall
  // through to Space when it does not.
  const space = mac.chordsOf(press(" ", { code: "Space", shift: true }));
  assert.deepEqual(space, ["shift+space", "space"]);
});

test("a chord is offered by what it says and by what key was hit", () => {
  const zoom = mac.chordsOf(press("+", { code: "Equal", mod: true, shift: true }));
  assert.deepEqual(zoom, ["mod+shift++", "mod+shift+=", "mod++", "mod+="]);
});

test("the modifiers a platform does not use match nothing", () => {
  assert.deepEqual(mac.chordsOf(press("Shift", { shift: true })), []);
  // The Windows key is not bound to anything, and reading it as no modifier
  // at all would turn ⊞J into a scroll.
  assert.deepEqual(pc.chordsOf(press("j", { mod: true })), []);
  assert.deepEqual(pc.chordsOf(press("j", { ctrl: true })), ["mod+j"]);
});

/* --------------------------------------------------------------- keymaps */

test("what HyloPDF ships with is readable, and no key is asked to do two things", () => {
  for (const platform of [mac, pc]) {
    const map = platform.buildKeymap();
    assert.deepEqual(map.problems, []);
    for (const spec of platform.ACTIONS) {
      for (const binding of platform.defaultKeys(spec)) {
        assert.equal(platform.parseBinding(binding), binding, `${spec.id}: ${binding}`);
      }
    }
  }
});

test("every action is in a group the Keyboard page draws", () => {
  const groups = new Set(mac.GROUPS);
  for (const spec of mac.ACTIONS) assert.ok(groups.has(spec.group), spec.id);
  const ids = mac.ACTIONS.map((spec) => spec.id);
  assert.equal(new Set(ids).size, ids.length, "an action is named twice");
});

test("naming an action replaces its keys; naming it with nothing unbinds it", () => {
  const map = mac.buildKeymap({ "next-page": ["n"], dark: [] });
  assert.deepEqual(map.problems, []);
  assert.equal(map.byBinding.get("n"), "next-page");
  assert.equal(map.byBinding.get("right"), undefined, "the default was replaced, not added to");
  assert.equal(map.byBinding.get("mod+d"), undefined);
  // Everything not mentioned keeps what it shipped with, so a file that
  // changes one key stays one line long.
  assert.equal(map.byBinding.get("mod+f"), "find");
});

test("a line that cannot be used is named, and the rest of the file still lands", () => {
  const map = mac.buildKeymap({ "next-page": ["n", "mod+wibble"], "eat-lunch": ["x"] });
  assert.equal(map.byBinding.get("n"), "next-page");
  assert.equal(map.problems.length, 2);
  assert.ok(map.problems.some((p) => p.includes("mod+wibble")));
  assert.ok(map.problems.some((p) => p.includes("eat-lunch")));
});

test("a key given to two things goes to the one the reader wrote", () => {
  const map = mac.buildKeymap({ "next-page": ["mod+f"] });
  assert.equal(map.byBinding.get("mod+f"), "next-page");
  assert.deepEqual(map.byAction.get("find"), []);
  assert.equal(map.problems.length, 1);
  assert.ok(map.problems[0].includes("find"), map.problems[0]);
});

test("a sequence waits, and never waits behind a key that already does something", () => {
  const shipped = mac.buildKeymap();
  assert.ok(shipped.prefixes.has("g"));
  assert.equal(shipped.byBinding.get("g g"), "first-page");
  // Which is why the page field is on p: g has to be free to wait.
  assert.equal(shipped.byBinding.get("p"), "go-to-page");

  // Give g to something and g g becomes unreachable — a key cannot both act
  // at once and wait to see what follows it. The shorter one keeps it.
  const clash = mac.buildKeymap({ "go-to-page": ["g"] });
  assert.equal(clash.byBinding.get("g g"), undefined);
  assert.equal(clash.byBinding.get("g"), "go-to-page");
  assert.equal(clash.problems.length, 1);
  assert.ok(clash.problems[0].includes("can never be pressed"), clash.problems[0]);
});

test("what needs a document open is only what moves around inside one", () => {
  assert.equal(mac.needsDocument("scroll-down"), true);
  assert.equal(mac.needsDocument("select-page"), true);
  assert.equal(mac.needsDocument("open"), false);
  assert.equal(mac.needsDocument("dismiss"), false);
});

/* ------------------------------------------------------------- in words */

test("a chord reads the way the platform writes it", () => {
  assert.equal(mac.describeChord("mod+shift+f"), "⇧⌘F");
  assert.equal(pc.describeChord("mod+shift+f"), "Ctrl+Shift+F");
  assert.equal(mac.describeChord("alt+left"), "⌥←");
  assert.equal(mac.describeChord("mod+ctrl+f"), "⌃⌘F");
  assert.equal(mac.describeChord("f11"), "F11");
  // A bare shift and a letter is the letter: nobody reads G as ⇧G.
  assert.equal(mac.describeChord("shift+g"), "G");
  assert.equal(mac.describeChord("j"), "j");
  assert.equal(mac.describeBinding("g g"), "g g");
});

/** Two windows, and the three keys that came with them.
 *
 * ⌘N is the one shortcut this app added that every other application already
 * has, so it is worth knowing it is not quietly colliding with something. The
 * other two are the split: ⌘W used to close the app because closing the window
 * *was* closing the app, and with more than one window that is no longer true
 * — so Quit needed a key that is not "whichever window has the keyboard". Both
 * are empty on a Mac, where AppKit answers them before the page does. */
test("a second window has a key, and closing one is not quitting", () => {
  const shipped = mac.buildKeymap();
  assert.deepEqual(shipped.problems, [], "one of these took a key something else wanted");
  assert.equal(shipped.byBinding.get("mod+n"), "new-window");

  assert.deepEqual(mac.defaultKeys(mac.ACTIONS.find((a) => a.id === "close-window")), []);
  assert.deepEqual(mac.defaultKeys(mac.ACTIONS.find((a) => a.id === "quit")), []);

  const elsewhere = pc.buildKeymap();
  assert.equal(elsewhere.byBinding.get("mod+w"), "close-window");
  assert.equal(elsewhere.byBinding.get("mod+q"), "quit");
  assert.equal(elsewhere.byBinding.get("mod+n"), "new-window");
});

/* ------------------------------------------------------------- the file */

test("the shipped keys.toml shows the keys the app actually ships with", () => {
  // The template is the first thing a reader sees when they open the file, and
  // a template that quotes a key the app no longer uses is worse than one that
  // quotes none: it is the same drift `tests/settings.test.mjs` watches for
  // between the two copies of the settings table.
  const body = readFileSync("src-tauri/keys.toml", "utf8");
  const shown = new Map();
  for (const line of body.split("\n")) {
    const found = /^# ([a-z-]+) = (\[.*\])$/.exec(line);
    if (found) shown.set(found[1], JSON.parse(found[2]));
  }

  for (const spec of mac.ACTIONS) {
    assert.ok(shown.has(spec.id), `${spec.id} is not in keys.toml`);
    assert.deepEqual(shown.get(spec.id), spec.keys, `keys.toml disagrees about ${spec.id}`);
  }
  for (const name of shown.keys()) {
    assert.ok(
      mac.ACTIONS.some((spec) => spec.id === name),
      `keys.toml offers ${name}, which HyloPDF cannot do`,
    );
  }
});

/* ----------------------------------------------------- and in a document */

test("a rebound key reaches the document", async (t) => {
  const app = await openApp({
    pdf: "tests/fixtures/book.pdf",
    keys: { "next-page": ["n"], "scroll-down": [] },
  });
  t.after(() => app.close());

  const reaches = async (page) => {
    await app.page
      .waitForFunction((p) => document.getElementById("page-number")?.value === p, String(page), {
        timeout: 15_000,
        polling: 50,
      })
      .catch(() => {});
    assert.equal((await app.state()).page, String(page));
  };

  await t.test("the key the reader gave it turns the page", async () => {
    await app.press("n");
    await reaches(2);
  });

  await t.test("the key it replaced does not", async () => {
    await app.press("ArrowRight");
    await app.page.waitForTimeout(200);
    assert.equal((await app.state()).page, "2");
  });

  await t.test("an action unbound does nothing at all", async () => {
    const before = await app.page.evaluate(() => document.getElementById("viewer").scrollTop);
    await app.press("j");
    await app.page.waitForTimeout(200);
    const after = await app.page.evaluate(() => document.getElementById("viewer").scrollTop);
    assert.equal(after, before);
  });

  await t.test("everything the file did not mention still works", async () => {
    await app.press("End");
    await reaches(400);
  });
});

test("the keys a Vim-shaped reader reaches for", async (t) => {
  const app = await openApp({ pdf: "tests/fixtures/book.pdf" });
  t.after(() => app.close());

  const reaches = async (page) => {
    await app.page
      .waitForFunction((p) => document.getElementById("page-number")?.value === p, String(page), {
        timeout: 15_000,
        polling: 50,
      })
      .catch(() => {});
    assert.equal((await app.state()).page, String(page));
  };

  await t.test("h and l turn pages, the same as the arrows beside them", async () => {
    await app.press("l");
    await reaches(2);
    await app.press("h");
    await reaches(1);
  });

  await t.test("gg and G are the two ends", async () => {
    await app.press("Shift+g");
    await reaches(400);
    // Two presses, one binding: the first g waits to see what follows it.
    await app.press("g");
    await app.press("g");
    await reaches(1);
  });

  await t.test("a g that leads nowhere is dropped rather than left waiting", async () => {
    await app.press("g");
    await app.press("l");
    await reaches(2);
    await app.press("h");
    await reaches(1);
  });

  await t.test("d and u are half a screen, which is half of what Space moves", async () => {
    const at = () => app.page.evaluate(() => document.getElementById("viewer").scrollTop);
    const start = await at();
    await app.press("d");
    await app.page.waitForTimeout(200);
    const half = (await at()) - start;
    await app.press("u");
    await app.page.waitForTimeout(200);
    assert.equal(await at(), start, "u did not put it back");

    await app.press("Space");
    await app.page.waitForTimeout(200);
    const whole = (await at()) - start;
    assert.ok(half > 0, "d did not move");
    assert.ok(Math.abs(whole - half * 2) < 3, `${half} then ${whole}`);
  });
});
