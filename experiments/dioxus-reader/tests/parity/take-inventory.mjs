/* What the Tauri app's interface actually contains, written down.
 *
 * The Dioxus port is judged against this file rather than against somebody's
 * memory of the app: `tests/parity.rs` reads it and asserts the port says the
 * same things in the same order. Regenerate it with
 *
 *   node experiments/dioxus-reader/tests/parity/take-inventory.mjs
 *
 * with `npm run dev` running. It is committed because a fixture taken from
 * the real thing is the only reason the comparison means anything.
 */
import { openApp } from "../../../../scripts/ui-harness.mjs";
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const OUT = path.join(path.dirname(fileURLToPath(import.meta.url)), "app-inventory.json");
const app = await openApp({ pdf: "tests/fixtures/book.pdf", width: 1280, height: 860 });

const inventory = await app.page.evaluate(async () => {
  const settle = () => new Promise((go) => setTimeout(go, 120));
  /** The words on a control, with the icon and the shortcut stripped off. */
  const words = (el) => {
    if (!el) return "";
    // The note under a row is a sentence, not a label, and this reader puts it
    // in a span of its own — so it comes off both sides before comparing.
    const copy = el.cloneNode(true);
    for (const note of copy.querySelectorAll(".popover-note")) note.remove();
    return (copy.textContent ?? "").replace(/\s+/g, " ").trim();
  };
  const visible = (el) => {
    const style = getComputedStyle(el);
    return style.display !== "none" && style.visibility !== "hidden" && !el.hidden;
  };

  const out = { toolbar: {}, menus: {}, sidebar: [], find: {}, theme: {} };

  // The toolbar, group by group, in document order.
  for (const group of ["bar-left", "bar-center", "bar-right"]) {
    const box = document.querySelector(`.${group}`);
    out.toolbar[group] = [...box.querySelectorAll("button, input, .page-count")]
      .filter(visible)
      .map((el) => ({
        id: el.id || null,
        // A field's *value* is its label for this purpose: the page number is
        // an input here and a readout that becomes one in the port, which is a
        // deliberate difference of shape and not of content.
        label: el.tagName === "INPUT" ? el.value : words(el),
        icon: el.dataset.icon ?? null,
        // **How wide the control is, to the tenth of a pixel.** This is the
        // one thing in the inventory that is not about *what* the bar holds,
        // and it is here because it is the only question that can see the
        // type: a chip is its padding plus its icon plus its word, and the
        // padding and the icon are numbers both sides already agree on. The
        // port's chips came out five per cent narrow across the whole bar,
        // which is what a UI face looks like with its small-size tracking
        // taken away, and nothing that reads labels could have said so.
        width: +el.getBoundingClientRect().width.toFixed(1),
      }));
    // And the group, which is what says where the middle one sits.
    out.toolbar[`${group}-box`] = (() => {
      const r = box.getBoundingClientRect();
      return { x: +r.x.toFixed(1), width: +r.width.toFixed(1) };
    })();
  }

  // Every menu the toolbar opens, by the button that opens it.
  const menuOf = async (id) => {
    document.getElementById(id).click();
    await settle();
    const popover = document.querySelector("#popovers .popover");
    if (!popover) return null;
    const rows = [
      ...popover.querySelectorAll(
        ".popover-item, .popover-divider, .popover-section, .popover-row",
      ),
    ]
      .filter(visible)
      .map((row) => {
        if (row.classList.contains("popover-item")) {
          return {
            kind: "item",
            label: words(row.querySelector(".popover-label")),
            icon: row.querySelector("[data-icon]")?.dataset.icon ?? null,
          };
        }
        if (row.classList.contains("popover-divider")) return { kind: "rule" };
        if (row.classList.contains("popover-section")) {
          return { kind: "heading", label: words(row) };
        }
        return { kind: "row", label: words(row.querySelector("label") ?? row) };
      });
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await settle();
    return rows;
  };
  for (const id of ["open", "doc-title", "zoom-level", "theme", "settings"]) {
    out.menus[id] = await menuOf(id);
  }

  // The sidebar's tabs, and the find bar's controls.
  document.getElementById("contents").click();
  await settle();
  out.sidebar = [...document.querySelectorAll(".sidebar-tabs .tab")]
    .filter(visible)
    .map((tab) => ({ id: tab.id, label: words(tab), icon: tab.dataset.icon ?? null }));
  document.getElementById("contents").click();
  await settle();

  document.getElementById("find").click();
  await settle();
  const bar = document.getElementById("find-bar");
  out.find = {
    rows: [...bar.querySelectorAll("button, input")].filter(visible).map((el) => ({
      id: el.id || null,
      label: el.tagName === "INPUT" ? null : words(el),
      icon: el.dataset.icon ?? null,
    })),
  };

  // The Settings window, page by page: the fields each carries, in order, and
  // the buttons at the foot of it. The nav column too, because which pages
  // there are is half of what the window is.
  out.settings = { nav: [], pages: {} };
  // Through the cog's own "All settings…", which is the route a reader takes.
  document.getElementById("settings").click();
  await settle();
  const allSettings = [...document.querySelectorAll("#popovers .popover-item")].find(
    (item) => words(item).startsWith("All settings"),
  );
  allSettings?.click();
  await settle();
  const openWindow = document.querySelector(".window");
  if (openWindow) {
    const nav = [...openWindow.querySelectorAll(".window-nav button")];
    out.settings.nav = nav.map((item) => words(item));
    for (const item of nav) {
      item.click();
      await settle();
      const pane = openWindow.querySelector(".window-pane");
      out.settings.pages[words(item)] = {
        // A field the page has hidden — "Fixed zoom" while the zoom is a fit
        // — is not on the page, and the label inside it is not itself hidden.
        fields: [...pane.querySelectorAll(".field")]
          .filter(visible)
          .map((field) => words(field.querySelector(".field-label"))),
        groups: [...pane.querySelectorAll(".group, .pane-group")].map(words),
        actions: [...pane.querySelectorAll(".pane-actions button")].map(words),
      };
    }
    openWindow.querySelector(".window-bar button")?.click();
    await settle();
  }

  // And what the theme resolves to, which is the other half of "does it look
  // the same": every custom property `applyTheme` sets.
  const root = getComputedStyle(document.documentElement);
  for (const name of [
    "--bg", "--surface", "--surface-hover", "--surface-sunk", "--line", "--text",
    "--text-soft", "--text-note", "--text-faint", "--accent", "--accent-soft",
    "--accent-contrast", "--positive", "--negative", "--negative-contrast",
    "--page-paper", "--bar-hover", "--bar-sunk", "--bar-line", "--bar-accent",
    "--selection-area", "--selection-text",
  ]) {
    out.theme[name] = root.getPropertyValue(name).trim();
  }
  return out;
});

writeFileSync(OUT, JSON.stringify(inventory, null, 2) + "\n");
console.log(`wrote ${OUT}`);
await app.close();
