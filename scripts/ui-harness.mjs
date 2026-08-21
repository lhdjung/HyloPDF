/* Drive the interface in a headless browser.
 *
 * `npm run dev` serves the frontend on its own, and `api.ts` has a browser
 * fallback for everything that would otherwise be Rust — settings go to
 * localStorage, a file input stands in for the native picker. So the whole
 * interface can be exercised here: keys, menus, search, scrolling, zoom.
 *
 * The point is that it costs nobody their screen. The alternative is
 * synthesising input into the real app, which has to be frontmost to receive
 * it, so it takes the machine away from whoever is using it.
 *
 * Usage:
 *
 *   import { openApp } from "./ui-harness.mjs";
 *   const app = await openApp({ pdf: "some.pdf" });
 *   await app.press("ArrowRight");
 *   console.log(await app.state());
 *   await app.close();
 */

import { chromium, webkit } from "playwright";
import { fileURLToPath } from "node:url";
import path from "node:path";

const ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const URL_BASE = process.env.HYLOPDF_URL ?? "http://localhost:1420/";

/**
 * Boot the interface and, if asked, open a document in it.
 *
 * @param {object}  [options]
 * @param {string}  [options.pdf]       path to a PDF to open, relative to the repo
 * @param {object}  [options.settings]  settings to seed before the app starts
 * @param {boolean} [options.headed]    show the browser, for watching it work
 * @param {"webkit"|"chromium"} [options.engine]  webkit by default — the app
 *        runs in a WKWebView, and the differences are not cosmetic: pinch
 *        zoom, blend modes and text layout all behave differently in Chromium.
 * @param {number}  [options.width]
 * @param {number}  [options.height]
 * @param {"document"|"password"} [options.expect]  what opening the file should
 *        produce: the document itself, or the password window for an encrypted
 *        one, which never reaches a page count.
 */
export async function openApp(options = {}) {
  const engine = options.engine === "chromium" ? chromium : webkit;
  const browser = await engine.launch({ headless: !options.headed });
  const context = await browser.newContext({
    viewport: { width: options.width ?? 1280, height: options.height ?? 860 },
    deviceScaleFactor: 2,
  });
  const page = await context.newPage();

  const logs = [];
  page.on("console", (message) => logs.push(`${message.type()}: ${message.text()}`));
  page.on("pageerror", (error) => logs.push(`pageerror: ${error.message}`));

  // Settings live in localStorage in the browser fallback, and have to be
  // there before main.ts reads them.
  if (options.settings) {
    await page.addInitScript((seed) => {
      // One key holds the whole table, the same as `FALLBACK_KEY` in api.ts.
      const key = "hylopdf.settings";
      const held = JSON.parse(localStorage.getItem(key) || "{}");
      localStorage.setItem(key, JSON.stringify({ ...held, ...seed }));
    }, options.settings);
  }

  await page.goto(URL_BASE, { waitUntil: "domcontentloaded" });
  await page.waitForSelector("#shell", { state: "attached" });

  if (options.pdf) {
    const file = path.resolve(ROOT, options.pdf);
    const chooser = page.waitForEvent("filechooser");
    await page.click("#welcome-open");
    await (await chooser).setFiles(file);
    if (options.expect === "password") {
      await page.waitForSelector("#windows .window", { timeout: 20_000 });
    } else {
      // The page count lands only once pdf.js has the document.
      await page.waitForFunction(
        () => (document.getElementById("page-count")?.textContent ?? "").length > 0,
        null,
        { timeout: 20_000 },
      );
    }
  }

  return {
    page,
    browser,
    logs,

    /** A key to the document, the way the app's own shortcut handler sees it. */
    async press(key, options) {
      await page.locator("#viewer").focus();
      await page.keyboard.press(key, options);
      await page.waitForTimeout(120);
    },

    /** A wheel gesture over the document: `ticks` events of `deltaY` each.
        `ctrl` makes it the pinch that a trackpad sends. */
    async wheel(ticks, deltaY, { ctrl = false, pause = 16 } = {}) {
      const box = await page.locator("#viewer").boundingBox();
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      if (ctrl) await page.keyboard.down("Control");
      for (let i = 0; i < ticks; i++) {
        await page.mouse.wheel(0, deltaY);
        await page.waitForTimeout(pause);
      }
      if (ctrl) await page.keyboard.up("Control");
      await page.waitForTimeout(200);
    },

    /** What the interface currently says about itself. */
    async state() {
      return page.evaluate(() => {
        const window = document.querySelector("#windows .window");
        return {
          page: document.getElementById("page-number")?.value,
          pages: document.getElementById("page-count")?.textContent,
          zoom: document.getElementById("zoom-level")?.textContent,
          scrollTop: document.getElementById("viewer")?.scrollTop,
          findOpen: !document.getElementById("find-bar")?.hidden,
          findStatus: document.getElementById("find-status")?.textContent,
          menuOpen: document.querySelectorAll("#popovers .popover").length > 0,
          windowTitle: window?.querySelector(".window-title")?.textContent ?? null,
          windowText: window?.querySelector(".pane-lede")?.textContent ?? null,
          onStartScreen: document.getElementById("shell")?.dataset.empty === "true",
        };
      });
    },

    async shot(file) {
      await page.screenshot({ path: file });
      return file;
    },

    async close() {
      await browser.close();
    },
  };
}
