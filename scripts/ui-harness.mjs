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

/* The app takes its whole shortcut scheme from the browser's platform — `isMac`
 * in api.ts, off `navigator.platform` — so ⌘F opens the find bar on a Mac and
 * does nothing at all anywhere else, where it is Ctrl+F. A test that presses a
 * hard-coded Meta therefore passes on the machine it was written on and fails
 * on CI, which is Linux, and the failure looks like a broken find bar rather
 * than a wrong key.
 *
 * So the modifier comes from the platform here, and `HYLOPDF_PLATFORM=other`
 * forces the other scheme — it lies to `navigator.platform` as well, so the app
 * and the test agree about which machine they are on. That is what makes the
 * Linux keyboard reachable from a Mac without waiting seven minutes for CI. */
const PRETEND = process.env.HYLOPDF_PLATFORM;
export const onMac = PRETEND ? PRETEND === "mac" : process.platform === "darwin";

/** The modifier this platform's shortcuts hang off: ⌘ on a Mac, Ctrl elsewhere. */
export const MOD = onMac ? "Meta" : "Control";

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
 * @param {"light"|"dark"} [options.appearance]  what the machine outside the
 *        app is set to, which the app follows unless told not to.
 * @param {Record<string, string[]>} [options.keys]  bindings, as `keys.toml`
 *        would give them: `{ "next-page": ["n"] }`.
 * @param {{writable?: boolean, reason?: string, cloud?: string|null, size?: number}}
 *        [options.writability]  what the disk says about the document:
 *        `{ writable: false, reason: "x.pdf is read only." }` for the
 *        journal-only path, `{ cloud: "Dropbox" }` for the warning.
 */
export async function openApp(options = {}) {
  const engine = options.engine === "chromium" ? chromium : webkit;
  const browser = await engine.launch({ headless: !options.headed });
  const context = await browser.newContext({
    viewport: { width: options.width ?? 1280, height: options.height ?? 860 },
    deviceScaleFactor: 2,
    colorScheme: options.appearance ?? "light",
  });
  const page = await context.newPage();

  // `HYLOPDF_NO_BLEND=1` refuses the non-separable blend modes the way an engine
  // that does not implement them on a canvas does — silently, by keeping the
  // property's previous value rather than by throwing. `canBlend()` in themes.ts
  // probes for exactly that and falls back to `recolorByPixel`, so this runs the
  // whole interface down the slow path: what WebKitGTK may be doing on Linux,
  // and the thing least likely to be exercised anywhere else. `recolor.test.mjs`
  // tests the fallback function; this tests reading under it.
  if (process.env.HYLOPDF_NO_BLEND) {
    await page.addInitScript(() => {
      const proto = CanvasRenderingContext2D.prototype;
      const real = Object.getOwnPropertyDescriptor(proto, "globalCompositeOperation");
      if (!real?.get || !real.set) return;
      const { get, set } = real;
      const unsupported = ["saturation", "color-dodge", "luminosity", "color", "hue"];
      Object.defineProperty(proto, "globalCompositeOperation", {
        get() {
          return get.call(this);
        },
        /** @param {string} value */
        set(value) {
          if (!unsupported.includes(value)) set.call(this, value);
        },
      });
    });
  }

  // Before anything reads it: api.ts computes `isMac` at module load.
  if (PRETEND) {
    await page.addInitScript((platform) => {
      Object.defineProperty(navigator, "platform", { get: () => platform });
    }, onMac ? "MacIntel" : "Linux x86_64");
  }

  /** @type {string[]} */
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

  // The keys, the same way: `keys.toml` is a file and there is no disk here,
  // so the browser twin of `loadKeys` reads this instead. Action name against
  // the keys it should answer to — exactly what a line of `keys.toml` says.
  if (options.keys) {
    await page.addInitScript((seed) => {
      localStorage.setItem("hylopdf.keys", JSON.stringify(seed));
    }, options.keys);
  }

  // What the disk would say about the document, for the browser twin of
  // `documentWritability`. There is no disk here, so a read-only document —
  // or one in a syncing folder — is seeded rather than made.
  if (options.writability) {
    await page.addInitScript((seed) => {
      localStorage.setItem("hylopdf.writability", JSON.stringify(seed));
    }, options.writability);
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

    /** A key to the document, the way the app's own shortcut handler sees it.
     *
     * @param {string} key
     * @param {{ delay?: number }} [options]
     */
    async press(key, options) {
      await page.locator("#viewer").focus();
      await page.keyboard.press(key, options);
      await page.waitForTimeout(120);
    },

    /** A wheel gesture over the document: `ticks` events of `deltaY` each.
     *  `ctrl` makes it the pinch that a trackpad sends.
     *
     * @param {number} ticks
     * @param {number} deltaY
     * @param {{ ctrl?: boolean, pause?: number }} [options]
     */
    async wheel(ticks, deltaY, { ctrl = false, pause = 16 } = {}) {
      const box = await page.locator("#viewer").boundingBox();
      if (!box) throw new Error("the viewer has no box to aim at");
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      if (ctrl) await page.keyboard.down("Control");
      for (let i = 0; i < ticks; i++) {
        await page.mouse.wheel(0, deltaY);
        await page.waitForTimeout(pause);
      }
      if (ctrl) await page.keyboard.up("Control");
      await page.waitForTimeout(200);
    },

    /** The machine outside the app changes its mind — sunset, or somebody
        throwing the switch in System Settings. */
    /** @param {"light"|"dark"} appearance */
    async setAppearance(appearance) {
      await page.emulateMedia({ colorScheme: appearance });
      await page.waitForTimeout(200);
    },

    /** What the interface currently says about itself. */
    async state() {
      return page.evaluate(() => {
        const window = document.querySelector("#windows .window");
        const number = /** @type {HTMLInputElement | null} */ (
          document.getElementById("page-number")
        );
        return {
          page: number?.value,
          pages: document.getElementById("page-count")?.textContent,
          zoom: document.getElementById("zoom-level")?.textContent,
          scrollTop: document.getElementById("viewer")?.scrollTop,
          findOpen: !document.getElementById("find-bar")?.hidden,
          findStatus: document.getElementById("find-status")?.textContent,
          menuOpen: document.querySelectorAll("#popovers .popover").length > 0,
          windowTitle: window?.querySelector(".window-title")?.textContent ?? null,
          windowText: window?.querySelector(".pane-lede")?.textContent ?? null,
          onStartScreen: document.getElementById("shell")?.dataset.empty === "true",
          paper: getComputedStyle(document.documentElement)
            .getPropertyValue("--page-paper")
            .trim(),
        };
      });
    },

    /** @param {string} file */
    async shot(file) {
      await page.screenshot({ path: file });
      return file;
    },

    async close() {
      await browser.close();
    },
  };
}
