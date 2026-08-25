/* The Settings window, driven headlessly.
 *
 * `settings.ts` had no tests either, and the theme editor is the part of it
 * with state: a draft is installed as the live theme so that the app around
 * you is the preview, which means for as long as the editor is open the theme
 * in use is one that does not exist on disk and — for a new theme — has no id
 * at all. Everything below is about that window staying closed over its draft.
 *
 * Needs a dev server, like `reader.test.mjs`. `npm test` starts one.
 */

import test from "node:test";
import assert from "node:assert/strict";
import { MOD, openApp } from "../scripts/ui-harness.mjs";

let app;

test.before(async () => {
  app = await openApp();
});

test.after(async () => {
  await app?.close();
});

/** The colour the chrome is currently painted in. */
const background = () =>
  app.page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue("--bg").trim(),
  );

const openSettings = async () => {
  await app.press(`${MOD}+,`);
  await app.page.waitForSelector("#windows .window", { timeout: 10_000 });
};

/** `isEditingTheme`, asked of the module the app is actually running.
 *
 *  Importing "/src/settings.ts" by name is not the same module: a dev server
 *  that has hot-reloaded the file serves it as "/src/settings.ts?t=<stamp>",
 *  and a second URL is a second instance with its own module-scope state. The
 *  flag then reads false for the perfectly good reason that nothing ever set
 *  it — on a server started for this run it passes, and on one that has been
 *  open while somebody edited the file it fails. So the URL is taken from what
 *  the page already loaded. */
const editingTheme = () =>
  app.page.evaluate(async () => {
    const loaded = performance
      .getEntriesByType("resource")
      .map((entry) => entry.name)
      .filter((name) => /\/src\/settings\.ts(\?|$)/.test(name));
    const settings = await import(loaded.at(-1) ?? "/src/settings.ts");
    return settings.isEditingTheme();
  });

const closeSettings = async () => {
  await app.page.keyboard.press("Escape");
  await app.page.waitForFunction(
    () => document.querySelectorAll("#windows .window").length === 0,
    null,
    { timeout: 10_000 },
  );
};

/** Walk to a pane by the name on its button in the nav column. */
const goToPane = async (label) => {
  await app.page.click(`#windows .window-nav button:text-is("${label}")`);
  await app.page.waitForTimeout(200);
};

test("the window opens on a pane and closes on Escape", async () => {
  await openSettings();
  assert.equal(await app.page.locator("#windows .window").count(), 1);
  await closeSettings();
  assert.equal(await app.page.locator("#windows .window").count(), 0);
});

test("a theme's colours apply as they are picked", async () => {
  await openSettings();
  await goToPane("Appearance");

  const before = await background();
  // "New theme…" starts a draft from whatever is in use.
  await app.page.click('#windows .window-pane button:text-is("New theme…")');
  await app.page.waitForSelector('#windows input[type="color"]', { timeout: 10_000 });

  // The second colour field is the background; the app follows it live.
  await app.page.evaluate(() => {
    const fields = [...document.querySelectorAll('#windows input[type="text"]')];
    // name, text, background, accent, link, selection area, selection ink
    const backgroundField = fields[2];
    backgroundField.value = "#3b2a1e";
    backgroundField.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await app.page.waitForTimeout(200);

  const during = await background();
  assert.notEqual(during, before, "the draft did not reach the app around it");

  // Cancelling puts back what was in use.
  await app.page.click('#windows .window-pane button:text-is("Cancel")');
  await app.page.waitForTimeout(200);
  assert.equal(await background(), before, "cancelling did not put the theme back");
  await closeSettings();
});

test("the editor says it is open, which is what protects the draft", async () => {
  await openSettings();
  await goToPane("Appearance");
  await app.page.click('#windows .window-pane button:text-is("New theme…")');
  await app.page.waitForSelector('#windows input[type="color"]', { timeout: 10_000 });

  await app.page.evaluate(() => {
    const fields = [...document.querySelectorAll('#windows input[type="text"]')];
    fields[2].value = "#123456";
    fields[2].dispatchEvent(new Event("input", { bubbles: true }));
  });
  await app.page.waitForTimeout(200);
  assert.notEqual(await background(), "", "the draft is live");

  // `themesChanged` cannot be driven from here — it hangs off a Tauri event,
  // and there is no Tauri behind the browser path — so what is checked is the
  // question it asks. A draft has no id, so the watcher looking the live theme
  // up in the new set finds nothing and used to read that as "your theme was
  // deleted": preview thrown away, replacement chosen, choice written to
  // settings, and a notice saying so. This flag is the whole of what stops it,
  // and it has to be true exactly while a draft is live.
  assert.equal(await editingTheme(), true, "the editor does not admit to being open");

  await closeSettings();
});

test("backing out of the window puts the theme back too", async () => {
  const before = await background();
  await openSettings();
  await goToPane("Appearance");
  await app.page.click('#windows .window-pane button:text-is("New theme…")');
  await app.page.waitForSelector('#windows input[type="color"]', { timeout: 10_000 });

  await app.page.evaluate(() => {
    const fields = [...document.querySelectorAll('#windows input[type="text"]')];
    fields[2].value = "#402030";
    fields[2].dispatchEvent(new Event("input", { bubbles: true }));
  });
  await app.page.waitForTimeout(200);
  assert.notEqual(await background(), before);

  // Escape, rather than Cancel: closing the window is the third way out of an
  // edit and has to undo it like the other two.
  await closeSettings();
  await app.page.waitForTimeout(200);
  assert.equal(await background(), before, "the draft outlived the window");

  assert.equal(await editingTheme(), false, "the editor is still open with no window");
});

/* A stepper's readout is a field, and the bug it was written to close is a
 * click: with the unit inside the field the caret landed in the middle of
 * "16 px" and typing 30 gave "3016 px", which clamps to the maximum. So what
 * is checked is the whole gesture — click, type, Enter — rather than the value
 * being settable at all, which it always was. */
test("a stepper takes a typed number, by click and keyboard alone", async () => {
  await openSettings();
  await goToPane("Reading");

  const field = app.page.locator("#windows .stepper-field input").first();
  await field.click();
  await app.page.keyboard.type("30");
  await app.page.keyboard.press("Enter");

  await app.page
    .waitForFunction(
      () => document.querySelector("#windows .stepper-field input")?.value === "30",
      null,
      { timeout: 10_000 },
    )
    .catch(() => {});
  assert.equal(await field.inputValue(), "30", "typing over the number did not replace it");

  // The unit is a label beside the number, not part of what can be typed.
  assert.equal(
    await app.page.locator("#windows .stepper-field .stepper-unit").first().textContent(),
    "px",
  );

  // And it is clamped rather than snapped: 30 is not a multiple of the step,
  // and 900 is past the end of the range.
  await field.click();
  await app.page.keyboard.type("900");
  await app.page.keyboard.press("Enter");
  await app.page
    .waitForFunction(
      () => document.querySelector("#windows .stepper-field input")?.value === "64",
      null,
      { timeout: 10_000 },
    )
    .catch(() => {});
  assert.equal(await field.inputValue(), "64", "a number past the end was not clamped");

  await closeSettings();
});
