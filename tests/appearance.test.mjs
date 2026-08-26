/* Following the machine's own light and dark.
 *
 * An app that stays white while everything around it has gone dark at sunset
 * is the thing every reader notices, and the app had no answer to it: a theme
 * and a switch, both moved by hand. */

import test from "node:test";
import assert from "node:assert/strict";
import { MOD, openApp } from "../scripts/ui-harness.mjs";

/** The theme in force, read the way a reader would read it: the ticked entry
    in the Theme menu. Reading the setting instead would say nothing on the
    run where the theme never had to be written down. */
async function themeOf(app) {
  await app.page.click("#theme");
  await app.page.waitForTimeout(150);
  const name = await app.page.evaluate(
    () =>
      document.querySelector("#popovers .popover-item.current .popover-label")?.textContent ??
      null,
  );
  await app.press("Escape");
  await app.page.waitForTimeout(100);
  return name;
}

test("a machine in dark mode gets the dark theme", async () => {
  const app = await openApp({ appearance: "dark" });
  try {
    assert.equal(await themeOf(app), "Hylo Dark");
  } finally {
    await app.close();
  }
});

test("a machine in light mode gets the light one", async () => {
  const app = await openApp({ appearance: "light" });
  try {
    assert.equal(await themeOf(app), "Hylo Light");
  } finally {
    await app.close();
  }
});

test("and it changes its mind when the machine does", async () => {
  const app = await openApp({ appearance: "light" });
  try {
    const day = (await app.state()).paper;
    await app.setAppearance("dark");
    assert.equal(await themeOf(app), "Hylo Dark");
    const night = (await app.state()).paper;
    assert.notEqual(night, day, "the page kept its daytime paper");

    await app.setAppearance("light");
    assert.equal(await themeOf(app), "Hylo Light");
  } finally {
    await app.close();
  }
});

test("the two slots are the reader's own, not the defaults", async () => {
  const app = await openApp({
    appearance: "light",
    settings: { light_theme: "sepia", dark_theme: "nord", theme: "sepia" },
  });
  try {
    assert.equal(await themeOf(app), "Sepia");
    await app.setAppearance("dark");
    assert.equal(await themeOf(app), "Nord");
  } finally {
    await app.close();
  }
});

test("choosing a theme that disagrees with the machine stops the following", async () => {
  const app = await openApp({ appearance: "light" });
  try {
    // ⌘D in the daytime: the reader has overruled the system, and leaving the
    // switch on would let the next thing the system did take it back off them.
    // `MOD`, never a hard-coded Meta: the app takes its whole scheme from the
    // platform, so a test that names one passes here and does nothing at all
    // under `HYLOPDF_PLATFORM=other` or on CI.
    await app.press(`${MOD}+KeyD`);
    await app.page.waitForTimeout(300);
    assert.equal(await themeOf(app), "Hylo Dark");
    const following = await app.page.evaluate(
      () => JSON.parse(localStorage.getItem("hylopdf.settings") || "{}").follow_system_theme,
    );
    assert.equal(following, false);

    // And the system changing its mind now leaves them where they are.
    await app.setAppearance("light");
    assert.equal(await themeOf(app), "Hylo Dark");
  } finally {
    await app.close();
  }
});
