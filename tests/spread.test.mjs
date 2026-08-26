/* Two pages side by side.
 *
 * Fit page in a wide window spends half of it on nothing. Two across is what
 * a book does with that room, and "cover alone" is how a book falls open:
 * page one is a right-hand page, so pairing it with page two puts every
 * spread after it out by one. */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { openApp } from "../scripts/ui-harness.mjs";

const PDF = "tests/fixtures/labelled.pdf";

if (!existsSync(PDF)) {
  throw new Error(`missing ${PDF} — run: node tests/fixtures/make-pdf.mjs ${PDF} 12 labels`);
}

/** Where each mounted page sits, by page number. */
const layout = (app) =>
  app.page.evaluate(() =>
    [...document.querySelectorAll("#pages .page")]
      .map((page) => {
        const transform = /translate\(([-\d.]+)px,\s*([-\d.]+)px\)/.exec(page.style.transform);
        return {
          page: Number(page.dataset.page),
          left: Number(transform?.[1] ?? 0),
          top: Number(transform?.[2] ?? 0),
        };
      })
      .sort((a, b) => a.page - b.page),
  );

/** Wait until a particular page is in the DOM. Only the pages near the window
    are, so these all read at a small zoom where several rows fit at once. */
const settled = async (app, page) => {
  await app.page
    .waitForFunction(
      (want) => Boolean(document.querySelector(`#pages .page[data-page="${want}"]`)),
      page,
      { timeout: 15_000, polling: 50 },
    )
    .catch(() => {});
};

const SMALL = { fit_mode: "actual", zoom: 0.3 };

test("one page across is one page a row", async () => {
  const app = await openApp({ pdf: PDF, settings: SMALL });
  try {
    await settled(app, 3);
    const boxes = await layout(app);
    assert.notEqual(boxes[0].top, boxes[1].top, "two pages shared a row");
  } finally {
    await app.close();
  }
});

test("two across puts the pages in pairs", async () => {
  const app = await openApp({ pdf: PDF, settings: { ...SMALL, spread_mode: "two" } });
  try {
    await settled(app, 3);
    const boxes = await layout(app);
    const [one, two, three] = boxes;
    assert.equal(one.top, two.top, "pages 1 and 2 should stand together");
    assert.ok(one.left < two.left, "page 2 should be to the right of page 1");
    assert.ok(three.top > two.top, "page 3 should start a new row");

    // The pair is what the layout centres, not each page: one gap between
    // them, and the same amount of ground either side of the two together.
    const across = await app.page.evaluate(() => {
      const at = (page) => {
        const el = document.querySelector(`#pages .page[data-page="${page}"]`);
        const left = Number(/translate\(([-\d.]+)px/.exec(el.style.transform)[1]);
        return { left, right: left + el.offsetWidth };
      };
      const one = at(1);
      const two = at(2);
      return {
        gap: two.left - one.right,
        before: one.left,
        after: document.getElementById("pages").offsetWidth - two.right,
      };
    });
    assert.equal(across.gap, 16, "the two pages of a spread should be one gap apart");
    assert.ok(
      Math.abs(across.before - across.after) <= 2,
      `the row is not centred: ${across.before} before, ${across.after} after`,
    );
  } finally {
    await app.close();
  }
});

test("cover alone leaves page one by itself", async () => {
  const app = await openApp({ pdf: PDF, settings: { ...SMALL, spread_mode: "cover" } });
  try {
    await settled(app, 3);
    const boxes = await layout(app);
    const [one, two, three] = boxes;
    assert.ok(two.top > one.top, "page 2 should not stand beside the cover");
    assert.equal(two.top, three.top, "pages 2 and 3 should stand together");
    assert.ok(two.left < three.left);
  } finally {
    await app.close();
  }
});

test("a page turn turns the spread, not half of it", async () => {
  const app = await openApp({ pdf: PDF, settings: { ...SMALL, spread_mode: "two" } });
  try {
    await settled(app, 3);
    assert.equal((await app.state()).page, "i");
    await app.press("ArrowRight");
    await app.page.waitForTimeout(400);
    // Pages 1 and 2 are i and ii; the next spread starts at iii.
    assert.equal((await app.state()).page, "iii");
    await app.press("ArrowLeft");
    await app.page.waitForTimeout(400);
    assert.equal((await app.state()).page, "i");
  } finally {
    await app.close();
  }
});
