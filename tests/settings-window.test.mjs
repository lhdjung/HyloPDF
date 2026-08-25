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
  const stillEditing = await app.page.evaluate(async () => {
    const settings = await import("/src/settings.ts");
    return settings.isEditingTheme();
  });
  assert.equal(stillEditing, true, "the editor does not admit to being open");

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

  const stillEditing = await app.page.evaluate(async () => {
    const settings = await import("/src/settings.ts");
    return settings.isEditingTheme();
  });
  assert.equal(stillEditing, false, "the editor is still open with no window");
});
