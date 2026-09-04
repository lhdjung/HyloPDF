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

  const out = { toolbar: {}, menus: {}, sidebar: [], find: {}, theme: {}, rows: {}, chrome: {} };

  /* **What is actually painted, as against what is named.**
   *
   * The twenty-two variables below are compared name for name and every one
   * of them matched while the start screen stood on the wrong one of them —
   * `--paper` where the app has `--bg`, which is a whole window of the wrong
   * colour and nothing in this file could see it. So the two largest areas
   * of the interface are read as *resolved* colours here, and the port is
   * asked for them off its own pixels. */
  const painted = (selector) => {
    const el = document.querySelector(selector);
    if (!el) return null;
    return (
      "#" +
      (getComputedStyle(el).backgroundColor.match(/\d+/g) ?? [])
        .slice(0, 3)
        .map((channel) => (+channel).toString(16).padStart(2, "0"))
        .join("")
    );
  };
  out.chrome.toolbar = painted("#toolbar");
  out.chrome.ground = painted("body");

  /** How tall a row of some surface is laid out, to the tenth of a pixel.
   *
   * The companion to the widths in `toolbar` above and the same argument: a
   * row is its padding plus its line, both sides agree about the padding, so
   * the height is the type. Every surface in the port that floats or lists —
   * the menus, the sidebar, the Settings window — had been written in the
   * toolbar's 13.5 where the app writes them in 14.5, and every one of them
   * came out a row shorter. Nothing that compares labels can see that. */
  const heightOf = (sel) => {
    const el = document.querySelector(sel);
    return el ? +el.getBoundingClientRect().height.toFixed(1) : null;
  };

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

  // A menu row, measured with a menu actually open.
  document.getElementById("theme").click();
  await settle();
  out.rows["menu-item"] = heightOf("#popovers .popover-item");
  out.rows["menu-heading"] = heightOf("#popovers .popover-section");
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await settle();

  // The sidebar's tabs, and the find bar's controls.
  document.getElementById("contents").click();
  await settle();
  out.sidebar = [...document.querySelectorAll(".sidebar-tabs .tab")]
    .filter(visible)
    .map((tab) => ({ id: tab.id, label: words(tab), icon: tab.dataset.icon ?? null }));
  out.rows["tab"] = heightOf(".sidebar-tabs .tab");
  out.rows["outline-item"] = heightOf(".outline-item");
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
    out.rows["window-bar"] = heightOf(".window-bar");
    out.rows["nav-item"] = heightOf(".window-nav button");
    out.rows["switch"] = heightOf(".window-pane .switch");
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

  // The Information window, which is the last item of the title menu and the
  // only window in the app that is neither Settings nor a question. What it
  // holds is decided by the document — a paper with no `/Subject` has no
  // Subject row — so the fixture is the fixture's answer, and the assertion
  // that reads it asks for the *rows this document produces* rather than for
  // all ten.
  out.document = null;
  document.getElementById("doc-title").click();
  await settle();
  const information = [...document.querySelectorAll("#popovers .popover-item")].find(
    (item) => words(item) === "Information",
  );
  if (information) {
    information.click();
    await settle();
    await settle();
    const win = document.querySelector(".window");
    if (win) {
      out.document = {
        title: words(win.querySelector(".window-bar")).replace(/\s*✕?\s*$/, ""),
        heading: words(win.querySelector(".window-pane .pane-title")),
        fields: [...win.querySelectorAll(".field")]
          .filter(visible)
          .map((field) => words(field.querySelector(".field-label"))),
      };
      win.querySelector(".window-bar button")?.click();
      await settle();
    }
  } else {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await settle();
  }

  // The theme editor, which is Appearance's second half and the largest
  // surface in the app that the fixture had never opened. Reached through
  // "New theme…", because that is the route with no theme to undo afterwards.
  out.editor = null;
  document.getElementById("settings").click();
  await settle();
  const toSettings = [...document.querySelectorAll("#popovers .popover-item")].find((item) =>
    words(item).startsWith("All settings"),
  );
  toSettings?.click();
  await settle();
  const settingsWindow = document.querySelector(".window");
  if (settingsWindow) {
    const appearance = [...settingsWindow.querySelectorAll(".window-nav button")].find(
      (item) => words(item) === "Appearance",
    );
    appearance?.click();
    await settle();
    const newTheme = [...settingsWindow.querySelectorAll(".pane-actions button")].find(
      (button) => words(button).startsWith("New theme"),
    );
    if (newTheme) {
      newTheme.click();
      await settle();
      // The editor is appended into the pane below Appearance's own three
      // fields, so it is taken from its heading down rather than from the
      // pane — otherwise "Follow the system" is reported as a field of the
      // theme editor, which it is not.
      const pane = settingsWindow.querySelector(".window-pane");
      const heading = [...pane.querySelectorAll(".pane-group")].find((el) =>
        ["New theme", "Edit theme"].includes(words(el)),
      );
      const editorBox = heading?.parentElement ?? pane;
      out.editor = {
        heading: words(heading),
        fields: [...editorBox.querySelectorAll(".field")]
          .filter(visible)
          .map((field) => ({
            label: words(field.querySelector(".field-label")),
            // The sentence under a colour, which is the whole of how a reader
            // finds out what "Accent" is for. A port with the fields and not
            // the notes has the form and not the help.
            note: words(field.querySelector(".field-note")),
          })),
        actions: [...editorBox.querySelectorAll(".pane-actions button")].map(words),
      };
    }
    settingsWindow.querySelector(".window-bar button")?.click();
    await settle();
  }

  // The three things the app says over a page rather than in the chrome: the
  // page number while a scroll is running, the way back to a toolbar that has
  // been put away, and what a dragged file is told. None of them is reachable
  // by clicking, so each is read out of the element itself — which is fair,
  // because what is being compared is the words.
  out.overlay = {
    peek: words(document.getElementById("toolbar-peek")),
    drop: words(document.getElementById("drop-hint")),
    // The pill is filled as the scroll runs. Asking for it is the one way to
    // see what shape it is written in.
    pill: await (async () => {
      const viewer = document.getElementById("viewer");
      viewer.scrollBy(0, 900);
      await settle();
      return words(document.getElementById("page-pill"));
    })(),
  };

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

await app.close();

/* The start screen, which needs an app with nothing in it.
 *
 * It is a second launch rather than a Close in the first, because closing a
 * document is a gesture with a *history* — the recents shelf has the document
 * just put down at the top of it — and what the port has to match is the
 * screen a reader meets on opening the app, not the one they get on the way
 * out of a paper. Seeding a library gives the shelf something to hold. */
const empty = await openApp({
  width: 1280,
  height: 860,
  settings: { restore_last: false },
});
inventory.start = await empty.page.evaluate(async () => {
  const words = (el) => (el?.textContent ?? "").replace(/\s+/g, " ").trim();
  const welcome = document.getElementById("welcome");
  const box = (el) => {
    const rect = el.getBoundingClientRect();
    return { width: +rect.width.toFixed(1), height: +rect.height.toFixed(1) };
  };
  const hex = (colour) =>
    "#" +
    (colour.match(/\d+/g) ?? [])
      .slice(0, 3)
      .map((channel) => (+channel).toString(16).padStart(2, "0"))
      .join("");
  // A row of the recents shelf, laid out by the app's own cascade. The shelf
  // itself is empty here — the browser fallback's `bootstrap` returns no
  // library at all — so the row is built rather than found, and what is read
  // off it is the stylesheet's answer and not a number written down twice.
  const probe = document.createElement("button");
  probe.className = "recent";
  probe.textContent = "x";
  welcome.append(probe);
  const rowStyle = getComputedStyle(probe);
  const row = +probe.getBoundingClientRect().height.toFixed(1);
  const rowPadding = `${rowStyle.paddingTop} ${rowStyle.paddingRight}`;
  probe.remove();
  return {
    name: words(welcome.querySelector("h1")),
    sub: words(welcome.querySelector(".welcome-sub")),
    open: words(welcome.querySelector("#welcome-open")),
    hint: words(welcome.querySelector(".welcome-hint")),
    // **What the four lines above cannot see, and what a reader saw anyway.**
    // The screen was compared by its words alone and every one of them
    // matched while the port ran it at the body's 13.5px on the toolbar's
    // colour with a button the width of the whole column. So: the type, the
    // ground, and the two boxes that are padding plus a word.
    fontSize: parseFloat(getComputedStyle(welcome).fontSize),
    background: hex(getComputedStyle(welcome).backgroundColor),
    boxes: {
      open: box(welcome.querySelector("#welcome-open")),
      inner: box(welcome.querySelector(".welcome-inner")),
      recent: { height: row, padding: rowPadding },
    },
    // What the shelf is called when there is one. It is written whether or
    // not this launch has anything to put in it, because the words are the
    // thing being compared and an empty library would report `""` for a
    // heading the app does have.
    recentsTitle: "Recently read",
  };
});
await empty.close();

writeFileSync(OUT, JSON.stringify(inventory, null, 2) + "\n");
console.log(`wrote ${OUT}`);
