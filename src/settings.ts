/* The settings window.
 *
 * The little menu under the Settings button holds the handful of things a
 * reader reaches for mid-document. Everything else lives here: the same
 * settings written the same way — one key at a time, applied the moment they
 * change — with room to say what each one does, and to show the themes as
 * themes rather than as a list of names.
 *
 * This file knows nothing about the app class; it is handed the small set of
 * things it needs through `SettingsHost`, so nothing here can drift out of
 * step with the rest of the interface. */

import { type Settings, type Theme, deleteTheme, isMac, openPath, saveTheme } from "./api";
import { hydrateIcons } from "./icons";
import { ACTIONS, GROUPS, type Keymap, describeBinding } from "./keys";
import * as ui from "./ui";
import { isDarkTheme, parseColor, selectionArea, selectionInk, toHex } from "./themes";
import type { FitMode, SpreadMode } from "./viewer";

export interface SettingsHost {
  settings: Settings;
  themes: Theme[];
  theme: Theme;
  paths: { config: string; themes: string };
  /** What the keyboard is bound to, and what of `keys.toml` could not be
      read. The Keyboard page is drawn from this rather than from a list of
      its own, so it cannot describe a key the app does not answer to. */
  readonly keymap: Keymap;
  reloadKeys(announce?: boolean): Promise<void>;

  set<K extends keyof Settings>(key: K, value: Settings[K]): void;
  themeById(id: string): Theme;
  replacementFor(gone: Theme): Theme;
  useTheme(theme: Theme, remember?: boolean): void;
  refreshThemes(): Promise<void>;
  toggleDark(on: boolean): void;
  followSystemTheme(): void;
  toggleToolbar(show: boolean): void;
  toggleSidebar(show: boolean): void;
  toggleFullscreen(on: boolean): Promise<void>;
  readonly presenting: boolean;
  togglePresentation(on: boolean): void;
  setFit(mode: FitMode, zoom?: number): void;
  setScrollMode(mode: Settings["scroll_mode"]): void;
  setSpread(spread: SpreadMode): void;
  setPageGap(value: number): void;
  setTrimMargins(on: boolean): void;
  setSidebarWidth(value: number): void;
  setRecolorImages(on: boolean): void;
}

type PageId = "reading" | "appearance" | "window" | "keyboard" | "about";

const PAGES: { id: PageId; label: string; icon: string }[] = [
  { id: "reading", label: "Reading", icon: "book" },
  { id: "appearance", label: "Appearance", icon: "theme" },
  { id: "window", label: "Window", icon: "sidebar" },
  { id: "keyboard", label: "Keyboard", icon: "keyboard" },
  { id: "about", label: "About", icon: "info" },
];

/** The colours a theme editor is working on, before it is saved. A draft is
    never built in: editing one of the packaged themes starts a copy. */
type Draft = Theme;

// Which pane was last open. Coming back to the same one is what a window
// with a sidebar is expected to do.
let currentPage: PageId = "reading";

/** Redraw the open Settings window, if there is one.
 *
 * The theme list in Appearance is a picture of what is on the disk, and the
 * disk can now change while the window is open. Nothing happens mid-edit: a
 * half-finished theme is in the draft rather than in a file, and pulling the
 * pane out from under it would lose it. */
let redraw: (() => void) | null = null;

/**
 * Whether a theme is being written right now.
 *
 * The draft is installed as the live theme so that the app around you is the
 * preview, and while it is, `host.theme` is not a theme that exists on disk —
 * a new one has no id at all. So the watcher's "these are the themes now"
 * cannot be applied the way it is applied at any other time: looking the
 * current theme up in the new list finds nothing, which reads as "the theme
 * you are reading in was deleted", and the answer to that is to pick another
 * one and write the choice down. That would throw the draft away, record a
 * theme nobody chose, and say so in a notice that is simply false.
 *
 * Hence a flag rather than a guess. It lives at module scope because the
 * editor's state is a closure inside `showSettingsWindow` and the question is
 * asked from outside it.
 */
let editingTheme = false;

export function isEditingTheme(): boolean {
  return editingTheme;
}

export function refreshSettingsWindow(): void {
  redraw?.();
}

export function showSettingsWindow(
  host: SettingsHost,
  opening?: { page?: PageId; edit?: { from: Theme | null } },
): void {
  if (opening?.page) currentPage = opening.page;

  // What is being edited in Appearance, if anything. Cleared when the window
  // closes, so it never reopens half-way through something.
  let editing: { draft: Draft; before: Theme } | null = null;
  // One place to change it, so the module-level flag above can never drift
  // from the closure it describes.
  const setEditing = (next: { draft: Draft; before: Theme } | null) => {
    editing = next;
    editingTheme = next !== null;
  };
  if (opening?.edit) {
    const next = { draft: draftFrom(opening.edit.from, host.theme), before: host.theme };
    setEditing(next);
    host.useTheme(next.draft, false);
  }

  // Backing out of a half-finished theme — by cancelling, by walking to
  // another pane, or by closing the window — puts back the one in use.
  const cancelEdit = () => {
    if (!editing) return;
    host.useTheme(editing.before, false);
    setEditing(null);
  };

  ui.showWindow("Settings", () => {
    const body = document.createElement("div");
    body.className = "window-body";

    const nav = document.createElement("nav");
    nav.className = "window-nav";
    const pane = document.createElement("div");
    pane.className = "window-pane";
    body.append(nav, pane);

    const render = () => {
      nav.replaceChildren();
      for (const page of PAGES) {
        const item = document.createElement("button");
        item.className = "btn";
        item.dataset.icon = page.icon;
        item.append(page.label);
        item.setAttribute("aria-pressed", String(page.id === currentPage));
        item.addEventListener("click", () => {
          if (page.id === currentPage) return;
          cancelEdit();
          currentPage = page.id;
          render();
        });
        nav.append(item);
      }

      pane.replaceChildren();
      pane.scrollTop = 0;
      switch (currentPage) {
        case "reading":
          readingPage(host, pane);
          break;
        case "appearance":
          appearancePage(host, pane, {
            editing,
            begin: (from) => {
              const next = { draft: draftFrom(from, host.theme), before: host.theme };
              setEditing(next);
              host.useTheme(next.draft, false);
              render();
            },
            preview: (draft) => host.useTheme(draft, false),
            cancel: () => {
              cancelEdit();
              render();
            },
            done: () => {
              setEditing(null);
              render();
            },
          });
          break;
        case "window":
          windowPage(host, pane);
          break;
        case "keyboard":
          keyboardPage(host, pane, render);
          break;
        case "about":
          aboutPage(host, pane);
          break;
      }
      hydrateIcons(nav);
      hydrateIcons(pane);
    };

    render();
    // Only while this window is up, and only when it is not in the middle of
    // a theme being written.
    redraw = () => {
      if (!editing) render();
    };
    return body;
  }, () => {
    redraw = null;
    cancelEdit();
    // Belt and braces: `cancelEdit` clears the flag, but only if there was
    // something to cancel, and this is the last word on whether an edit is
    // still open.
    editingTheme = false;
  }, "full");
}

/* --------------------------------------------------------------- reading */

function readingPage(host: SettingsHost, pane: HTMLElement): void {
  const s = host.settings;
  pane.append(
    ui.text("title", "Reading"),
  );

  pane.append(
    ui.field(
      "Page progression",
      ui.segmented(
        [
          { value: "continuous" as const, label: "Continuous" },
          { value: "paged" as const, label: "One page at a time" },
        ],
        s.scroll_mode,
        (mode) => host.setScrollMode(mode),
      ),
      "Continuous scrolling is the default, and no shortcut can change it by accident.",
    ),
    ui.field(
      "Pages side by side",
      withDefaultNote(
        ui.segmented(
          [
            { value: "single" as const, label: "One" },
            { value: "two" as const, label: "Two" },
            { value: "cover" as const, label: "Two, cover alone" },
          ],
          s.spread_mode,
          (spread) => host.setSpread(spread),
        ),
        "One is the default",
      ),
      "Two pages across uses a wide window the way a book does. “Cover alone” leaves page one on its own, so that every spread after it falls the way it was printed.",
    ),
    ui.field(
      "Space between pages",
      ui.stepper(s.page_gap, { min: 0, max: 64, step: 4 }, (value) => host.setPageGap(value), "px"),
      "How much room to leave between one page and the next.",
    ),
    ui.field(
      "Trim the margins",
      ui.toggle(s.trim_margins, (on) => host.setTrimMargins(on)),
      "Scanned books and anything typeset with an inch of white down each side spend a quarter of the window on paper. This measures where the ink starts — over a sample of the pages, so every page keeps the same scale — and gives that room back to the words.",
    ),
    ui.field(
      "Zoom",
      ui.segmented(
        [
          { value: "width" as const, label: "Fit width" },
          { value: "page" as const, label: "Fit page" },
          { value: "actual" as const, label: "Fixed" },
        ],
        s.fit_mode,
        (mode) => {
          host.setFit(mode);
          zoomField.hidden = mode !== "actual";
        },
      ),
      "Fit width follows the window; a fixed zoom stays where you put it.",
    ),
  );

  const zoomField = ui.field(
    "Fixed zoom",
    ui.stepper(
      Math.round(s.zoom * 100),
      { min: 25, max: 600, step: 25 },
      (value) => host.setFit("actual", value / 100),
      "%",
    ),
  );
  zoomField.hidden = s.fit_mode !== "actual";
  pane.append(zoomField);

  pane.append(
    ui.field(
      "Come back to where I stopped",
      ui.toggle(s.remember_position, (on) => host.set("remember_position", on)),
      "Each document reopens on the page you left it on.",
    ),
    ui.field(
      "Open what I was reading",
      ui.toggle(s.reopen_last_document, (on) => host.set("reopen_last_document", on)),
      "Start on the documents that were open when you last quit — a window each, where there was more than one. Closing a document yourself means you are done with it, and it is not reopened.",
    ),
    ui.field(
      "Show page count while scrolling",
      ui.toggle(s.show_page_pill, (on) => host.set("show_page_pill", on)),
      "A brief “page 23 of 197” while you scroll with the toolbar hidden.",
    ),
  );
}

/* ------------------------------------------------------------ appearance */

function draftFrom(from: Theme | null, current: Theme): Draft {
  if (!from) {
    return {
      id: "",
      name: "My theme",
      text: current.text,
      background: current.background,
      accent: current.accent,
      link: current.link,
      selection_area: current.selection_area,
      selection_text: current.selection_text,
      recolor: true,
      built_in: false,
    };
  }
  // A built-in theme is never overwritten; editing one starts a copy.
  return {
    id: from.built_in ? "" : from.id,
    name: from.built_in ? `${from.name} copy` : from.name,
    text: from.text,
    background: from.background,
    accent: from.accent,
    link: from.link,
    selection_area: from.selection_area,
    selection_text: from.selection_text,
    recolor: from.recolor,
    built_in: false,
  };
}

function appearancePage(
  host: SettingsHost,
  pane: HTMLElement,
  edit: {
    editing: { draft: Draft; before: Theme } | null;
    begin: (from: Theme | null) => void;
    preview: (draft: Draft) => void;
    cancel: () => void;
    done: () => void;
  },
): void {
  pane.append(
    ui.text("title", "Appearance"),
  );

  pane.append(
    ui.field(
      "Follow the system",
      ui.toggle(host.settings.follow_system_theme, (on) => {
        host.set("follow_system_theme", on);
        if (on) host.followSystemTheme();
        edit.done();
      }),
      "Take the light theme when the machine is light and the dark one when it is dark. Choosing a theme that disagrees turns this off.",
    ),
    ui.field(
      "Dark mode",
      ui.toggle(isDarkTheme(host.theme), (on) => {
        host.toggleDark(on);
        edit.done();
      }),
      "Switches between the light theme and the dark theme you last chose.",
    ),
    ui.field(
      "Recolour pictures too",
      ui.toggle(host.settings.recolor_images, (on) => host.setRecolorImages(on)),
      "On, pictures take the theme along with the rest of the page. Off, they stay exactly as printed.",
    ),
  );

  if (edit.editing) {
    pane.append(themeEditor(host, edit.editing.draft, edit));
    return;
  }

  pane.append(ui.text("group", "Themes"));

  const grid = document.createElement("div");
  grid.className = "theme-grid";
  for (const theme of host.themes) {
    grid.append(themeCard(theme, theme.id === host.theme.id, () => {
      host.useTheme(theme);
      edit.done();
    }));
  }
  pane.append(grid);

  const buttons = document.createElement("div");
  buttons.className = "pane-actions";
  buttons.append(
    ui.button("New theme…", () => edit.begin(null)),
    ui.button(
      host.theme.built_in ? `Make a copy of ${host.theme.name}…` : `Edit ${host.theme.name}…`,
      () => edit.begin(host.theme),
    ),
  );
  if (!host.theme.built_in) {
    buttons.append(
      ui.button(`Delete ${host.theme.name}`, () => {
        const removed = host.theme;
        void (async () => {
          if (!(await ui.confirmDeleteTheme(removed.name))) return;
          try {
            await deleteTheme(removed.id);
            await host.refreshThemes();
            host.useTheme(host.replacementFor(removed));
            ui.notice(`Deleted ${removed.name}.`);
          } catch (error) {
            ui.notice(messageOf(error));
          }
          edit.done();
        })();
      }, "danger"),
    );
  }
  pane.append(buttons);

  pane.append(ui.text("note", `Theme files live in ${host.paths.themes}. They are plain text — a theme can be written by hand, or copied to another computer.`));
}

function themeCard(theme: Theme, current: boolean, onSelect: () => void): HTMLElement {
  const card = document.createElement("button");
  card.className = current ? "theme-card current" : "theme-card";

  const preview = document.createElement("span");
  preview.className = "preview";
  // Through the same reader the page uses, for the reason `ui.swatch` says:
  // a card that shows a colour the renderer cannot read is showing you
  // something you are not going to get.
  const ink = (value: string, fallback: string) =>
    toHex(parseColor(value, parseColor(fallback)));
  preview.style.background = theme.recolor ? ink(theme.background, "#ffffff") : "#ffffff";
  preview.style.color = theme.recolor ? ink(theme.text, "#000000") : "#2f3237";
  const letters = document.createElement("span");
  letters.textContent = "Aa";
  const link = document.createElement("span");
  link.className = "preview-link";
  link.textContent = "Link";
  // A theme that leaves the document alone leaves its links alone too.
  link.style.color = theme.recolor
    ? ink(theme.link ?? theme.accent ?? theme.text, "#000000")
    : "#1a5fb4";
  preview.append(letters, link);

  const name = document.createElement("span");
  name.className = "theme-name";
  if (current) name.dataset.icon = "check";
  name.append(theme.name);

  card.append(preview, name);
  card.addEventListener("click", onSelect);
  return card;
}

function themeEditor(
  host: SettingsHost,
  draft: Draft,
  edit: { preview: (draft: Draft) => void; cancel: () => void; done: () => void },
): HTMLElement {
  const box = document.createElement("div");
  box.append(ui.text("group", draft.id ? "Edit theme" : "New theme"));

  // Held on its own because the field above writes to it: moving the selection
  // area moves the derived ink with it, and a swatch that still showed the old
  // one would be showing something the reader is not going to get.
  const inkField = ui.colorField(toHex(selectionInk(draft)), (value) => {
    draft.selection_text = value;
    edit.preview(draft);
  });

  box.append(
    ui.field(
      "Name",
      ui.textField(draft.name, (value) => {
        draft.name = value;
      }),
    ),
    ui.field(
      "Text",
      ui.colorField(draft.text, (value) => {
        draft.text = value;
        edit.preview(draft);
      }),
      "The colour the words are printed in.",
    ),
    ui.field(
      "Background",
      ui.colorField(draft.background, (value) => {
        draft.background = value;
        edit.preview(draft);
      }),
      "The colour of the paper behind them.",
    ),
    ui.field(
      "Accent",
      ui.colorField(draft.accent ?? draft.text, (value) => {
        draft.accent = value;
        edit.preview(draft);
      }),
      "The current page, the ring around whatever has the keyboard, and anything else that needs to stand out.",
    ),
    ui.field(
      "Links",
      ui.colorField(draft.link ?? draft.accent ?? draft.text, (value) => {
        draft.link = value;
        edit.preview(draft);
      }),
      "Links in the document take this colour, wherever the page is recoloured.",
    ),
    ui.field(
      "Selection area",
      ui.colorField(toHex(selectionArea(draft)), (value) => {
        draft.selection_area = value;
        // The ink on the selection follows the area under it unless it has
        // been set on its own, so the field below has to be told.
        inkField.show(toHex(selectionInk(draft)));
        edit.preview(draft);
      }),
      "The colour behind text you have selected. Left alone it follows the accent.",
    ),
    ui.field(
      "Selected text",
      inkField,
      "The words inside that area. Left alone they take the opposite of it.",
    ),
    ui.field(
      "Recolour the document",
      ui.toggle(draft.recolor, (on) => {
        draft.recolor = on;
        edit.preview(draft);
      }),
      "Off leaves every page exactly as it was printed.",
    ),
  );

  const buttons = document.createElement("div");
  buttons.className = "pane-actions";
  buttons.append(
    ui.button("Cancel", () => edit.cancel()),
    ui.button(
      "Save theme",
      () => {
        void (async () => {
          try {
            const stored = await saveTheme(draft);
            await host.refreshThemes();
            host.useTheme(host.themes.find((theme) => theme.id === stored.id) ?? stored);
            ui.notice(`Saved ${stored.name}.`);
            edit.done();
          } catch (error) {
            ui.notice(messageOf(error));
          }
        })();
      },
      "primary",
    ),
  );
  // Only a theme already on disk can be deleted — "New theme…" and a copy
  // begun from a built-in one have not been saved yet.
  if (draft.id) {
    buttons.append(
      ui.button(
        "Delete this theme",
        () => {
          void (async () => {
            if (!(await ui.confirmDeleteTheme(draft.name))) return;
            try {
              await deleteTheme(draft.id);
              await host.refreshThemes();
              host.useTheme(host.replacementFor(draft));
              ui.notice(`Deleted ${draft.name}.`);
              edit.done();
            } catch (error) {
              ui.notice(messageOf(error));
            }
          })();
        },
        "danger",
      ),
    );
  }
  box.append(buttons);
  box.append(ui.text("note", "Colours apply as you pick them, so the app around you is the preview."));
  return box;
}

/* ---------------------------------------------------------------- window */

function windowPage(host: SettingsHost, pane: HTMLElement): void {
  const s = host.settings;
  pane.append(
    ui.text("title", "Window"),
  );

  pane.append(
    ui.field(
      "Show toolbar",
      ui.toggle(s.show_toolbar, (on) => host.toggleToolbar(on)),
      `The bar along the top. Hidden, the page number appears briefly as you scroll, and the top edge of the window brings the bar back. ${shortcut("⌘T", "Ctrl+T")}`,
    ),
    ui.field(
      "Show contents sidebar",
      ui.toggle(s.show_sidebar, (on) => host.toggleSidebar(on)),
      `Chapters and page thumbnails, down the left. ${shortcut("⌘B", "Ctrl+B")}`,
    ),
    ui.field(
      "Sidebar width",
      ui.stepper(
        s.sidebar_width,
        { min: 160, max: 460, step: 8 },
        (value) => host.setSidebarWidth(value),
        "px",
      ),
      "It can also be dragged by its edge.",
    ),
    ui.field(
      "Full screen",
      ui.toggle(s.fullscreen, (on) => void host.toggleFullscreen(on)),
      `The window fills the screen. ${shortcut("⌘⇧F", "F11")} — and Escape leaves again.`,
    ),
    ui.field(
      "Presenting",
      ui.toggle(host.presenting, (on) => host.togglePresentation(on)),
      `Full screen with nothing else on it: the two switches above, thrown together, and Escape puts both back. ${shortcut("⌘⇧P", "Ctrl+Shift+P")}`,
    ),
  );

}

/* -------------------------------------------------------------- keyboard */

/** What the app listens for — read off what it listens for.
 *
 * This page used to be a hand-written table, and a hand-written table of
 * shortcuts drifts the moment a key moves: it named ⌘T twice, it did not
 * mention the pinch, and it could not have known about a key the reader had
 * rebound. Now every row is an action out of `keys.ts` with whatever
 * `keys.toml` has given it, so the page is a view of the keymap and cannot
 * disagree with it. */
function keyboardPage(host: SettingsHost, pane: HTMLElement, refresh: () => void): void {
  pane.append(
    ui.text("title", "Keyboard"),
  );

  // What could not be read comes first: a key that does nothing is otherwise
  // found out about by pressing it.
  if (host.keymap.problems.length > 0) {
    pane.append(ui.text("group", "In your keys.toml"));
    for (const problem of host.keymap.problems) pane.append(ui.text("note", problem));
  }

  const unbound: string[] = [];
  for (const group of GROUPS) {
    const rows: [string, string][] = [];
    for (const spec of ACTIONS) {
      if (spec.group !== group) continue;
      const keys = host.keymap.byAction.get(spec.id) ?? [];
      if (keys.length === 0) {
        unbound.push(spec.id);
        continue;
      }
      rows.push([spec.label, keys.map(describeBinding).join("  or  ")]);
    }
    if (rows.length === 0) continue;
    pane.append(ui.text("group", group));
    pane.append(keyTable(rows));
  }

  pane.append(ui.text("group", "Without the keyboard"));
  pane.append(
    keyTable([
      ["Zoom", "Pinch, or " + shortcut("⌘ and scroll", "Ctrl and scroll")],
      ["Bring the toolbar back when it is hidden", "The top edge of the window"],
      ["Open something else you have been reading", "The document's name in the bar"],
      ["Back, forward", "The side buttons on a mouse"],
      ["Drag the page around when it is bigger than the window", "The middle button"],
    ]),
  );

  pane.append(ui.text("group", "Changing keybinds"));
  pane.append(
    ui.text(
      "note",
      "Every keybind is in the keys file, commented out. Uncomment a line to change its keys.",
    ),
  );
  if (unbound.length > 0) {
    pane.append(
      ui.text(
        "note",
        // Not guessable from a row that is simply missing.
        (isMac && (unbound.includes("quit") || unbound.includes("close-window"))
          ? "Quitting the app is always ⌘W or ⌘Q."
          : ""),
      ),
    );
  }
  const buttons = document.createElement("div");
  buttons.className = "pane-actions";
  buttons.append(
    ui.button("Open keys file", () => {
      void openPath(join(host.paths.config, "keys.toml")).catch((error) =>
        ui.notice(messageOf(error)),
      );
    }),
    // The themes directory is watched and this file is not: a watch on the
    // config directory would fire on every `settings.toml` write the app makes
    // itself, which is several a minute while somebody is scrolling. So there
    // is a button, rather than a restart.
    ui.button("Reload", () => {
      void host.reloadKeys(false).then(() => {
        refresh();
        // The page redraws either way, so the line is what says it happened —
        // a file with nothing wrong in it redraws to exactly what was there.
        ui.notice(
          host.keymap.problems.length === 0
            ? "Keys reloaded."
            : `Keys reloaded. ${host.keymap.problems.length} could not be used — below.`,
          host.keymap.problems.length === 0 ? "done" : "plain",
        );
      });
    }),
  );
  pane.append(buttons);
}

function keyTable(rows: [string, string][]): HTMLElement {
  const table = document.createElement("div");
  table.className = "keys";
  for (const [what, keys] of rows) {
    const label = document.createElement("span");
    label.textContent = what;
    const key = document.createElement("span");
    key.className = "key";
    key.textContent = keys;
    table.append(label, key);
  }
  return table;
}

/* ----------------------------------------------------------------- about */

function aboutPage(host: SettingsHost, pane: HTMLElement): void {
  pane.append(
    ui.text("title", `HyloPDF ${__APP_VERSION__}`),
    ui.text("lede", "A calm place to read."),
  );

  pane.append(
    ui.text(
      "note",
      "Both are plain text. Nothing is stored anywhere else, and nothing leaves this computer.",
    ),
  );

  const buttons = document.createElement("div");
  buttons.className = "pane-actions";
  buttons.append(
    ui.button("Open settings file", () => {
      void openPath(join(host.paths.config, "settings.toml")).catch((error) =>
        ui.notice(messageOf(error)),
      );
    }),
    ui.button("Open themes folder", () => {
      void openPath(host.paths.themes).catch((error) => ui.notice(messageOf(error)));
    }),
  );
  pane.append(buttons);
}

function join(dir: string, name: string): string {
  const separator = dir.includes("\\") ? "\\" : "/";
  return `${dir}${dir.endsWith(separator) ? "" : separator}${name}`;
}

/* ----------------------------------------------------------------- odds */

/** A control with a quiet note beside it, for the one choice among several
    that is already there until the reader picks another. */
function withDefaultNote(control: HTMLElement, label: string): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "field-control-default";
  const note = document.createElement("span");
  note.className = "field-note";
  note.textContent = label;
  wrap.append(control, note);
  return wrap;
}

function shortcut(mac: string, other: string): string {
  return isMac ? mac : other;
}

function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong.";
}
