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
  scroll_mode: "continuous" | "paged";
  fit_mode: "width" | "page" | "actual";
  zoom: number;
  page_gap: number;
  recolor_images: boolean;
  remember_position: boolean;
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
};

export type Theme = {
  id: string;
  name: string;
  text: string;
  background: string;
  accent: string | null;
  link: string | null;
  /** The colour behind selected text. Null means "derive it from the accent". */
  selection: string | null;
  /** The colour selected text is drawn in. Null means "derive it from the
      colour behind it". */
  selection_text: string | null;
  recolor: boolean;
  built_in: boolean;
};

export type LibraryEntry = {
  path: string;
  title: string;
  page: number;
  offset: number;
  opened_at: number;
};

export type Bootstrap = {
  settings: Settings;
  themes: Theme[];
  library: LibraryEntry[];
  config_dir: string;
  themes_dir: string;
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

const fallbackDefaults: Settings = {
  theme: "hylo-light",
  light_theme: "hylo-light",
  dark_theme: "hylo-dark",
  scroll_mode: "continuous",
  fit_mode: "width",
  zoom: 1,
  page_gap: 16,
  recolor_images: false,
  remember_position: true,
  search_highlight_all: true,
  search_match_case: false,
  search_whole_words: false,
  show_toolbar: true,
  show_sidebar: false,
  sidebar_width: 232,
  fullscreen: false,
  window_width: 1280,
  window_height: 860,
  window_x: null,
  window_y: null,
  window_maximized: true,
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
 * The parsing is a fraction of TOML — a flat table of quoted strings and
 * booleans, which is all a theme file is. Anything cleverer is Rust's job. */
const packagedOrder = ["hylo-light", "hylo-dark", "glamour", "dracula", "gruvbox", "sepia", "high-contrast"];

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
    selection: read("selection"),
    selection_text: read("selection_text"),
    recolor: !/^\s*recolor\s*=\s*false/m.test(source),
    built_in: true,
  };
}

const fallbackThemes: Theme[] = packagedOrder.flatMap((id) => {
  const source = packagedSources[`../src-tauri/themes/${id}.toml`];
  return source ? [parsePackaged(id, source)] : [];
});

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

/** Files dropped on the window, and documents the OS asks us to open. */
export async function onExternalDocument(
  handler: (path: string) => void,
): Promise<void> {
  if (!hasBackend) return;
  await listen<string>("open-document", (event) => handler(event.payload));
}

/** Theme files rewritten on the disk, by an editor or by the app itself.
    The whole set comes with the event — there are seven of them and a handful
    of colours each, so asking again would cost more than sending it. */
export async function onThemesChanged(
  handler: (themes: Theme[]) => void,
): Promise<void> {
  if (!hasBackend) return;
  await listen<Theme[]>("themes-changed", (event) => handler(event.payload));
}

/** The open document, rewritten underneath the reader — a paper recompiled,
    usually. Rust only says so once what is on the disk is a whole PDF again. */
export async function onDocumentChanged(
  handler: (path: string) => void,
): Promise<void> {
  if (!hasBackend) return;
  await listen<string>("document-changed", (event) => handler(event.payload));
}

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

/** Ask for the window to go, the same way its close button does — the close
    handler below runs first, so the place in the document and anything not
    yet written are saved on the way out. In a browser there is no window of
    ours to close, so this does nothing. */
export async function quitApp(): Promise<void> {
  if (!hasBackend) return;
  await getCurrentWindow().close();
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
