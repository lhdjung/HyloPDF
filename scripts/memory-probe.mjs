/* What a reading session costs, from outside the browser.
 *
 * There is no memory API in WebKit worth the name — `performance.memory` is
 * Chrome's, and `measureUserAgentSpecificMemory` needs cross-origin isolation
 * — so the number has to be read the way Activity Monitor reads it: physical
 * footprint, per process, from the outside. This opens the interface in the
 * harness, reads a document the way somebody would, and reports the footprint
 * of every process the browser runs in as it goes.
 *
 * It is not part of `npm test`: it wants a real document, it takes minutes,
 * and what it measures is the machine as much as the app. It is here so the
 * numbers in AGENTS.md can be checked rather than believed.
 *
 *   npm run dev
 *   node scripts/memory-probe.mjs some-book.pdf [viewports]
 *
 * `--regions` adds a vmmap breakdown at the end, which is what says *where*
 * the memory is — the app's own heap and the pages it is drawing live in the
 * web content process, and the pictures it has decoded live in the GPU process
 * as image buffers. That distinction is the whole reason this file exists: the
 * first sighting of the problem it was written for was three gigabytes with
 * nothing on the JavaScript heap to explain it.
 *
 * Every run ends by closing the document, because the plateau is only half the
 * question. A number that stops climbing may still be memory the app will never
 * give back, and the way to tell is to take the document away and see whether
 * the start screen costs what it cost before.
 */

import { execSync } from "node:child_process";

import { openApp } from "./ui-harness.mjs";

const [, , file, viewportsArg, ...rest] = process.argv;
if (!file) {
  console.error("usage: node scripts/memory-probe.mjs <file.pdf> [viewports] [--regions]");
  process.exit(1);
}
const viewports = Number(viewportsArg ?? 120);
const wantRegions = rest.includes("--regions") || viewportsArg === "--regions";

/* Every WebKit process that was not already running. Playwright's browser is
   several — the UI process, the web content process, the GPU process and the
   network process — and the interesting ones are not the one it launched. */
const listWebKit = () =>
  execSync("pgrep -f 'Playwright.app|WebKit' || true").toString().trim().split("\n").filter(Boolean);
const before = new Set(listWebKit());
const ours = () => listWebKit().filter((pid) => !before.has(pid)).map(Number);

/** @param {number} pid */
const footprintMB = (pid) => {
  try {
    const line = execSync(`footprint -p ${pid} 2>/dev/null | awk '/phys_footprint:/{print $2, $3}'`)
      .toString().trim();
    if (!line) return 0;
    const [size, unit] = line.split(" ");
    return Number(size) * (unit === "MB" ? 1 : unit === "KB" ? 1 / 1024 : 1024);
  } catch {
    return 0;
  }
};

/** @param {number} pid */
const nameOf = (pid) => {
  try {
    return execSync(`ps -p ${pid} -o comm= | sed 's|.*/||;s|com.apple.WebKit.||;s|.Development||'`)
      .toString().trim();
  } catch {
    return String(pid);
  }
};

/** @param {string} label */
const report = (label) => {
  const parts = [];
  let total = 0;
  for (const pid of ours()) {
    const mb = footprintMB(pid);
    if (mb < 1) continue;
    parts.push(`${nameOf(pid)} ${mb.toFixed(0)}`);
    total += mb;
  }
  console.log(`${label.padEnd(16)}${parts.join(" · ").padEnd(56)} total ${total.toFixed(0)}MB`);
};

const app = await openApp({
  settings: { theme: "hylo-light", fit_mode: "width", zoom: 1, remember_position: false },
  width: 1280,
  height: 860,
  pdf: file,
});
// Let the first pages settle before anything is counted.
await app.page.waitForTimeout(4000);
report("opened");

for (let step = 1; step <= viewports; step++) {
  await app.page.locator("#viewer").focus();
  await app.page.keyboard.press("PageDown");
  await app.page.waitForTimeout(250);
  if (step % 20 === 0) {
    await app.page.waitForTimeout(1500);
    report(`after ${step}`);
  }
}

/* The regions that hold pixels, virtual against resident.
 *
 * Read the resident column and only the resident column. WebKit keeps a pool
 * of IOSurfaces mapped and mostly not backed — 612MB of address space against
 * 113MB actually resident, over 160 regions, is a normal reading of a document
 * with three pages on screen — so the virtual figure looks like a catastrophe
 * and is not one. It is the column the first pass through this read, and it
 * sent the hunt after a leak that was not there. */
if (wantRegions) {
  for (const pid of ours()) {
    const name = nameOf(pid);
    if (!/WebContent|GPU/.test(name)) continue;
    const rows = execSync(
      // Only the summary's region table; the malloc-zone table below it repeats
      // some of the same names with a different shape.
      `vmmap --summary ${pid} 2>/dev/null | sed -n '/REGION TYPE/,/^ *$/p'` +
        ` | grep -E '^(IOSurface|IOAccelerator \\(graphics\\)|owned unmapped \\(graphics\\)|MALLOC_LARGE|JS |WebKit Malloc)' || true`,
    ).toString().trimEnd();
    console.log(`\n${name} — virtual · resident · dirty\n${rows || "  (nothing of note)"}`);
  }
}

/* Whether the document gives it back.
 *
 * The plateau above says the app stops growing; it does not say the memory is
 * the document's rather than the app's. Closing it should return the whole
 * process group to what the start screen costs, and if it does not, whatever
 * is left is held by something that has no page to be on. */
report("before closing");
await app.page.click("#close-doc").catch(() => {});
await app.page.waitForTimeout(6000);
report("closed");
await app.page.waitForTimeout(8000);
report("…8s later");

await app.close();
