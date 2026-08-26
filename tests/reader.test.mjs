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
import { MOD, openApp } from "../scripts/ui-harness.mjs";

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
  // settles shortly after the document opens rather than before it appears —
  // and how shortly is four hundred page proxies' worth of the machine's time,
  // not a number that can be written down here.
  const tall = PAGES * 500;
  await app.page
    .waitForFunction(
      (want) => document.getElementById("pages").offsetHeight > want,
      tall,
      { timeout: 30_000, polling: 100 },
    )
    .catch(() => {});
  const height = await app.page.evaluate(
    () => document.getElementById("pages").offsetHeight,
  );
  // Four hundred pages of a page each; the exact height depends on the fit.
  assert.ok(height > tall, `scroll height was only ${height}px`);
});

test("pages are drawn", async () => {
  // Waited for here rather than inherited: this used to run late enough to
  // find a painted page only because the test before it slept, and that test
  // now waits on the layout, which settles well before any pixel does.
  await app.page
    .waitForFunction(
      () => [...document.querySelectorAll(".page canvas")].some((c) => c.width > 1),
      null,
      { timeout: 30_000, polling: 100 },
    )
    .catch(() => {});
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
  // A page turn is an instant scroll, but the remount that follows it is not:
  // only the pages near the viewport are in the DOM, so jumping the length of
  // the book means discarding one neighbourhood and building another, and how
  // long that takes belongs to the machine. `press` waits a fixed moment,
  // which is enough here and was not enough on CI. So the page number is
  // waited for, and the assertion still reports the page it stopped on.
  const reaches = async (want) => {
    await app.page
      .waitForFunction((n) => document.getElementById("page-number")?.value === String(n), want, {
        timeout: 15_000,
        polling: 50,
      })
      .catch(() => {});
    assert.equal((await app.state()).page, String(want));
  };

  await t.test("End reaches the last page", async () => {
    await app.press("End");
    await reaches(PAGES);
  });

  await t.test("Home comes back", async () => {
    await app.press("Home");
    await reaches(1);
  });

  await t.test("the arrow keys turn pages", async () => {
    await app.press("ArrowRight");
    await reaches(2);
    await app.press("ArrowLeft");
    await reaches(1);
  });

  await t.test("a shortcut hands over the page number, ready to type into", async () => {
    for (const keys of ["g", `${MOD}+Alt+g`]) {
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
    // The subtest before this one jumped to page 42 and came back, so both the
    // page turned from and the page turned to have to be back in the DOM
    // before there is anything to measure.
    await reaches(1);
    await app.press("ArrowRight");
    await reaches(2);
    await app.page.waitForFunction(
      () =>
        document.querySelector('.page[data-page="1"]') &&
        document.querySelector('.page[data-page="2"]'),
      null,
      { timeout: 15_000, polling: 50 },
    );
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
    await reaches(1);
  });
});

test("stepping back out of a jump", async (t) => {
  const reaches = async (want) => {
    await app.page
      .waitForFunction((n) => document.getElementById("page-number")?.value === String(n), want, {
        timeout: 15_000,
        polling: 50,
      })
      .catch(() => {});
    assert.equal((await app.state()).page, String(want));
  };

  await t.test("back returns to where the jump started", async () => {
    await app.press("Home");
    await reaches(1);
    await app.press("End");
    await reaches(PAGES);
    await app.page.keyboard.press(`${MOD}+BracketLeft`);
    await reaches(1);
  });

  await t.test("and forward goes there again", async () => {
    await app.page.keyboard.press(`${MOD}+BracketRight`);
    await reaches(PAGES);
  });

  await t.test("Alt+arrow does the same", async () => {
    await app.page.keyboard.press("Alt+ArrowLeft");
    await reaches(1);
    await app.page.keyboard.press("Alt+ArrowRight");
    await reaches(PAGES);
  });

  await t.test("turning a page is movement, not a jump", async () => {
    // Two page turns and a scroll, none of which may leave a trace: a history
    // of the last twenty keystrokes is no use to anybody.
    await app.press("ArrowLeft");
    await reaches(PAGES - 1);
    await app.press("ArrowLeft");
    await reaches(PAGES - 2);
    await app.page.keyboard.press("Alt+ArrowLeft");
    await reaches(1);
    await app.press("Home");
  });
});

test("turning the document", async (t) => {
  const shape = () =>
    app.page.evaluate(() => {
      const canvas = document.querySelector("#pages canvas");
      const page = document.querySelector("#pages .page");
      return canvas && page
        ? { canvas: canvas.width / canvas.height, page: page.offsetWidth / page.offsetHeight }
        : null;
    });

  // The box turns on the next layout and the bitmap on the next render, and
  // the second is the one that takes the machine's time — so the wait is for
  // the canvas, or the first assertion below passes on a page that is still a
  // picture of the old orientation.
  const settles = async (want, message) => {
    await app.page
      .waitForFunction(
        (portrait) => {
          const canvas = document.querySelector("#pages canvas");
          const page = document.querySelector("#pages .page");
          if (!canvas || !page) return false;
          return (
            portrait === canvas.width < canvas.height &&
            portrait === page.offsetWidth < page.offsetHeight
          );
        },
        want,
        { timeout: 15_000, polling: 50 },
      )
      .catch(() => {});
    const now = await shape();
    assert.equal(now.page < 1, want, `${message} — page ratio was ${now.page.toFixed(2)}`);
    // The bitmap has to turn with the box it is drawn into, or the page is
    // a picture of something else stretched to fit.
    assert.equal(now.canvas < 1, want, `${message} — canvas ratio was ${now.canvas.toFixed(2)}`);
  };

  await t.test("a page starts up the right way", async () => {
    await settles(true, "a fresh document was not portrait");
  });

  await t.test("a quarter turn lays it on its side", async () => {
    await app.page.keyboard.press(`${MOD}+KeyR`);
    await settles(false, "the page did not turn");
  });

  await t.test("and turning back straightens it", async () => {
    await app.page.keyboard.press(`${MOD}+KeyL`);
    await settles(true, "the page did not come back");
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
    // The sidebar opening relays the pages out, and the relayout is a frame or
    // several rather than a fixed 400ms. Waited for both ways round: the
    // subtest after this one measures the window the sidebar has just left.
    const framed = () =>
      app.page
        .waitForFunction(
          () => {
            const viewer = document.getElementById("viewer").getBoundingClientRect();
            const page = document.querySelector("#pages .page")?.getBoundingClientRect();
            return !!page && page.left - viewer.left < 1 && viewer.right - page.right < 1;
          },
          null,
          { timeout: 15_000, polling: 50 },
        )
        .catch(() => {});

    await app.page.keyboard.press(`${MOD}+b`);
    await framed();
    const { left, right } = await strips();
    assert.ok(left < 1 && right < 1, `${left}px and ${right}px of ground left over`);
    await app.page.keyboard.press(`${MOD}+b`);
    await framed();
  });

  await t.test("and does not put a sideways scrollbar under it", async () => {
    const over = await app.page.evaluate(() => {
      const viewer = document.getElementById("viewer");
      return viewer.scrollWidth - viewer.clientWidth;
    });
    assert.equal(over, 0, `${over}px wider than the window`);
  });
});

test("the zoom modes have keys of their own", async (t) => {
  const zoom = async () => (await app.state()).zoom;

  await t.test("actual size", async () => {
    await app.page.keyboard.press(`${MOD}+Digit1`);
    await app.page.waitForTimeout(300);
    assert.equal(await zoom(), "100%");
  });

  await t.test("fit page", async () => {
    await app.page.keyboard.press(`${MOD}+Digit2`);
    await app.page.waitForTimeout(300);
    assert.equal(await zoom(), "Fit page");
  });

  await t.test("and fit width, which is where the app lives", async () => {
    await app.page.keyboard.press(`${MOD}+Digit0`);
    await app.page.waitForTimeout(300);
    assert.equal(await zoom(), "Fit width");
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
  // Indexing four hundred pages takes as long as the machine takes, and the bar
  // says so while it is at it: `…` until the scan is done, then a count or
  // "None". Each step used to sleep 2.5 seconds, which was generous here and a
  // guess about CI.
  //
  // Waiting for the scan to *end* is not enough on its own — between the query
  // changing and the scan starting, the bar still holds the last answer, and a
  // wait for "not scanning" would return on it. So it waits for what the step
  // expects the bar to say, and then asserts it: a slow scan is waited out, and
  // a scan that ends somewhere else is still reported as what it ended on
  // rather than as a timeout.
  const says = async (want) => {
    const literal = typeof want === "string";
    await app.page
      .waitForFunction(
        ({ literal: exact, expected }) => {
          const text = document.getElementById("find-status")?.textContent ?? "";
          if (text === "…") return false;
          return exact ? text === expected : new RegExp(expected).test(text);
        },
        { literal, expected: literal ? want : want.source },
        { timeout: 30_000, polling: 50 },
      )
      .catch(() => {});
    const status = (await app.state()).findStatus ?? "";
    if (literal) assert.equal(status, want);
    else assert.match(status, want);
  };

  const scanned = () =>
    app.page
      .waitForFunction(
        () => (document.getElementById("find-status")?.textContent ?? "") !== "…",
        null,
        { timeout: 30_000, polling: 50 },
      )
      .catch(() => {});

  await t.test("finds matches and highlights them", async () => {
    await app.page.keyboard.press(`${MOD}+f`);
    await app.page.waitForTimeout(150);
    await app.page.fill("#find-input", "quick brown");
    await says(/\d+ of \d+/);

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
    await says(/\d+ of \d+/);

    await app.page.click("#find-words");
    await says("None");

    await app.page.click("#find-words");
    await says(/\d+ of \d+/);
  });

  await t.test("match case takes the query at its word", async () => {
    // The fixture writes "Page" and never "page", so the same six letters
    // find everything or nothing depending on this switch alone.
    await app.page.fill("#find-input", "page");
    await says(/\d+ of \d+/);

    await app.page.click("#find-case");
    await says("None");

    await app.page.fill("#find-input", "Page");
    await says(/\d+ of \d+/);

    await app.page.click("#find-case");
    await scanned();
  });

  await t.test("a common letter is found on the last page too", async () => {
    // The cap used to be two thousand and it stopped the scan, not just the
    // count — so a letter this common was indexed for the first few chapters
    // and the rest of the book was not searched at all, with a "+" in the
    // corner as the only sign of it.
    // Emptied first, and waited on. The bar holds the previous query's count
    // until the new scan produces one, so a wait for "a finished-looking
    // status" is answered by the answer to the last question — which is how
    // this test first came to assert 400 matches for the letter o.
    await app.page.fill("#find-input", "");
    await app.page
      .waitForFunction(() => document.getElementById("find-status")?.textContent === "", null, {
        timeout: 10_000,
        polling: 50,
      })
      .catch(() => {});
    await app.page.fill("#find-input", "o");
    await app.page
      .waitForFunction(
        () => {
          const status = document.getElementById("find-status")?.textContent ?? "";
          return status.length > 0 && !status.endsWith("…");
        },
        null,
        { timeout: 30_000, polling: 100 },
      )
      .catch(() => {});

    const status = await app.page.evaluate(
      () => document.getElementById("find-status")?.textContent ?? "",
    );
    const total = Number(status.split(" of ")[1]?.replace("+", ""));
    assert.ok(total > 2000, `only ${status}`);
    assert.ok(!status.includes("+"), `the count is a floor rather than a count: ${status}`);

    // And the last page really is in the index: stepping backwards from the
    // first match wraps to the last one, which is on the last page. Through
    // the button, because the keyboard belongs to the find field while the
    // caret is in it.
    await app.page.click("#find-prev");
    await app.page
      .waitForFunction((n) => document.getElementById("page-number")?.value === String(n), 400, {
        timeout: 20_000,
        polling: 50,
      })
      .catch(() => {});
    assert.equal((await app.state()).page, "400");
  });

  await t.test("every match is listed, with the line it is on", async () => {
    await app.page.fill("#find-input", "");
    await app.page
      .waitForFunction(() => document.getElementById("find-status")?.textContent === "", null, {
        timeout: 10_000,
        polling: 50,
      })
      .catch(() => {});
    await app.page.fill("#find-input", "lazy dog");
    await app.page
      .waitForFunction(
        () => {
          const status = document.getElementById("find-status")?.textContent ?? "";
          return status.length > 0 && !status.endsWith("…");
        },
        null,
        { timeout: 30_000, polling: 100 },
      )
      .catch(() => {});

    // The count is the door to the list: "3 of 128" answers "is it in here"
    // and not "which one did I mean".
    await app.page.click("#find-status");
    await app.page.waitForTimeout(300);

    const listed = await app.page.evaluate(() => {
      const results = [...document.querySelectorAll("#results-panel .result")];
      const first = results[0];
      return {
        count: results.length,
        shown: !document.getElementById("results-panel")?.hidden,
        page: first?.querySelector(".result-page")?.textContent,
        hit: first?.querySelector("mark")?.textContent,
        line: first?.querySelector(".result-line")?.textContent,
      };
    });
    assert.ok(listed.shown, "the panel did not come forward");
    assert.ok(listed.count > 1, `only ${listed.count} results listed`);
    assert.equal(listed.hit, "lazy dog");
    assert.equal(listed.page, "1");
    // A line of the document either side of it, which is the whole point.
    assert.match(listed.line, /quick brown fox/);

    // And picking one goes there.
    await app.page.click("#results-panel .result:last-of-type");
    await app.page.waitForTimeout(400);
    const at = Number((await app.state()).page);
    assert.ok(at > 1, `picking a result stayed on page ${at}`);
    // ⌘B rather than the Contents button: that button closes the find bar on
    // purpose, and the subtest after this one is about what closes it.
    await app.page.keyboard.press(`${MOD}+KeyB`);
    await app.page.waitForTimeout(200);
  });

  await t.test("clicking the document puts it away", async () => {
    assert.equal((await app.state()).findOpen, true);
    await app.page.mouse.click(640, 600);
    await app.page.waitForTimeout(150);
    assert.equal((await app.state()).findOpen, false);
  });

  await t.test("Escape puts it away", async () => {
    await app.page.keyboard.press(`${MOD}+f`);
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
    // By position, not by label: two rows running are two switches, and a
    // switch has no text in it — so comparing what the focused thing says
    // compared "" with "" and passed only for as long as the menu happened
    // not to open on one.
    const at = () =>
      app.page.evaluate(() => {
        const focusable = [
          ...document.querySelectorAll("#popovers button, #popovers [tabindex]"),
        ];
        return focusable.indexOf(document.activeElement);
      });
    const before = await at();
    await app.page.keyboard.press("ArrowDown");
    assert.notEqual(await at(), before);
  });

  await t.test("Escape closes it", async () => {
    await app.page.keyboard.press("Escape");
    assert.equal((await app.state()).menuOpen, false);
  });
});

test("the document's name is the way to another document", async () => {
  // The recently-read list used to live on the start screen and nowhere else,
  // which is the one screen a reader who is reading something cannot see.
  await app.page.click("#doc-title");
  await app.page.waitForTimeout(200);
  const items = await app.page.evaluate(() =>
    [...document.querySelectorAll("#popovers .popover-item")].map((el) => el.textContent),
  );
  assert.ok(
    items.some((label) => label?.includes("Open a document")),
    `the title menu offered ${JSON.stringify(items)}`,
  );
  assert.ok(items.some((label) => label?.includes("Copy path")));
  await app.press("Escape");
  await app.page.waitForTimeout(150);
  assert.equal((await app.state()).menuOpen, false);
});

test("hiding the toolbar takes the menu hanging off it away too", async () => {
  await app.page.click("#settings");
  await app.page.waitForTimeout(200);
  assert.equal((await app.state()).menuOpen, true);

  await app.page.click('#popovers .popover-row:has(label:text-is("Show toolbar")) .switch');
  await app.page.waitForTimeout(300);
  const state = await app.state();
  assert.equal(
    await app.page.evaluate(() => document.getElementById("shell").dataset.toolbar),
    "hidden",
  );
  // Left open, it would be anchored to a button that is no longer there.
  assert.equal(state.menuOpen, false, "the menu outlived the bar it hung off");

  await app.press(`${MOD}+KeyT`);
  await app.page.waitForTimeout(300);
  assert.equal(
    await app.page.evaluate(() => document.getElementById("shell").dataset.toolbar),
    "shown",
  );
});

test("selected text is repainted from the page, not from the text layer", async () => {
  // The text layer is pdf.js's and exists to be selected rather than seen: no
  // weight, no style, a generic family at a stretched width. Colouring it
  // would put a page's bold type back as regular and its symbols back as
  // boxes, so the selected words are copied off the page canvas and
  // recoloured instead. What this checks is that the copies are made, are the
  // shape of the lines selected, and go away again.
  const copies = () =>
    app.page.evaluate(() =>
      [...document.querySelectorAll("#pages .selection-layer canvas")].map((canvas) => ({
        width: Math.round(canvas.getBoundingClientRect().width),
        height: Math.round(canvas.getBoundingClientRect().height),
        drawn: canvas.width > 0 && canvas.height > 0,
      })),
    );

  const line = await app.page.evaluate(() => {
    const span = document.querySelector("#pages .textLayer span");
    const range = document.createRange();
    range.setStart(span.firstChild, 0);
    range.setEnd(span.firstChild, 20);
    const selection = getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
    const rect = range.getBoundingClientRect();
    return { width: Math.round(rect.width), height: Math.round(rect.height) };
  });

  // Copying a line off the page and recolouring it is real work, and clearing
  // the copies again waits on the same repaint. Both are waited for rather
  // than slept through, so that the count each assertion reports is the count
  // it settled on rather than the count it happened to be passing through.
  const runs = (n) =>
    app.page
      .waitForFunction(
        (want) => document.querySelectorAll("#pages .selection-layer canvas").length === want,
        n,
        { timeout: 15_000, polling: 50 },
      )
      .catch(() => {});

  await runs(1);
  const painted = await copies();
  assert.equal(painted.length, 1, `expected one run, got ${painted.length}`);
  assert.ok(painted[0].drawn, "the copy was never drawn");
  // Rounded outwards, so a hairline of unselected page cannot show between two
  // runs that meet — never smaller than what was selected, never much larger.
  assert.ok(
    painted[0].width >= line.width && painted[0].width <= line.width + 2,
    `run was ${painted[0].width}px wide for a ${line.width}px selection`,
  );

  await app.page.evaluate(() => getSelection().removeAllRanges());
  await runs(0);
  assert.deepEqual(await copies(), [], "the copies outlived the selection");
});

test("a colour changed in the editor reaches the page it recolours", async () => {
  // Its own window: this one has to be reading under a theme that recolours,
  // and the shared app is on the light theme, which by design does not.
  // `follow_system_theme` off, or the app would take the light theme back off
  // it: the headless machine is in light mode, and following it is the
  // default. What this test is about is the editor, not the appearance.
  const editing = await openApp({
    pdf: PDF,
    settings: { theme: "hylo-dark", sidebar: false, follow_system_theme: false },
  });
  try {
    const paper = () =>
      editing.page.evaluate(() => {
        const canvas = document.querySelector("#pages canvas");
        if (!canvas) return null;
        const [r, g, b] = canvas.getContext("2d").getImageData(4, 4, 1, 1).data;
        return `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
      });

    // Waited for rather than slept on. Recolouring a page is a whole canvas of
    // work — and on an engine without blend modes on a canvas it is the pixel
    // fallback doing it a byte at a time, for every mounted page — so how long
    // it takes is a property of the machine, not of the app. A fixed wait long
    // enough for CI would be a fixed wait wasted on every run here. On timeout
    // this falls through to the assertion, so a page that never arrives is
    // still reported as the colour it is stuck on rather than as a timeout.
    const settlesOn = async (want, message) => {
      await editing.page
        .waitForFunction(
          (expected) => {
            const canvas = document.querySelector("#pages canvas");
            if (!canvas) return false;
            const [r, g, b] = canvas.getContext("2d").getImageData(4, 4, 1, 1).data;
            return `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}` === expected;
          },
          want,
          { timeout: 15_000, polling: 100 },
        )
        .catch(() => {});
      assert.equal(await paper(), want, message);
    };

    await settlesOn("#24272f", "the page did not open in the theme's colours");

    await editing.page.keyboard.press(`${MOD}+Comma`);
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

    // The editor previews by handing the viewer the draft it goes on editing,
    // so a viewer that kept the object rather than a copy of it would find
    // nothing had changed and leave the page as it was printed.
    await settlesOn("#3a0a0a", "the page kept the old background");
  } finally {
    await editing.close();
  }
});

test("select all selects a page, and says so", async () => {
  await app.press("Home");
  await app.page.waitForTimeout(400);
  await app.page.keyboard.press(`${MOD}+KeyA`);
  await app.page.waitForTimeout(300);

  const selection = await app.page.evaluate(() => {
    const range = window.getSelection();
    const text = range?.toString() ?? "";
    const node = range?.anchorNode;
    const element = node instanceof Element ? node : node?.parentElement;
    return { text, inPage: Boolean(element?.closest("#pages .page")) };
  });
  assert.ok(selection.text.includes("Page 1."), `selected ${JSON.stringify(selection.text.slice(0, 40))}`);
  // And nothing outside the document: the whole complaint about the browser's
  // own select-all here is that it takes the contents panel with it.
  assert.ok(!selection.text.includes("Contents"), "the interface came with it");
  assert.ok(selection.inPage);

  const said = await app.page.evaluate(
    () => document.getElementById("notice")?.textContent ?? "",
  );
  assert.match(said, /one page at a time/);

  await app.page.evaluate(() => window.getSelection()?.removeAllRanges());
});

test("presenting is one switch, and Escape is the way back", async () => {
  // Full screen is the window's, and once a browser is in it Escape belongs
  // to the browser — the key never reaches the page at all, which is why this
  // presses the switch again instead. The half of presenting that does live
  // in the page is the toolbar, and the point of it is that one gesture moves
  // both and one gesture puts both back.
  const chrome = () =>
    app.page.evaluate(() => ({
      toolbar: document.getElementById("shell").dataset.toolbar,
      full: document.getElementById("shell").dataset.fullscreen,
      said: document.getElementById("notice").textContent,
    }));
  assert.equal((await chrome()).toolbar, "shown");

  await app.page.keyboard.press(`${MOD}+Shift+KeyP`);
  await app.page.waitForTimeout(500);
  const presenting = await chrome();
  assert.equal(presenting.toolbar, "hidden");
  assert.equal(presenting.full, "true");
  // One gesture, one sentence: the two switches underneath say nothing of
  // their own while this is on.
  assert.match(presenting.said, /^Presenting\./);

  await app.page.keyboard.press(`${MOD}+Shift+KeyP`);
  await app.page.waitForTimeout(500);
  const back = await chrome();
  assert.equal(back.toolbar, "shown", "the toolbar did not come back");
  assert.equal(back.full, "false");
});

test("printing says what it can and cannot do", async () => {
  // ⌘P did nothing at all, which reads as a broken app rather than as a
  // missing feature. In the browser fallback the hand-over is the browser's
  // own print, so what is checked here is that the key is answered and that
  // the sentence says where the document went.
  await app.page.evaluate(() => {
    window.print = () => {};
  });
  await app.page.keyboard.press(`${MOD}+KeyP`);
  await app.page
    .waitForFunction(
      () => !document.getElementById("notice")?.hidden,
      null,
      { timeout: 10_000, polling: 50 },
    )
    .catch(() => {});
  const said = await app.page.evaluate(
    () => document.getElementById("notice")?.textContent ?? "",
  );
  assert.match(said, /HyloPDF does not print/);
  assert.match(said, /print it from there/);
});

test("the way out is on the start screen and nowhere else", async () => {
  const seen = () =>
    app.page.evaluate(() => {
      const button = document.getElementById("quit");
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
