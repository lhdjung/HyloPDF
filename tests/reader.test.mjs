/* The reader itself, driven headlessly.
 *
 * These go through `ui-harness.mjs`, which is the browser fallback in `api.ts`
 * — no Rust, no window. That covers everything the interface does and nothing
 * about the window it sits in; full screen, the title bar and the drag regions
 * still have to be looked at in the real app.
 *
 * Needs a dev server. `npm test` starts one; `node --test "tests/*.test.mjs"` on its own
 * expects `npm run dev` to already be running. */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { openApp } from "../scripts/ui-harness.mjs";

const PDF = "tests/fixtures/book.pdf";
const PAGES = 400;

if (!existsSync(PDF)) {
  throw new Error(`missing ${PDF} — run: node tests/fixtures/make-pdf.mjs ${PDF} ${PAGES}`);
}

let app;

test.before(async () => {
  app = await openApp({ pdf: PDF });
});

test.after(async () => {
  await app?.close();
});

test("a document opens and knows how long it is", async () => {
  const state = await app.state();
  assert.equal(state.pages, `of ${PAGES}`);
  assert.equal(state.page, "1");
});

test("the scrollbar tells the truth about the whole book", async () => {
  // Pages beyond the first are measured in the background, so the layout
  // settles shortly after the document opens rather than before it appears.
  await app.page.waitForTimeout(1500);
  const height = await app.page.evaluate(
    () => document.getElementById("pages").offsetHeight,
  );
  // Four hundred pages of a page each; the exact height depends on the fit.
  assert.ok(height > PAGES * 500, `scroll height was only ${height}px`);
});

test("pages are drawn", async () => {
  const drawn = await app.page.evaluate(
    () => [...document.querySelectorAll(".page canvas")].filter((c) => c.width > 1).length,
  );
  assert.ok(drawn > 0, "no page was painted");
});

test("only the pages near the viewport exist", async () => {
  const mounted = await app.page.evaluate(
    () => document.querySelectorAll("#pages .page").length,
  );
  assert.ok(mounted > 0 && mounted < 12, `${mounted} pages were in the DOM`);
});

test("moving around", async (t) => {
  await t.test("End reaches the last page", async () => {
    await app.press("End");
    assert.equal((await app.state()).page, String(PAGES));
  });

  await t.test("Home comes back", async () => {
    await app.press("Home");
    assert.equal((await app.state()).page, "1");
  });

  await t.test("the arrow keys turn pages", async () => {
    await app.press("ArrowRight");
    assert.equal((await app.state()).page, "2");
    await app.press("ArrowLeft");
    assert.equal((await app.state()).page, "1");
  });

  await t.test("a shortcut hands over the page number, ready to type into", async () => {
    for (const keys of ["g", "Meta+Alt+g"]) {
      await app.press("End");
      await app.page.keyboard.press(keys);
      await app.page.waitForTimeout(120);
      const held = await app.page.evaluate(() => {
        const field = document.getElementById("page-number");
        return {
          focused: document.activeElement === field,
          selected: field.value.slice(field.selectionStart, field.selectionEnd),
        };
      });
      assert.ok(held.focused, `${keys} did not reach the page number`);
      assert.equal(held.selected, String(PAGES), `${keys} left the number unselected`);

      // What it is for: the number typed over the top of it goes there.
      await app.page.keyboard.type("42");
      await app.page.keyboard.press("Enter");
      await app.page.waitForTimeout(200);
      assert.equal((await app.state()).page, "42");
    }
    await app.press("Home");
  });

  await t.test("a turned page starts at the top of the window", async () => {
    await app.press("ArrowRight");
    const { above, before } = await app.page.evaluate(() => {
      const viewer = document.getElementById("viewer");
      const top = (n) =>
        document.querySelector(`.page[data-page="${n}"]`)?.getBoundingClientRect().top -
        viewer.getBoundingClientRect().top;
      return { above: top(2), before: top(1) + document.querySelector('.page[data-page="1"]').offsetHeight };
    });
    // The gap above the page is at the top of the window, and the page before
    // it ends exactly where the window starts rather than hanging into it.
    assert.ok(above > 0 && above < 40, `page two started ${above}px down`);
    assert.ok(Math.abs(before) < 1.5, `page one still showed ${before}px`);
    await app.press("ArrowLeft");
  });
});

test("fit width fits the width", async (t) => {
  const strips = () =>
    app.page.evaluate(() => {
      const viewer = document.getElementById("viewer").getBoundingClientRect();
      const page = document.querySelector("#pages .page").getBoundingClientRect();
      return { left: page.left - viewer.left, right: viewer.right - page.right };
    });

  await t.test("with nothing beside it", async () => {
    const { left, right } = await strips();
    assert.ok(left < 1 && right < 1, `${left}px and ${right}px of ground left over`);
  });

  await t.test("with the sidebar out", async () => {
    await app.page.keyboard.press("Meta+b");
    await app.page.waitForTimeout(400);
    const { left, right } = await strips();
    assert.ok(left < 1 && right < 1, `${left}px and ${right}px of ground left over`);
    await app.page.keyboard.press("Meta+b");
    await app.page.waitForTimeout(400);
  });

  await t.test("and does not put a sideways scrollbar under it", async () => {
    const over = await app.page.evaluate(() => {
      const viewer = document.getElementById("viewer");
      return viewer.scrollWidth - viewer.clientWidth;
    });
    assert.equal(over, 0, `${over}px wider than the window`);
  });
});

test("ctrl+wheel zooms", async () => {
  const before = (await app.state()).zoom;
  await app.wheel(4, -40, { ctrl: true });
  const after = (await app.state()).zoom;
  assert.notEqual(after, before);
  assert.match(after, /%$/);
});

test("search", async (t) => {
  await t.test("finds matches and highlights them", async () => {
    await app.page.keyboard.press("Meta+f");
    await app.page.waitForTimeout(150);
    await app.page.fill("#find-input", "quick brown");
    await app.page.waitForTimeout(2500);

    const state = await app.state();
    assert.match(state.findStatus ?? "", /\d+ of \d+/);

    const marks = await app.page.evaluate(
      () => document.querySelectorAll(".find-highlight").length,
    );
    assert.ok(marks > 0, "matches were counted but not shown");
  });

  await t.test("only the current match is marked when highlight all is off", async () => {
    const all = await app.page.evaluate(
      () => document.querySelectorAll(".find-highlight").length,
    );
    assert.ok(all > 1, "the fixture should have more than one match on screen");

    await app.page.click("#find-highlight");
    await app.page.waitForTimeout(200);
    const one = await app.page.evaluate(
      () => document.querySelectorAll(".find-highlight").length,
    );
    // One match, but a match spanning two lines is two rectangles, so the
    // test is that it collapsed rather than that it collapsed to exactly one.
    assert.ok(one < all, `${one} marks left of ${all}`);
    assert.ok(
      await app.page.evaluate(
        () => document.querySelectorAll(".find-highlight.current").length > 0,
      ),
      "the match being read stopped being marked",
    );

    await app.page.click("#find-highlight");
    await app.page.waitForTimeout(200);
  });

  await t.test("whole words drops the matches inside longer words", async () => {
    // Every "he" in the fixture is inside "the", and every "row" inside
    // "brown", so whole words leaves the query with nothing.
    await app.page.fill("#find-input", "row");
    await app.page.waitForTimeout(2500);
    assert.match((await app.state()).findStatus ?? "", /\d+ of \d+/);

    await app.page.click("#find-words");
    await app.page.waitForTimeout(2500);
    assert.equal((await app.state()).findStatus, "None");

    await app.page.click("#find-words");
    await app.page.waitForTimeout(2500);
    assert.match((await app.state()).findStatus ?? "", /\d+ of \d+/);
  });

  await t.test("match case takes the query at its word", async () => {
    // The fixture writes "Page" and never "page", so the same six letters
    // find everything or nothing depending on this switch alone.
    await app.page.fill("#find-input", "page");
    await app.page.waitForTimeout(2500);
    assert.match((await app.state()).findStatus ?? "", /\d+ of \d+/);

    await app.page.click("#find-case");
    await app.page.waitForTimeout(2500);
    assert.equal((await app.state()).findStatus, "None");

    await app.page.fill("#find-input", "Page");
    await app.page.waitForTimeout(2500);
    assert.match((await app.state()).findStatus ?? "", /\d+ of \d+/);

    await app.page.click("#find-case");
    await app.page.waitForTimeout(2500);
  });

  await t.test("clicking the document puts it away", async () => {
    assert.equal((await app.state()).findOpen, true);
    await app.page.mouse.click(640, 600);
    await app.page.waitForTimeout(150);
    assert.equal((await app.state()).findOpen, false);
  });

  await t.test("Escape puts it away", async () => {
    await app.page.keyboard.press("Meta+f");
    await app.page.waitForTimeout(150);
    assert.equal((await app.state()).findOpen, true);
    await app.page.keyboard.press("Escape");
    assert.equal((await app.state()).findOpen, false);
  });
});

test("menus answer the keyboard", async (t) => {
  await t.test("opening one from the keyboard moves the focus into it", async () => {
    await app.page.locator("#theme").focus();
    await app.page.keyboard.press("Enter");
    await app.page.waitForTimeout(200);

    assert.equal((await app.state()).menuOpen, true);
    const inside = await app.page.evaluate(
      () => document.getElementById("popovers").contains(document.activeElement),
    );
    assert.ok(inside, "the menu opened without taking the keyboard");
  });

  await t.test("the arrow keys move through it", async () => {
    const before = await app.page.evaluate(() => document.activeElement?.textContent);
    await app.page.keyboard.press("ArrowDown");
    const after = await app.page.evaluate(() => document.activeElement?.textContent);
    assert.notEqual(after, before);
  });

  await t.test("Escape closes it", async () => {
    await app.page.keyboard.press("Escape");
    assert.equal((await app.state()).menuOpen, false);
  });
});

test("selected text is painted by the theme, not by the engine", async () => {
  // WebKit will not paint a ::selection background at the colour it was given,
  // so the viewer says the selection a second time as a custom highlight and
  // the stylesheet spends the theme's colours there. If this stops being
  // registered, selection quietly goes back to a wash over the printed words.
  const selected = await app.page.evaluate(() => {
    const span = document.querySelector("#pages .textLayer span");
    const range = document.createRange();
    range.setStart(span.firstChild, 0);
    range.setEnd(span.firstChild, 6);
    const selection = getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
    return range.toString();
  });
  await app.page.waitForTimeout(100);

  const painted = await app.page.evaluate(() => {
    const highlight = CSS.highlights.get("document-selection");
    if (!highlight) return null;
    return [...highlight].map((range) => range.toString());
  });
  assert.deepEqual(painted, [selected]);

  await app.page.evaluate(() => getSelection().removeAllRanges());
  await app.page.waitForTimeout(100);
  assert.equal(
    await app.page.evaluate(() => CSS.highlights.has("document-selection")),
    false,
    "the highlight outlived the selection",
  );
});

test("a colour changed in the editor reaches the page it recolours", async () => {
  // Its own window: this one has to be reading under a theme that recolours,
  // and the shared app is on the light theme, which by design does not.
  const editing = await openApp({ pdf: PDF, settings: { theme: "hylo-dark", sidebar: false } });
  try {
    const paper = () =>
      editing.page.evaluate(() => {
        const canvas = document.querySelector("#pages canvas");
        const [r, g, b] = canvas.getContext("2d").getImageData(4, 4, 1, 1).data;
        return `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
      });
    await editing.page.waitForTimeout(1200);
    assert.equal(await paper(), "#24272f", "the page did not open in the theme's colours");

    await editing.page.keyboard.press("Meta+Comma");
    await editing.page.waitForTimeout(300);
    await editing.page.click("#windows .window-nav button:has-text('Appearance')");
    await editing.page.click("#windows .pane-actions button:has-text('Make a copy')");
    // Let the copy settle before touching it. Starting an edit already repaints
    // once — the draft arrives under an id of its own — and a colour typed
    // before that repaint has run would be picked up by it, which is a page
    // that is right for the wrong reason.
    await editing.page.waitForTimeout(600);
    await editing.page
      .locator('#windows .field:has(.field-label:text-is("Background")) input[type=text]')
      .fill("#3a0a0a");
    await editing.page.waitForTimeout(800);

    // The editor previews by handing the viewer the draft it goes on editing,
    // so a viewer that kept the object rather than a copy of it would find
    // nothing had changed and leave the page as it was printed.
    assert.equal(await paper(), "#3a0a0a", "the page kept the old background");
  } finally {
    await editing.close();
  }
});

test("the way out is on the start screen and nowhere else", async () => {
  const seen = () =>
    app.page.evaluate(() => {
      const button = document.getElementById("welcome-quit");
      return button.getBoundingClientRect().width > 0 && button.offsetParent !== null;
    });

  // A document is open by the time this runs; the start screen is behind it.
  assert.equal(await seen(), false, "the quit button showed over a document");

  await app.page.click("#close-doc");
  await app.page.waitForTimeout(300);
  assert.equal((await app.state()).onStartScreen, true);
  assert.equal(await seen(), true, "no way out of the start screen");
});

test("nothing went wrong on the way", () => {
  const noise = app.logs.filter(
    (line) =>
      /pageerror|error:/i.test(line) &&
      // pdf.js says this when a document leans on a font it has to fetch and
      // the fixture, which embeds nothing, does. Not the app's doing.
      !/standardFontDataUrl/.test(line),
  );
  assert.deepEqual(noise, []);
});
