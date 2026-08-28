/* Everything that crosses into Rust goes through here.
   The browser fallback exists so `npm run dev` can be opened in a normal
   browser while working on the interface; the real app always has Tauri. */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type Settings = {
  theme: string;
  light_theme: string;
  dark_theme: string;
  follow_system_theme: boolean;
  scroll_mode: "continuous" | "paged";
  spread_mode: "single" | "two" | "cover";
  fit_mode: "width" | "page" | "actual";
  zoom: number;
  page_gap: number;
  trim_margins: boolean;
  recolor_images: boolean;
  remember_position: boolean;
  reopen_last_document: boolean;
  show_page_pill: boolean;
  search_highlight_all: boolean;
  search_match_case: boolean;
  search_whole_words: boolean;
  show_toolbar: boolean;
  show_sidebar: boolean;
  sidebar_width: number;
  fullscreen: boolean;
  window_width: number;
  window_height: number;
  window_x: number | null;
  window_y: number | null;
  window_maximized: boolean;
  markup_color_1: string;
  markup_color_2: string;
  markup_color_3: string;
  markup_color_4: string;
  markup_color_5: string;
  markup_color_6: string;
};

export type Theme = {
  id: string;
  name: string;
  text: string;
  background: string;
  accent: string | null;
  link: string | null;
  /** The colour behind selected text. Null means "derive it from the accent". */
  selection_area: string | null;
  /** The colour selected text is drawn in. Null means "derive it from the
      colour behind it". */
  selection_text: string | null;
  recolor: boolean;
  built_in: boolean;
};

/** A place the reader put a pin in. One per page — see `library.rs`. */
export type Mark = {
  page: number;
  offset: number;
  title: string;
  at: number;
};

/** The PDF spec's own four markup styles. */
export type HighlightStyle = "highlight" | "underline" | "strikeout" | "squiggly";

/** Coloured markup the reader drew on a passage — the journal's copy, not the
    document's. See `library.rs`'s `Highlight` for why this is a cache rather
    than the record: on open, the file wins and this list is rebuilt from
    what `getAnnotations` reports. */
export type Highlight = {
  /** Generated with `crypto.randomUUID` the moment a passage is marked, so a
      highlight not yet saved into the document still has something to be
      removed by. */
  id: string;
  page: number;
  /** Four points per run, x then y, run after run, in the page's own PDF
      coordinate space. */
  quads: number[];
  /** Hex, like a theme's colours — read the same careful way, through
      `parseColor`, never handed to CSS on its own. */
  color: string;
  opacity: number;
  style: HighlightStyle;
  quote: string;
  at: number;
  /** The annotation's object id in the file, once `getAnnotations` has
      actually read it back out. Absent for a highlight the journal knows
      about but the file does not yet. */
  annotation_id: string | null;
};

export type LibraryEntry = {
  path: string;
  title: string;
  page: number;
  offset: number;
  opened_at: number;
  marks?: Mark[];
  highlights?: Highlight[];
};

export type Bootstrap = {
  settings: Settings;
  themes: Theme[];
  library: LibraryEntry[];
  /** What was open when the app was last put down, if it is still there.
      Empty when the reader closed it themselves. */
  open_document: string;
  config_dir: string;
  themes_dir: string;
};

/** The keyboard as the reader has it: action name → the keys that ask for it,
    and whatever HyloPDF could not make sense of on the way in. */
export type Keys = {
  bindings: Record<string, string[]>;
  problems: string[];
};

export type OpenedDocument = {
  path: string;
  name: string;
  page: number;
  offset: number;
};

export const hasBackend = "__TAURI_INTERNALS__" in window;

export const isMac = /mac/i.test(navigator.platform || navigator.userAgent);

/* ------------------------------------------------------------- fallbacks */

const FALLBACK_KEY = "hylopdf.settings";
/* The keys, for the browser path. `keys.toml` is a file and there is no disk
 * here, so the same table arrives as JSON — which is what lets the harness
 * open the app with a key rebound and press it. */
const KEYS_FALLBACK_KEY = "hylopdf.keys";

const fallbackDefaults: Settings = {
  theme: "hylo-light",
  light_theme: "hylo-light",
  dark_theme: "hylo-dark",
  follow_system_theme: true,
  scroll_mode: "continuous",
  spread_mode: "single",
  fit_mode: "width",
  zoom: 1,
  page_gap: 16,
  trim_margins: false,
  recolor_images: true,
  remember_position: true,
  reopen_last_document: true,
  show_page_pill: true,
  search_highlight_all: true,
  search_match_case: false,
  search_whole_words: false,
  show_toolbar: true,
  show_sidebar: false,
  sidebar_width: 252,
  fullscreen: false,
  window_width: 1280,
  window_height: 860,
  window_x: null,
  window_y: null,
  window_maximized: true,
  markup_color_1: "#ffd60a",
  markup_color_2: "#7bed9f",
  markup_color_3: "#ff6b6b",
  markup_color_4: "#74c0fc",
  markup_color_5: "#ffa94d",
  markup_color_6: "#da77f2",
};

/* The packaged themes, for the browser.
 *
 * With Rust behind it the app reads these from the user's theme directory,
 * where they were installed from the same files this reads here. Keeping a
 * second copy of the colours in TypeScript is what made `npm run dev` show a
 * theme nobody had shipped for months, and the drift is invisible until
 * somebody edits a theme and it does not change: the file was right and the
 * copy was what was on screen.
 *
 * The set and its order come from the files too. There used to be a list of
 * ids here, restating the one in `theme.rs`, and it went stale the same way
 * and worse: a theme missing from it simply never appeared under `npm run
 * dev`, with nothing to say why. `order` is a key in each shipped file — see
 * `build.rs`, which is where the same question is answered for Rust.
 *
 * The parsing is a fraction of TOML — a flat table of quoted strings, numbers
 * and booleans, which is all a theme file is. Anything cleverer is Rust's job. */
const packagedSources = import.meta.glob("../src-tauri/themes/*.toml", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

function parsePackaged(id: string, source: string): Theme {
  const read = (key: string): string | null => {
    const found = source.match(new RegExp(`^\\s*${key}\\s*=\\s*"([^"]*)"`, "m"));
    return found ? found[1] : null;
  };
  return {
    id,
    name: read("name") ?? id,
    text: read("text") ?? "#000000",
    background: read("background") ?? "#ffffff",
    accent: read("accent"),
    link: read("link"),
    // `selection` is what this key used to be called, and a theme on somebody's
    // disk may still say it — see the alias in `theme.rs`, which is the same
    // decision on the side that owns the files.
    selection_area: read("selection_area") ?? read("selection"),
    selection_text: read("selection_text"),
    recolor: !/^\s*recolor\s*=\s*false/m.test(source),
    built_in: true,
  };
}

/* Where a shipped theme sits in the list. Absent puts it at the end rather
 * than at the front, so a file that forgets it is odd rather than disruptive;
 * `build.rs` refuses to build one, which is the loud half of the same check. */
function orderOf(source: string): number {
  const found = source.match(/^\s*order\s*=\s*(-?\d+)/m);
  return found ? Number(found[1]) : Number.MAX_SAFE_INTEGER;
}

const fallbackThemes: Theme[] = Object.entries(packagedSources)
  .map(([path, source]) => ({ id: path.slice(path.lastIndexOf("/") + 1, -".toml".length), source }))
  .sort((a, b) => orderOf(a.source) - orderOf(b.source) || a.id.localeCompare(b.id))
  .map(({ id, source }) => parsePackaged(id, source));

function fallbackSettings(): Settings {
  try {
    return { ...fallbackDefaults, ...JSON.parse(localStorage.getItem(FALLBACK_KEY) || "{}") };
  } catch {
    return { ...fallbackDefaults };
  }
}

/* -------------------------------------------------------------- commands */

export async function bootstrap(): Promise<Bootstrap> {
  if (!hasBackend) {
    return {
      settings: fallbackSettings(),
      themes: fallbackThemes,
      library: [],
      open_document: "",
      config_dir: "(browser)",
      themes_dir: "(browser)",
    };
  }
  return invoke<Bootstrap>("bootstrap");
}

/** Write settings. Nothing else in the file is touched.
 *
 * Plural because the interface almost always changes settings in groups — a
 * theme and the light or dark slot it fills, a zoom and the fit mode that goes
 * with it — and each call is a whole-file rewrite on the other side. Sending
 * the group as one call makes it one write, and means the group can never be
 * seen half-applied. */
export async function setSettings(
  entries: [keyof Settings, Settings[keyof Settings]][],
): Promise<void> {
  if (entries.length === 0) return;
  if (!hasBackend) {
    const stored = fallbackSettings();
    for (const [key, value] of entries) (stored as Record<string, unknown>)[key] = value;
    localStorage.setItem(FALLBACK_KEY, JSON.stringify(stored));
    return;
  }
  await invoke("set_settings", { entries });
}

/** What `keys.toml` says, as it is written.
 *
 * Nothing is interpreted on the way through: the action names and the grammar
 * of a chord both live in `keys.ts`, which is the side that has to turn a
 * keystroke into one. Rust reports only what TOML itself can describe and the
 * frontend cannot use — a key bound to a number, an entry that is a table. */
export async function loadKeys(): Promise<Keys> {
  if (!hasBackend) {
    try {
      const stored = JSON.parse(localStorage.getItem(KEYS_FALLBACK_KEY) || "{}");
      return { bindings: stored, problems: [] };
    } catch {
      return { bindings: {}, problems: ["Your keys could not be read."] };
    }
  }
  return invoke<Keys>("load_keys");
}

export async function listThemes(): Promise<Theme[]> {
  if (!hasBackend) return fallbackThemes;
  return invoke<Theme[]>("list_themes");
}

export async function saveTheme(theme: Omit<Theme, "built_in">): Promise<Theme> {
  if (!hasBackend) throw new Error("Only the app can save a theme to disk.");
  return invoke<Theme>("save_theme", { theme: { ...theme, built_in: false } });
}

export async function deleteTheme(id: string): Promise<Theme[]> {
  if (!hasBackend) throw new Error("Only the app can delete a theme from disk.");
  return invoke<Theme[]>("delete_theme", { id });
}

export async function pickPdf(): Promise<string | null> {
  if (!hasBackend) return browsePdf();
  return invoke<string | null>("pick_pdf");
}

export async function openDocument(path: string): Promise<OpenedDocument> {
  if (!hasBackend) {
    return { path, name: path.split("/").pop() || path, page: 1, offset: 0 };
  }
  return invoke<OpenedDocument>("open_document", { path });
}

/* --------------------------------------------------- reading a document

   A document is read in pieces rather than all at once. pdf.js asks for the
   cross-reference table at the end of the file and then only the pages being
   looked at, so nothing here ever holds a whole PDF — which used to mean three
   copies of it, one on each side of the bridge and one in the pdf.js worker. */

/** Open a document for reading and learn how long it is. Reads nothing. */
export async function openForReading(path: string): Promise<number> {
  const local = browserFiles.get(path);
  if (local) return local.size;
  return invoke<number>("open_for_reading", { path });
}

/** Bytes `[start, start + length)` of the document opened for reading. */
export async function readRange(
  path: string,
  start: number,
  length: number,
): Promise<Uint8Array> {
  const local = browserFiles.get(path);
  if (local) {
    return new Uint8Array(await local.slice(start, start + length).arrayBuffer());
  }
  const bytes = await invoke<ArrayBuffer>("read_range", { path, start, length });
  return new Uint8Array(bytes);
}

/** Let go of the document, so its handle does not outlive the reading. */
export async function closeReading(): Promise<void> {
  if (!hasBackend) return;
  await invoke("close_document").catch(() => {});
}

/** Write bytes pdf.js produced — an incremental update carrying a highlight
    — over the document a window has open, and let the reload that follows
    pick it back up. See `write_document` in lib.rs: on the real backend the
    reload is fired from there, as the same `document-changed` event a
    recompiled document arrives through, so `App.reload` needs nothing new to
    handle it. The browser fallback has no such event to ride, so it raises
    its own — see `onDocumentChanged` below — the moment the bytes land in
    `browserFiles`, which is what makes the whole gesture testable in the
    harness with no Rust behind it. `Array.from` rather than handing the
    typed array straight to `invoke`: `bytes` travels inside an ordinary JSON
    argument object alongside `path`, not through the raw-body path
    `read_range`'s *response* uses, so it has to already be a plain array of
    numbers for `Vec<u8>` on the other side to read. */
export async function writeDocument(path: string, bytes: Uint8Array): Promise<void> {
  if (!hasBackend) {
    const name = browserFiles.get(path)?.name ?? path;
    browserFiles.set(path, new File([bytes as BlobPart], name, { type: "application/pdf" }));
    for (const handler of browserDocumentChangedHandlers) handler(path);
    return;
  }
  await invoke("write_document", { path, bytes: Array.from(bytes) });
}

export async function rememberPosition(
  path: string,
  page: number,
  offset: number,
): Promise<void> {
  if (!hasBackend) return;
  await invoke("remember_position", { path, page, offset });
}

export async function forgetDocument(path: string): Promise<LibraryEntry[]> {
  if (!hasBackend) return [];
  return invoke<LibraryEntry[]>("forget_document", { path });
}

/** The name a document gives itself, kept for the recently-read list. */
export async function setDocumentTitle(path: string, title: string): Promise<LibraryEntry[]> {
  if (!hasBackend) return [];
  return invoke<LibraryEntry[]>("set_document_title", { path, title });
}

/** Put a pin in a page, or take the same pin out. */
export async function toggleMark(
  path: string,
  page: number,
  offset: number,
  title: string,
): Promise<{ marked: boolean; marks: Mark[] }> {
  if (!hasBackend) {
    // The browser path keeps them for as long as the page is open, which is
    // as long as anything else lives there.
    const held = browserMarks.get(path) ?? [];
    const at = held.findIndex((mark) => mark.page === page);
    if (at >= 0) held.splice(at, 1);
    else held.push({ page, offset, title, at: Date.now() });
    held.sort((one, other) => one.page - other.page);
    browserMarks.set(path, held);
    return { marked: at < 0, marks: [...held] };
  }
  return invoke<{ marked: boolean; marks: Mark[] }>("toggle_mark", {
    path,
    page,
    offset,
    title,
  });
}

/** Marks, for the browser path. Rust keeps the real ones in `library.toml`. */
const browserMarks = new Map<string, Mark[]>();

/** Journal one highlight the reader just drew. Returns the document's
    highlights as they now stand. */
export async function addHighlight(path: string, highlight: Highlight): Promise<Highlight[]> {
  if (!hasBackend) {
    const held = browserHighlights.get(path) ?? [];
    held.push(highlight);
    browserHighlights.set(path, held);
    return [...held];
  }
  return invoke<Highlight[]>("add_highlight", { path, highlight });
}

/** Take a highlight out of the journal, by the id it was added with. */
export async function removeHighlight(path: string, id: string): Promise<Highlight[]> {
  if (!hasBackend) {
    const held = (browserHighlights.get(path) ?? []).filter((h) => h.id !== id);
    browserHighlights.set(path, held);
    return [...held];
  }
  return invoke<Highlight[]>("remove_highlight", { path, id });
}

/** Replace a document's journaled highlights with what the file itself says,
    once it has been read — see `library::set_highlights` for why this
    discards rather than merges. */
export async function setHighlights(path: string, highlights: Highlight[]): Promise<void> {
  if (!hasBackend) {
    browserHighlights.set(path, [...highlights]);
    return;
  }
  await invoke("set_highlights", { path, highlights });
}

/** Highlights, for the browser path. Rust keeps the real ones in
    `library.toml`. */
const browserHighlights = new Map<string, Highlight[]>();

/** Note what this window is showing, for the next launch. `null` means it is
    showing nothing. Rust keeps one answer per window and writes them together
    — see `OpenDocuments` in lib.rs. */
export async function setOpenDocument(path: string | null): Promise<void> {
  if (!hasBackend) return;
  await invoke("set_open_document", { path });
}

/** A second window, with a document in it or with nothing.
 *
 * The whole interface is one `App` in one webview, so a window is a complete
 * second reader — its own viewer, search index and sidebar — and everything
 * that is the app's rather than a window's (settings, themes, the library)
 * stays where it is, shared by one process. In a plain browser the analogue is
 * a second tab: it can be opened, and it cannot be handed a path, because the
 * browser path has no file names to hand it. */
export async function newWindow(path: string | null = null): Promise<void> {
  if (!hasBackend) {
    window.open(location.href, "_blank");
    return;
  }
  await invoke("new_window", { path });
}

/** Hand a link from a document to whatever opens web pages here. */
export async function openExternal(url: string): Promise<void> {
  if (!hasBackend) {
    window.open(url, "_blank", "noopener");
    return;
  }
  await invoke("open_link", { url });
}

/** Show a document where it lives, in Finder, Explorer, or whatever this
    system browses files with. */
export async function revealDocument(path: string): Promise<void> {
  if (!hasBackend) throw new Error("Only the app can show a file on disk.");
  await invoke("reveal_document", { path });
}

/** Open a file or folder with whatever this system opens it with by
    default — a text editor for a settings file, the file manager for a
    folder. */
export async function openPath(path: string): Promise<void> {
  if (!hasBackend) throw new Error("Only the app can open a file on disk.");
  await invoke("open_path", { path });
}

/** What this system prints PDFs with, by name, for the sentence that says so. */
export const systemViewerName = isMac ? "Preview" : "your PDF viewer";

/** Hand a document to a program that prints. HyloPDF does not — see
    `print_document` in lib.rs for why not. */
export async function printDocument(path: string): Promise<void> {
  if (!hasBackend) {
    window.print();
    return;
  }
  await invoke("print_document", { path });
}

/** The name of whatever shows files here, for a menu item that has to say
    where it is about to take you. */
export const fileManagerName = isMac
  ? "Finder"
  : /win/i.test(navigator.platform || navigator.userAgent)
    ? "File Explorer"
    : "the file manager";

/** Put text on the clipboard. The modern route needs a secure context and a
    gesture behind it, both of which a menu click has; the older one is there
    for when it is refused anyway. */
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    // Fall through to the older route.
  }
  const field = document.createElement("textarea");
  field.value = text;
  field.setAttribute("readonly", "");
  field.style.position = "fixed";
  field.style.opacity = "0";
  document.body.append(field);
  field.select();
  const copied = document.execCommand("copy");
  field.remove();
  if (!copied) throw new Error("Nothing could be put on the clipboard.");
}

/** A line in the terminal running `tauri dev`.
 *
 * The webview has no terminal of its own, so this is the only way anything it
 * says reaches the one place a developer is looking. It lives here for the
 * same reason everything else does: this file is the only door into Rust, and
 * a `log` that went straight to `invoke` from `main.ts` made that false — the
 * claim is load-bearing (it is half of why the renderer is replaceable) and it
 * is only worth anything if one `grep` can still settle it. */
export function log(message: string): void {
  if (!hasBackend) return;
  void invoke("log", { message }).catch(() => {});
}

export async function saveWindowState(): Promise<void> {
  if (!hasBackend) return;
  await invoke("save_window_state");
}

/** Show the window, and collect the document the app was started with. */
export async function signalReady(): Promise<string | null> {
  if (!hasBackend) return null;
  return invoke<string | null>("ready");
}

/* ---------------------------------------------------------------- window */

export async function setFullscreen(on: boolean): Promise<void> {
  if (!hasBackend) {
    if (on) await document.documentElement.requestFullscreen().catch(() => {});
    else if (document.fullscreenElement) await document.exitFullscreen().catch(() => {});
    return;
  }
  await getCurrentWindow().setFullscreen(on);
}

export async function isFullscreen(): Promise<boolean> {
  if (!hasBackend) return Boolean(document.fullscreenElement);
  return getCurrentWindow().isFullscreen();
}

/** The close, minimise and zoom buttons. On a Mac they sit over the document,
    so they come and go with the toolbar; everywhere else the window has a real
    title bar and this is nobody's business but the system's. */
export async function setTitlebarButtons(visible: boolean): Promise<void> {
  if (!hasBackend) return;
  await invoke("set_titlebar_buttons", { visible });
}

/** Whether the window itself has the keyboard, which is not the same question
    as whether the page inside it does. */
export async function isWindowFocused(): Promise<boolean> {
  if (!hasBackend) return document.hasFocus();
  return getCurrentWindow().isFocused();
}

export async function focusWindow(): Promise<void> {
  if (!hasBackend) return;
  await getCurrentWindow().setFocus();
}

export async function setWindowTitle(title: string): Promise<void> {
  document.title = title;
  if (hasBackend) await getCurrentWindow().setTitle(title);
}

/** Documents the OS asks us to open: "Open with", the dock, a second launch.
 *
 * Listened for on this window rather than on the app, and that matters: a
 * plain `listen` registers for *any* target and hears everything, so a
 * document meant for the empty window over there would be opened by every
 * window at once. Rust picks the window and says so by name. */
export async function onExternalDocument(
  handler: (path: string) => void,
): Promise<void> {
  if (!hasBackend) return;
  await getCurrentWindow().listen<string>("open-document", (event) =>
    handler(event.payload),
  );
}

/** Theme files rewritten on the disk, by an editor or by the app itself.
    The whole set comes with the event — fourteen themes of five colours each
    at minimum, so asking again would cost more than sending it. */
export async function onThemesChanged(
  handler: (themes: Theme[]) => void,
): Promise<void> {
  if (!hasBackend) return;
  await listen<Theme[]>("themes-changed", (event) => handler(event.payload));
}

/** The open document, rewritten underneath the reader — a paper recompiled,
    usually, or this app's own `writeDocument`. Rust only says so once what
    is on the disk is a whole PDF again.

    The browser fallback has no window to emit an event to, so it keeps its
    own list of handlers and `writeDocument` above calls them directly —
    the same shape, so `App.reload` does not need to know which backend it
    is running against. */
export async function onDocumentChanged(
  handler: (path: string) => void,
): Promise<void> {
  if (!hasBackend) {
    browserDocumentChangedHandlers.push(handler);
    return;
  }
  // This window's document, and no other's — see `onExternalDocument`.
  await getCurrentWindow().listen<string>("document-changed", (event) =>
    handler(event.payload),
  );
}

const browserDocumentChangedHandlers: ((path: string) => void)[] = [];

export async function onFileDrop(handlers: {
  hover: () => void;
  cancel: () => void;
  drop: (paths: string[]) => void;
}): Promise<void> {
  if (!hasBackend) return;
  await getCurrentWindow().onDragDropEvent((event) => {
    if (event.payload.type === "over") handlers.hover();
    else if (event.payload.type === "drop") handlers.drop(event.payload.paths);
    else handlers.cancel();
  });
}

/** Fires whenever the window is moved or resized, so its geometry can be
    written down without waiting for the app to quit. */
export async function onWindowGeometryChange(handler: () => void): Promise<void> {
  if (!hasBackend) return;
  const window = getCurrentWindow();
  await window.onResized(() => handler());
  await window.onMoved(() => handler());
}

/** Close this window. It was the whole app when there was only ever one; with
    more than one it is the window, and the app goes when the last one does.
    This asks for it the same way the close button does — the close handler
    below runs first, so the place in the document and anything not yet
    written are saved on the way out. In a browser there is no window of ours
    to close, so this does nothing. */
export async function closeWindow(): Promise<void> {
  if (!hasBackend) return;
  await getCurrentWindow().close();
}

/** Close every window, which off a Mac is what quitting is.
 *
 * Closing them rather than exiting outright, because each window's close
 * handler is where its position, its settings and its geometry are written
 * down — `app.exit` would take the process out from under all three, and
 * "come back to where I stopped" is the one promise this app makes about what
 * survives a quit. */
export async function quitApp(): Promise<void> {
  if (!hasBackend) return;
  await invoke("quit_app");
}

export async function onCloseRequested(handler: () => Promise<void>): Promise<void> {
  if (!hasBackend) return;
  await getCurrentWindow().onCloseRequested(async () => {
    await handler();
  });
}

/* ------------------------------------------- browser-only file selection */

/** The documents this page can still read, standing in for the file handle
 *  Rust would be holding.
 *
 * Only the last few. A `File` is a handle rather than the bytes, but it pins
 * whatever is behind it, and this used to be every document ever picked or
 * dropped for as long as the tab was open. The recents list is six, so this
 * outlives anything the interface can still offer to reopen. */
const browserFiles = new Map<string, File>();
const BROWSER_FILE_LIMIT = 8;

function rememberBrowserFile(file: File): string {
  // Re-inserting moves it to the end, which is what makes this an LRU.
  browserFiles.delete(file.name);
  browserFiles.set(file.name, file);
  for (const name of browserFiles.keys()) {
    if (browserFiles.size <= BROWSER_FILE_LIMIT) break;
    browserFiles.delete(name);
  }
  return file.name;
}

function browsePdf(): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "application/pdf";
    // `change` does not fire when the picker is dismissed, so a promise that
    // only listened for it never settled: every cancelled Open left one
    // pending forever, and the `await` in `openDialog` behind it. `cancel` is
    // the event for that, and the focus check is for the engines that do not
    // send it — the window getting the keyboard back with no file chosen
    // means the picker has been and gone.
    let settled = false;
    const done = (answer: string | null) => {
      if (settled) return;
      settled = true;
      window.removeEventListener("focus", onFocus);
      resolve(answer);
    };
    // Focus can come back before `change` lands, so give it a moment to. If
    // `change` still hasn't fired by then, check `input.files` directly rather
    // than assuming cancelled: the spec updates it before queuing the event
    // that reports it, so a picker slow enough to blow the 300ms budget — a
    // network volume, a slow GTK dialog — still leaves the answer sitting
    // there to be read instead of silently dropping it.
    const onFocus = () =>
      setTimeout(() => {
        const file = input.files?.[0];
        done(file ? rememberBrowserFile(file) : null);
      }, 300);
    input.addEventListener("change", () => {
      const file = input.files?.[0];
      done(file ? rememberBrowserFile(file) : null);
    });
    input.addEventListener("cancel", () => done(null));
    window.addEventListener("focus", onFocus, { once: true });
    input.click();
  });
}

export function registerBrowserFile(file: File): string {
  return rememberBrowserFile(file);
}
