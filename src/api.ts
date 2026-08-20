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
  recolor_images: true,
  remember_position: true,
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

const fallbackThemes: Theme[] = [
  { id: "hylo-light", name: "Hylo Light", text: "#2f3237", background: "#f2f1ed", accent: "#3f7d94", link: "#2f6f8f", recolor: false, built_in: true },
  { id: "hylo-dark", name: "Hylo Dark", text: "#e9eaee", background: "#24272f", accent: "#8fb0d4", link: "#8ec5e8", recolor: true, built_in: true },
  { id: "pzazz", name: "Pzazz", text: "#f4ecff", background: "#1c1526", accent: "#ff5fa2", link: "#5fe3c8", recolor: true, built_in: true },
  { id: "dracula", name: "Dracula", text: "#f8f8f2", background: "#282a36", accent: "#ff79c6", link: "#8be9fd", recolor: true, built_in: true },
  { id: "gruvbox", name: "Gruvbox", text: "#ebdbb2", background: "#282828", accent: "#fe8019", link: "#83a598", recolor: true, built_in: true },
];

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

/** Write exactly one setting. Nothing else in the file is touched. */
export async function setSetting<K extends keyof Settings>(
  key: K,
  value: Settings[K],
): Promise<void> {
  if (!hasBackend) {
    const stored = fallbackSettings();
    stored[key] = value;
    localStorage.setItem(FALLBACK_KEY, JSON.stringify(stored));
    return;
  }
  await invoke("set_setting", { key, value });
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

export async function readDocument(path: string): Promise<Uint8Array> {
  const local = browserFiles.get(path);
  if (local) return new Uint8Array(await local.arrayBuffer());
  const bytes = await invoke<ArrayBuffer>("read_document", { path });
  return new Uint8Array(bytes);
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

export async function onCloseRequested(handler: () => Promise<void>): Promise<void> {
  if (!hasBackend) return;
  await getCurrentWindow().onCloseRequested(async () => {
    await handler();
  });
}

/* ------------------------------------------- browser-only file selection */

const browserFiles = new Map<string, File>();

function browsePdf(): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "application/pdf";
    input.addEventListener("change", () => {
      const file = input.files?.[0];
      if (!file) return resolve(null);
      browserFiles.set(file.name, file);
      resolve(file.name);
    });
    input.click();
  });
}

export function registerBrowserFile(file: File): string {
  browserFiles.set(file.name, file);
  return file.name;
}
