/* HyloPDF.
 *
 * One object holds the state — settings, themes, the open document — and every
 * change goes through it, so a setting written to disk and a setting shown in
 * the interface can never disagree. Settings are written one key at a time;
 * nothing here ever saves the whole blob. */

import type { PDFDocumentProxy } from "pdfjs-dist/types/src/display/api";

import {
  type Bootstrap,
  type LibraryEntry,
  type Mark,
  type Settings,
  type Theme,
  bootstrap,
  closeReading,
  copyText,
  deleteTheme,
  fileManagerName,
  forgetDocument,
  hasBackend,
  isFullscreen,
  isWindowFocused,
  isMac,
  listThemes,
  onCloseRequested,
  onDocumentChanged,
  onExternalDocument,
  onFileDrop,
  onThemesChanged,
  onWindowGeometryChange,
  openDocument,
  openExternal,
  pickPdf,
  printDocument,
  quitApp,
  closeWindow,
  newWindow,
  registerBrowserFile,
  rememberPosition,
  toggleMark,
  setDocumentTitle,
  setOpenDocument,
  revealDocument,
  saveWindowState,
  focusWindow,
  setFullscreen,
  setSettings,
  setTitlebarButtons,
  setWindowTitle,
  systemViewerName,
  signalReady,
  loadKeys,
  log,
} from "./api";

import { hydrateIcons, iconMarkup } from "./icons";
import { type Action, type Keymap, buildKeymap, chordsOf, needsDocument } from "./keys";
import { type SearchState, Search } from "./search";
import { isEditingTheme, refreshSettingsWindow, showSettingsWindow } from "./settings";
import { Sidebar } from "./sidebar";
import { applyTheme, isDarkTheme, unreadableColors } from "./themes";
import * as ui from "./ui";
import { Cancelled, type FitMode, type SpreadMode, Viewer } from "./viewer";

if (import.meta.env.DEV && hasBackend) {
  // The webview has no terminal of its own; send what it says to the one
  // running `tauri dev`.
  const forward = (kind: string, parts: unknown[]) =>
    log(`${kind}: ${parts.map(String).join(" ")}`);
  for (const kind of ["log", "warn", "error"] as const) {
    const original = console[kind].bind(console);
    console[kind] = (...parts: unknown[]) => {
      forward(kind, parts);
      original(...parts);
    };
  }
  window.addEventListener("error", (event) => forward("uncaught", [event.message]));
  window.addEventListener("unhandledrejection", (event) =>
    forward("unhandled", [String(event.reason)]),
  );
}

/** Whether the machine is in dark mode. In a webview this is the system
    setting, which is what it is on every platform the app ships to. */
const darkOutside = () => window.matchMedia("(prefers-color-scheme: dark)");

const ZOOM_LADDER = [25, 33, 50, 67, 75, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400, 600];

/** What to tell someone to press for full screen. The Mac also answers to
    ⌃⌘F, which is what every other app there uses, but ⌘⇧F is the one worth
    naming: ⌘F is taken by find, unlike ⌘T for the toolbar. */
const FULLSCREEN_KEYS = isMac ? "⌘⇧F" : "F11";

/** Preview's own "Go to Page…", which is the one people already know. */
const JUMP_KEYS = isMac ? "⌥⌘G" : "Ctrl+Alt+G";

const el = {
  shell: byId<HTMLDivElement>("shell"),
  toolbar: byId<HTMLElement>("toolbar"),
  open: byId<HTMLButtonElement>("open"),
  contents: byId<HTMLButtonElement>("contents"),
  closeDoc: byId<HTMLButtonElement>("close-doc"),
  title: byId<HTMLButtonElement>("doc-title"),
  prevPage: byId<HTMLButtonElement>("prev-page"),
  nextPage: byId<HTMLButtonElement>("next-page"),
  pageNumber: byId<HTMLInputElement>("page-number"),
  pageCount: byId<HTMLSpanElement>("page-count"),
  find: byId<HTMLButtonElement>("find"),
  zoomOut: byId<HTMLButtonElement>("zoom-out"),
  zoomIn: byId<HTMLButtonElement>("zoom-in"),
  zoomLevel: byId<HTMLButtonElement>("zoom-level"),
  theme: byId<HTMLButtonElement>("theme"),
  settings: byId<HTMLButtonElement>("settings"),
  sidebar: byId<HTMLElement>("sidebar"),
  sidebarGrip: byId<HTMLDivElement>("sidebar-grip"),
  outlinePanel: byId<HTMLDivElement>("outline-panel"),
  pagesPanel: byId<HTMLDivElement>("pages-panel"),
  resultsPanel: byId<HTMLDivElement>("results-panel"),
  viewer: byId<HTMLDivElement>("viewer"),
  pages: byId<HTMLDivElement>("pages"),
  welcome: byId<HTMLElement>("welcome"),
  welcomeOpen: byId<HTMLButtonElement>("welcome-open"),
  newWindow: byId<HTMLButtonElement>("new-window"),
  quit: byId<HTMLButtonElement>("quit"),
  recents: byId<HTMLDivElement>("recents"),
  findBar: byId<HTMLDivElement>("find-bar"),
  findInput: byId<HTMLInputElement>("find-input"),
  findStatus: byId<HTMLButtonElement>("find-status"),
  findPrev: byId<HTMLButtonElement>("find-prev"),
  findNext: byId<HTMLButtonElement>("find-next"),
  findClose: byId<HTMLButtonElement>("find-close"),
  findHighlight: byId<HTMLButtonElement>("find-highlight"),
  findCase: byId<HTMLButtonElement>("find-case"),
  findWords: byId<HTMLButtonElement>("find-words"),
  pagePill: byId<HTMLDivElement>("page-pill"),
  toolbarPeek: byId<HTMLButtonElement>("toolbar-peek"),
  titleDrag: byId<HTMLDivElement>("title-drag"),
  dropHint: byId<HTMLDivElement>("drop-hint"),
};

function byId<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`missing element: ${id}`);
  return element as T;
}

class App {
  settings!: Settings;
  themes: Theme[] = [];
  theme!: Theme;
  library: LibraryEntry[] = [];
  paths = { config: "", themes: "" };

  viewer: Viewer;
  sidebar: Sidebar;
  search: Search;

  private path: string | null = null;
  private saveTimer = 0;
  /** Settings changed but not yet written, and the timer that will write them.
      Keyed so the last value for a key wins. */
  private pendingWrites = new Map<keyof Settings, Settings[keyof Settings]>();
  private writeTimer = 0;
  /** When the queued write is due, so a later deadline cannot displace an
      earlier one. */
  private writeAt = 0;
  private pillTimer = 0;
  /** The toolbar shown for a page jump, without being asked to stay: it goes
      back into hiding the moment the field is left, and never touches the
      setting or writes to disk. */
  private toolbarPeeking = false;
  private geometryTimer = 0;
  private fullscreenTimer = 0;
  private searchTimer = 0;
  private searchPending = false;
  /** Whether the open document numbers its own pages, as last shown. */
  private labelled = false;
  /** Whether the reader is presenting, and what the toolbar was doing before
      they were. See `togglePresentation`. */
  presenting = false;
  private toolbarBeforePresenting = true;
  /** Said once per document: there is no text in this one to search. */
  private saidTextless = false;
  /** What the keyboard is bound to, and what of `keys.toml` could not be
      read. Rebuilt when the reader edits the file and presses Reload. */
  keymap: Keymap = buildKeymap();
  /** The first half of a sequence — `g`, waiting to find out whether it is
      `g g` — and the timer that gives up on it. */
  private pendingChord = "";
  private pendingTimer = 0;

  constructor() {
    this.viewer = new Viewer(el.viewer, el.pages, {
      onPageChange: (page, count) => this.onPageChange(page, count),
      onScroll: () => this.onScroll(),
      onError: (message) => ui.notice(message),
      onExternalLink: (url) => void this.openLink(url),
      onNote: (note) => this.showNote(note),
      onPassword: (wrong) => ui.askForPassword(wrong),
    });
    this.sidebar = new Sidebar(
      el.outlinePanel,
      el.pagesPanel,
      el.resultsPanel,
      [...el.sidebar.querySelectorAll<HTMLButtonElement>(".tab")],
      this.viewer,
    );
    this.search = new Search(this.viewer, (state) => this.onSearchUpdate(state));
  }

  /* ------------------------------------------------------------- startup */

  async start(): Promise<void> {
    hydrateIcons();
    // index.html has no `<html>` element — vite serves it as it is and the
    // browser implies the rest of the document around it — so this is the only
    // place the page's language can be stated. Without it a screen reader
    // reads the interface in whatever voice it was last set to.
    document.documentElement.lang = "en";
    el.shell.dataset.platform = isMac ? "macos" : "other";

    const data: Bootstrap = await bootstrap();
    this.settings = data.settings;
    this.themes = data.themes;
    this.library = data.library;
    this.paths = { config: data.config_dir, themes: data.themes_dir };
    this.theme = this.themeById(this.settings.theme);
    // Whether *this* window is full screen is the window's answer, not the
    // setting's. The setting is what the last window to change it left behind,
    // and it is Rust that puts the launch window back into full screen — a
    // second window made while the first one is full screen is not full screen
    // itself, and would otherwise spend its life with the chrome of a window
    // it is not. Adopted rather than remembered: nobody chose anything here.
    this.settings.fullscreen = await isFullscreen().catch(() => this.settings.fullscreen);

    // Before anything is painted: the theme this reader is owed may not be the
    // one that was written down, if the machine has changed its mind since.
    this.followSystemTheme();
    darkOutside().addEventListener("change", () => this.followSystemTheme());

    applyTheme(this.theme);
    this.applyChrome();
    this.reportUnreadableColors(this.theme);
    this.viewer.setTheme(this.theme, !this.settings.recolor_images);
    this.viewer.setGap(this.settings.page_gap);
    this.viewer.setTrimMargins(this.settings.trim_margins);
    this.viewer.setScrollMode(this.settings.scroll_mode);
    this.viewer.setSpread(this.settings.spread_mode);
    this.viewer.setFit(this.settings.fit_mode, this.settings.zoom);
    this.applySearchOptions();
    this.renderRecents();
    // Before the first keystroke can arrive, and before `wire` reads the map.
    await this.reloadKeys();
    this.wire();

    // Listen before reporting in: the answer to `ready` may itself be a
    // document, and anything arriving after it comes through as an event.
    await this.listenForDocuments();
    await this.listenForFileChanges();
    const startWith = await signalReady();
    // A document named on the command line, or double-clicked to start the
    // app, beats the one that happened to be open last: it is what this
    // launch was *for*.
    if (startWith) await this.open(startWith);
    else if (this.settings.reopen_last_document && data.open_document) {
      await this.open(data.open_document);
    }

    // Starting up in full screen with the toolbar away means starting up with
    // nothing on screen to press, so say once how to get back out.
    if (this.settings.fullscreen && !this.settings.show_toolbar) {
      ui.notice(`Full screen. Escape or ${FULLSCREEN_KEYS} comes back.`);
    }
  }

  /** Documents from the OS: "Open with", a file dropped on the icon, or one
      named on the command line. */
  private async listenForDocuments(): Promise<void> {
    await onExternalDocument((path) => void this.openFromOutside(path));
    await onFileDrop({
      hover: () => {
        el.dropHint.hidden = false;
      },
      cancel: () => {
        el.dropHint.hidden = true;
      },
      drop: (paths) => {
        el.dropHint.hidden = true;
        const pdf = paths.find((path) => path.toLowerCase().endsWith(".pdf"));
        if (pdf) void this.open(pdf);
        else ui.notice("That is not a PDF.");
      },
    });
  }

  /** A document handed over by the system — "Open with", the dock, the command
   *  line.
   *
   * It used to arrive on top of whatever was already open, because there was
   * one window and it had to give way: double-clicking a file closed the
   * document being read, which is nothing anybody asked for by double-clicking
   * a file. Rust now picks a window with nothing in it, or makes one, and
   * names the window it picked — so by the time this runs, the window it is
   * running in has nothing to lose and there is nothing to say about it. See
   * `hand_over` in lib.rs.
   */
  private async openFromOutside(path: string): Promise<void> {
    await this.open(path);
  }

  /** Another window, with a document in it or with nothing.
   *
   * Nothing here goes with it: a window is a fresh `App` in a fresh webview,
   * and everything it needs — the settings, the themes, the library — is on
   * the Rust side already, shared by the one process. */
  private async newWindow(path: string | null = null): Promise<void> {
    ui.closeMenus();
    this.closeFind();
    await newWindow(path).catch((error) => ui.notice(messageOf(error)));
  }

  /** Files this app reads but does not own: the themes, and the document.
      Rust watches both and says when one of them has really changed. */
  private async listenForFileChanges(): Promise<void> {
    await onThemesChanged((themes) => this.themesChanged(themes));
    await onDocumentChanged((path) => void this.reload(path));
  }

  /** A theme file was written — by hand, by an LLM, or by this app saving one.
   *
   * Whatever is in use is reapplied from the new set, so that editing a theme
   * beside the app shows up in the app. It goes through `useTheme` with
   * `remember` off: nobody chose a theme here, and remembering would write
   * `settings.toml` for every save an editor makes. A theme whose file has
   * gone takes the reader somewhere else rather than leaving the colours of
   * something that no longer exists on screen.
   *
   * None of that applies while a theme is being written. The draft is the live
   * theme for as long as the editor is open — that is how the app around you
   * becomes the preview — and a new one has no id at all, so looking it up in
   * the new set finds nothing and the branch below reads that as "the theme
   * you are reading in has been deleted". It would then throw the preview
   * away, write a theme choice nobody made, and say so out loud. The list
   * still updates, and the window still redraws; the colours on screen belong
   * to the draft until the reader saves it or backs out. */
  private themesChanged(themes: Theme[]): void {
    const before = this.theme;
    this.themes = themes;
    if (isEditingTheme()) {
      refreshSettingsWindow();
      return;
    }
    const current = themes.find((theme) => theme.id === before.id);
    if (current) {
      // Unconditional: the viewer and the sidebar both compare colours before
      // they repaint anything, so reapplying an unchanged theme costs a few
      // CSS variables and nothing else.
      this.useTheme(current, false);
    } else {
      const replacement = this.replacementFor(before);
      if (replacement) {
        this.useTheme(replacement);
        ui.notice(`${before.name} is gone. Now reading in ${replacement.name}.`);
      }
    }
    refreshSettingsWindow();
  }

  /** The open document was rewritten underneath the reader.
   *
   * Where they were is taken from the viewer rather than from the library,
   * because the library only has the last position written down and this is
   * the one place where the two can differ by a whole scroll. A document that
   * got shorter lands on its last page; `scrollTo` clamps. */
  private async reload(path: string): Promise<void> {
    if (path !== this.path) return;
    const at = this.viewer.position();
    await this.open(path);
    if (this.path !== path) return;
    this.viewer.scrollTo(at.page, at.offset);
    ui.notice("Reloaded — the document changed on disk.");
  }

  themeById(id: string): Theme {
    return this.themes.find((theme) => theme.id === id) ?? this.themes[0];
  }

  /** Change a setting: the interface follows at once, the file catches up.
   *
   * Writes are collected and sent together on the next turn of the event loop.
   * Settings almost never move alone — a theme comes with the light or dark
   * slot it fills, a zoom with its fit mode — and each write is a whole-file
   * rewrite on the other side, so sending them one at a time meant two of
   * those for every change, each having to re-read what the other had just
   * done. Grouped, the pair is one write and can never be seen half-applied. */
  set<K extends keyof Settings>(key: K, value: Settings[K]): void {
    this.settings[key] = value;
    this.pendingWrites.set(key, value);
    // Soonest wins. The two methods share a queue but not an urgency, and they
    // used to share the timer as well: a pinch calls `setSoon` every frame and
    // pushed the deadline out each time, so a theme chosen mid-gesture waited
    // for the fingers to stop. A `set` now pulls the deadline in and never
    // lets a `setSoon` push it back out.
    this.scheduleFlush(0);
  }

  /** Like `set`, but for a value that moves many times a second: the interface
      follows every change, the file only the one it settles on. */
  setSoon<K extends keyof Settings>(key: K, value: Settings[K]): void {
    this.settings[key] = value;
    this.pendingWrites.set(key, value);
    this.scheduleFlush(400);
  }

  /** Write what is queued in at most `delay`, never later. */
  private scheduleFlush(delay: number): void {
    const at = Date.now() + delay;
    if (this.writeTimer && at >= this.writeAt) return;
    window.clearTimeout(this.writeTimer);
    this.writeAt = at;
    this.writeTimer = window.setTimeout(() => void this.flushSettings(), delay);
  }

  /** Send whatever is waiting. Awaited on the way out, so nothing typed or
      chosen in the last moments of a session is lost with it. */
  async flushSettings(): Promise<void> {
    window.clearTimeout(this.writeTimer);
    this.writeTimer = 0;
    if (this.pendingWrites.size === 0) return;
    const entries = [...this.pendingWrites.entries()];
    this.pendingWrites.clear();
    await setSettings(entries).catch((error) => ui.notice(messageOf(error)));
  }

  /* ------------------------------------------------------------ document */

  async open(path: string): Promise<void> {
    ui.closeMenus();
    // The viewer lets go of whatever it was holding the moment a new document
    // reaches it, so the place in the old one is written down first — and the
    // pending write from scrolling is dropped, or it would land after the
    // handover and record the new document's position against the old path.
    void this.savePosition();
    window.clearTimeout(this.saveTimer);
    try {
      const opened = await openDocument(path);
      // The viewer reads the document itself, a piece at a time — nothing here
      // ever holds the whole file.
      const doc = await this.viewer.load(path);

      this.path = path;
      el.shell.dataset.empty = "false";
      el.title.textContent = opened.name;
      void setWindowTitle(`${opened.name} — HyloPDF`);
      el.pageCount.textContent = `of ${doc.numPages}`;
      this.search.reset();
      this.saidTextless = false;
      void this.sidebar.setDocument(doc, this.theme);
      this.showMarks();
      void this.reportFormFields(doc);
      void this.adoptDocumentTitle(path, opened.name);

      const start = this.settings.remember_position ? opened : { page: 1, offset: 0 };
      this.viewer.scrollTo(start.page, start.offset);
      this.library = [
        { path, title: opened.name, page: start.page, offset: start.offset, opened_at: 0 },
        ...this.library.filter((entry) => entry.path !== path),
      ];
      this.renderRecents();
      void setOpenDocument(path);
      el.viewer.focus();
    } catch (error) {
      // Choosing not to give a password is not a failure and has nothing to
      // say for itself. The start screen is still the right place to end up.
      if (error instanceof Cancelled) {
        this.clearDocument();
        return;
      }
      console.error("could not open", path, error);
      // The document that was open is already gone — the viewer let go of it
      // before this one turned out to be unreadable — so the start screen is
      // the only honest thing left to show. Leaving the old title and page
      // count over an empty viewer would also leave `path` pointing at a
      // document the viewer no longer has, and the next position written down
      // would be page one of nothing.
      this.clearDocument();
      ui.notice(messageOf(error));
    }
  }

  /**
   * Call the document what it calls itself.
   *
   * `2310.06825v3.pdf` is not a name, and a shelf of them is unreadable — but
   * the file usually knows better, because whatever produced it wrote the
   * title in. So the toolbar, the window and the recently-read list take the
   * document's own title where there is one worth having.
   *
   * "Worth having" is doing real work. A great many PDFs carry a title field
   * filled in by the program that made them and not by anybody: the file name
   * again, the file name of the *source* — "Microsoft Word - report.doc" — or
   * the word "untitled". Each of those is worse than the file name, because
   * it looks deliberate. Anything that fails the test leaves the file name
   * alone, which is what it was before.
   */
  private async adoptDocumentTitle(path: string, fileName: string): Promise<void> {
    const { info } = await this.viewer.details();
    const raw = typeof info.Title === "string" ? info.Title.trim() : "";
    if (this.path !== path || !worthCalling(raw, fileName)) return;

    el.title.textContent = raw;
    el.title.title = fileName;
    void setWindowTitle(`${raw} — HyloPDF`);
    const entry = this.library.find((item) => item.path === path);
    if (entry) entry.title = raw;
    this.renderRecents();
    void setDocumentTitle(path, raw).catch(() => []);
  }

  /** A form that cannot be filled in should say so.
   *
   * pdf.js paints a widget's appearance stream into the page, so a form
   * arrives looking exactly as it does everywhere else — the boxes are there,
   * and so is anything already typed into them. What is not there is any way
   * to type: that needs an interactive annotation layer, which this app does
   * not have. So the fields look live, click like nothing, and leave the
   * reader wondering which half is broken. Said once, when the document
   * opens, rather than on the click — by then they have already tried. */
  private async reportFormFields(doc: PDFDocumentProxy): Promise<void> {
    try {
      const fields = await doc.getFieldObjects();
      if (this.viewer.document !== doc) return;
      if (!fields || Object.keys(fields).length === 0) return;
      ui.notice(
        "This document has form fields. HyloPDF shows what is in them but cannot fill them in.",
      );
    } catch {
      // A document that cannot say whether it has fields is not worth a word.
    }
  }

  /** Put the document down without putting the app down: back to the start
      screen, with the place kept for next time. */
  closeDocument(): void {
    if (!this.path) return;
    ui.closeMenus();
    void this.savePosition();
    this.clearDocument();
  }

  /** Back to the start screen. Whatever was worth keeping about the document
      that was open has already been written down by the time this runs. */
  private clearDocument(): void {
    this.path = null;
    // Nothing is open now, and the next launch should agree. A document that
    // failed to open is cleared for the same reason: coming back to it every
    // launch, and failing every launch, is the worst version of this feature.
    void setOpenDocument(null);

    this.viewer.close();
    // The handle on the file goes here rather than in `viewer.close()`: this
    // is the one path that means "no document open", where opening another one
    // does not.
    void closeReading();
    this.closeFind();
    this.search.forget();
    void this.sidebar.setDocument(null, this.theme);

    el.shell.dataset.empty = "true";
    el.title.textContent = "";
    el.pageNumber.value = "";
    el.pageCount.textContent = "";
    void setWindowTitle("HyloPDF");
    this.renderRecents();
    el.viewer.focus();
  }

  async openDialog(): Promise<void> {
    const path = await pickPdf();
    if (path) await this.open(path);
  }

  /** Pick a document and give it a window of its own, leaving this one alone. */
  private async openInNewWindow(): Promise<void> {
    const path = await pickPdf();
    if (path) await this.newWindow(path);
  }

  /** The toolbar, told where the reader is.
   *
   * The number shown is the one printed on the page, not the page's position
   * in the file — those differ for the whole length of any book with front
   * matter, and the printed one is what a citation, an index and the reader's
   * own eyes are talking about. Where they differ the position is still said,
   * once, beside it: "xii (12 of 340)" is what every other reader shows, and
   * it is the only way to tell that the document is numbering itself. */
  private onPageChange(page: number, count: number): void {
    const label = count > 0 ? this.viewer.label(page) : "";
    const labelled = this.viewer.hasLabels;
    // The labels land a moment after the document opens, so the column of
    // thumbnails is already built and numbered by position when they do.
    if (labelled !== this.labelled) {
      this.labelled = labelled;
      this.sidebar.relabel();
    }
    if (document.activeElement !== el.pageNumber) {
      el.pageNumber.value = label;
    }
    el.pageCount.textContent = count > 0 ? `of ${labelled ? this.viewer.label(count) : count}` : "";
    el.pageNumber.title = labelled
      ? `Page ${label} — ${page} of ${count} in the file. ${JUMP_KEYS}, or g`
      : `Go to a page — ${JUMP_KEYS}, or g`;
    el.pagePill.textContent =
      count === 0 ? "" : labelled ? `${label} (${page} of ${count})` : `${page} of ${count}`;
    this.sidebar.setPage(page);
    this.updateZoomLabel();
  }

  private onScroll(): void {
    if (!this.toolbarShown && this.settings.show_page_pill) this.flashPill();
    window.clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => this.savePosition(), 700);
  }

  /** Write down where the reader is. Returns the write, so the one place that
      has to wait for it — quitting — can. */
  private savePosition(): Promise<void> {
    if (!this.path || !this.settings.remember_position) return Promise.resolve();
    const at = this.viewer.position();
    const entry = this.library.find((item) => item.path === this.path);
    if (entry) {
      entry.page = at.page;
      entry.offset = at.offset;
    }
    return rememberPosition(this.path, at.page, at.offset).catch(() => {});
  }

  private flashPill(): void {
    el.pagePill.classList.add("visible");
    window.clearTimeout(this.pillTimer);
    this.pillTimer = window.setTimeout(() => el.pagePill.classList.remove("visible"), 1100);
  }

  /* --------------------------------------------------------------- chrome */

  applyChrome(): void {
    const sidebar = this.settings.show_sidebar;
    el.shell.dataset.toolbar = this.toolbarShown ? "shown" : "hidden";
    // The drag strip along the top hangs off this: it stands in for the
    // toolbar, and only where a window can still be moved.
    el.shell.dataset.fullscreen = String(this.settings.fullscreen);
    // Whatever the chrome just became, the band starts inert again; it is only
    // ever woken by a hand arriving at the top edge.
    el.titleDrag.classList.remove("armed");
    // The three buttons are furniture of ours in all but name, so they leave
    // with the toolbar. Full screen is the system's own affair: the buttons
    // are out of sight there anyway, and the bar that slides down at the top
    // of the screen is the way back out for anyone who has lost the shortcut.
    void setTitlebarButtons(this.toolbarShown || this.settings.fullscreen);
    el.sidebar.hidden = !sidebar;
    el.sidebar.style.width = `${this.settings.sidebar_width}px`;
    el.contents.setAttribute("aria-pressed", String(sidebar));
    this.updateZoomLabel();
  }

  private get toolbarShown(): boolean {
    return this.settings.show_toolbar || this.toolbarPeeking;
  }

  updateZoomLabel(): void {
    if (this.settings.fit_mode === "width") el.zoomLevel.textContent = "Fit width";
    else if (this.settings.fit_mode === "page") el.zoomLevel.textContent = "Fit page";
    else el.zoomLevel.textContent = `${Math.round(this.settings.zoom * 100)}%`;
  }

  setScrollMode(mode: Settings["scroll_mode"]): void {
    this.set("scroll_mode", mode);
    this.viewer.setScrollMode(mode);
  }

  setPageGap(value: number): void {
    this.set("page_gap", value);
    this.viewer.setGap(value);
  }

  setRecolorImages(on: boolean): void {
    this.set("recolor_images", on);
    this.viewer.setTheme(this.theme, !on);
  }

  setSidebarWidth(value: number): void {
    this.set("sidebar_width", value);
    el.sidebar.style.width = `${value}px`;
    this.viewer.relayout();
    this.sidebar.resize();
  }

  toggleSidebar(show = !this.settings.show_sidebar): void {
    this.set("show_sidebar", show);
    this.applyChrome();
    this.viewer.relayout();
  }

  toggleToolbar(show = !this.settings.show_toolbar): void {
    this.set("show_toolbar", show);
    this.applyChrome();
    this.viewer.relayout();
    // Say once how to undo it. The shortcut is only any use to someone who
    // has heard it, and this is the moment they are listening.
    if (!show && !this.presenting) {
      ui.notice(
        `Toolbar hidden. ${isMac ? "⌘T" : "Ctrl+T"}, or the top edge of the window, brings it back.`,
      );
    }
  }

  /** With the toolbar away, the top edge of the window stands in for it: a
      handle drops into view when the pointer arrives there and puts the bar
      back. Nothing is on screen until someone reaches for it. */
  private wireToolbarPeek(): void {
    el.toolbarPeek.addEventListener("click", () => {
      el.toolbarPeek.classList.remove("visible");
      this.toggleToolbar(true);
    });
    // Reaching the top edge does two things at once, and they belong together:
    // the handle comes down to give the toolbar back, and the band either side
    // of it wakes up as somewhere to take hold of the window. Until then the
    // band is not there at all, so the top of the page is still the page.
    window.addEventListener("pointermove", (event) => {
      if (this.settings.show_toolbar) return;
      // In full screen the top of the window is not reliably ours: reaching
      // for it slides the menu bar and the title bar down over it, and while
      // they are there the page hears nothing from that band at all. Waiting
      // for the last eight pixels means catching the pointer only in the
      // instant before the system bar arrives, which is why it worked about
      // one try in three. So in full screen the handle answers from further
      // down, well clear of anything the system puts on top.
      const reach = this.settings.fullscreen ? 46 : 8;
      if (event.clientY <= reach) {
        el.toolbarPeek.classList.add("visible");
        if (event.clientY <= 8) el.titleDrag.classList.add("armed");
        return;
      }
      // The handle sits lower in full screen, and the hand has to travel down
      // to reach it. Going away while it is being reached for is the one thing
      // it must not do, so it stays while the pointer is on it and for a good
      // way below where it can appear.
      const onHandle = el.toolbarPeek.contains(event.target as Node);
      if (!onHandle && event.clientY > 110) el.toolbarPeek.classList.remove("visible");
      // The drag strip is only as tall as the band it covers, and it is not
      // there in full screen at all, so it can stand down sooner.
      if (event.clientY > 64) el.titleDrag.classList.remove("armed");
    });

    // The strip lies over the document, so a wheel that lands on it would
    // otherwise stop dead. Hand it on: scrolling past the top of the window
    // should feel like there is nothing there, because there almost isn't.
    el.titleDrag.addEventListener(
      "wheel",
      (event) => {
        event.preventDefault();
        el.viewer.scrollBy({ left: event.deltaX, top: event.deltaY });
      },
      { passive: false },
    );
  }

  /** Full screen is about the window and nothing else: it fills the screen.
      Whether the toolbar and the sidebar are there is their own business, and
      stays exactly as it was on the way in and on the way out — a document
      with no chrome at all is the two switches together, deliberately. */
  async toggleFullscreen(on = !this.settings.fullscreen): Promise<void> {
    this.set("fullscreen", on);
    this.applyChrome();
    await setFullscreen(on);
    // Presenting says its own sentence, and says it about both switches at
    // once. Two notices for one gesture, the second overwriting the first, is
    // how the reader learns nothing.
    if (on && !this.settings.show_toolbar && !this.presenting) {
      ui.notice(`Full screen. Escape or ${FULLSCREEN_KEYS} comes back.`);
    }
  }

  /** The window can be taken in and out of full screen without asking us —
      the green button, or a swipe. Follow it when that happens. */
  private async syncFullscreen(): Promise<void> {
    // The keyboard is taken back here rather than the moment the switch is
    // thrown: a full-screen change is an animation, and AppKit hands focus
    // around until it has finished. This runs once the window has stopped
    // moving, whether it moved because of a shortcut, the green button, or a
    // swipe between spaces.
    void this.reclaimKeyboard();
    const actual = await isFullscreen().catch(() => this.settings.fullscreen);
    if (actual === this.settings.fullscreen) return;
    this.set("fullscreen", actual);
    this.applyChrome();
  }

  /** Going in or out of full screen hands the keyboard back to the window and
      leaves the page with nothing focused, and every shortcut in the app is a
      key press on the page — so they all stop working until something is
      clicked. Take it back, unless the reader is in the middle of typing or a
      window of ours is up and wants it. */
  private async reclaimKeyboard(): Promise<void> {
    if (ui.isWindowOpen() || ui.isMenuOpen()) return;
    const active = document.activeElement as HTMLElement | null;
    if (active && /^(INPUT|TEXTAREA)$/.test(active.tagName)) return;
    // Already typing into the page: nothing to put right.
    if (document.hasFocus()) return;
    // The window has to be ours still. Someone who walked off to another app
    // while the animation ran should not be dragged back by it.
    if (!(await isWindowFocused().catch(() => false))) return;
    // Focusing an element is not enough. The page losing the keyboard here is
    // an AppKit matter — the webview stops being the window's first responder
    // — and only the window can hand that back.
    await focusWindow().catch(() => {});
    el.viewer.focus();
  }

  /* --------------------------------------------------------------- themes */

  /**
   * Take the light or the dark theme, according to the machine.
   *
   * Nothing here decides *which* light theme or *which* dark one: those are
   * the two slots the reader has already chosen, and this only says which of
   * them is in force. So a reader with Sepia by day and Tokyo Night by night
   * gets exactly that pair, and one who has never thought about it gets the
   * defaults.
   *
   * Called at startup as well as on every change, because the system can have
   * changed its mind while the app was shut — and called before the first
   * paint, so a machine in dark mode never sees a white page on the way in.
   */
  followSystemTheme(): void {
    if (!this.settings.follow_system_theme) return;
    const wanted = darkOutside().matches;
    if (isDarkTheme(this.theme) === wanted) return;
    // Through `toggleDark` rather than `useTheme`: the theme to move to is
    // whichever fills that slot, and finding it is the one thing `toggleDark`
    // knows how to do.
    this.toggleDark(wanted);
  }

  /** Stop following, because the reader has just said otherwise. */
  private stopFollowingSystem(): void {
    if (!this.settings.follow_system_theme) return;
    this.set("follow_system_theme", false);
    ui.notice("No longer following the system's light and dark. Settings has the switch.");
  }

  useTheme(theme: Theme, remember = true): void {
    this.theme = theme;
    applyTheme(theme);
    this.viewer.setTheme(theme, !this.settings.recolor_images);
    this.sidebar.setTheme(theme);
    this.reportUnreadableColors(theme);
    if (!remember) return;
    // A theme chosen by hand whose kind is not the machine's kind is the
    // reader overruling the system — by picking a dark theme in the daytime,
    // or by pressing ⌘D, which comes through here for exactly the same
    // reason. Left following, the next thing the system did would take it
    // straight back off them. Choosing another theme of the kind already in
    // force says nothing about the system and leaves the switch alone.
    if (isDarkTheme(theme) !== darkOutside().matches) this.stopFollowingSystem();
    this.set("theme", theme.id);
    // Remember which light and which dark theme this reader prefers, so the
    // dark-mode switch returns to the right one rather than a default.
    if (isDarkTheme(theme)) this.set("dark_theme", theme.id);
    else this.set("light_theme", theme.id);
  }

  /** Themes already complained about, so a theme reapplied on every save of
      its file says it once rather than every time. Keyed by id and *what* is
      wrong, not id alone — every in-progress draft shares `id: ""`, so id
      alone would let one abandoned draft's bad colour silently swallow the
      notice for an unrelated draft started later. */
  private complainedAbout = new Map<string, string>();

  /**
   * Say when a theme file names a colour the app cannot read.
   *
   * The whole argument for keeping themes as TOML is that somebody — or
   * something asked on their behalf — can write one by hand, and what they
   * will get wrong is the notation: `steelblue`, `rgb(30, 42, 59)`, a stray
   * character in a hex string. Every one of those fell through to a fallback,
   * which for the ink is black and for the paper is white, and nothing
   * anywhere said so. The file was wrong and the screen looked like a bug in
   * the app.
   */
  private reportUnreadableColors(theme: Theme): void {
    const bad = unreadableColors(theme);
    if (bad.length === 0) {
      // Put right since it was last complained about, so it is worth
      // complaining about again if it goes wrong again.
      this.complainedAbout.delete(theme.id);
      return;
    }
    const what = bad.map((field) => `${field}=${theme[field as keyof Theme]}`).join(",");
    if (this.complainedAbout.get(theme.id) === what) return;
    this.complainedAbout.set(theme.id, what);
    const fields = bad.join(", ");
    ui.notice(
      `${theme.name}: ${fields} ${bad.length === 1 ? "is not a colour" : "are not colours"} ` +
        `HyloPDF can read. Colours are hex — #1e2a3b.`,
    );
  }

  toggleDark(on = !isDarkTheme(this.theme)): void {
    const wanted = on ? this.settings.dark_theme : this.settings.light_theme;
    const theme =
      this.themes.find((candidate) => candidate.id === wanted) ??
      this.themes.find((candidate) => isDarkTheme(candidate) === on);
    if (theme) this.useTheme(theme);
  }

  /** What to fall back on when the theme in use is deleted.
   *
   * The remembered theme of the same kind, if it is still there; otherwise any
   * theme of the same kind; otherwise whatever is left. Kind matters: someone
   * who deletes a dark theme is reading in the dark, and throwing a white page
   * at them is the one answer that is certainly wrong. */
  replacementFor(gone: Theme): Theme {
    const dark = isDarkTheme(gone);
    const remembered = dark ? this.settings.dark_theme : this.settings.light_theme;
    const left = this.themes.filter((theme) => theme.id !== gone.id);
    return (
      left.find((theme) => theme.id === remembered) ??
      left.find((theme) => isDarkTheme(theme) === dark) ??
      left[0]
    );
  }

  async refreshThemes(): Promise<void> {
    this.themes = await listThemes();
    this.theme = this.themeById(this.theme.id);
  }

  /* ----------------------------------------------------------------- zoom */

  setFit(mode: FitMode, zoom = this.settings.zoom): void {
    this.set("fit_mode", mode);
    if (mode === "actual") this.set("zoom", zoom);
    this.viewer.setFit(mode, zoom);
    this.updateZoomLabel();
  }

  /** Zoom by a proportion of where we are, which is what a pinch asks for.
      The ladder below is for buttons and keys, where a definite step is what
      is wanted.

      `focus` is where the gesture is happening, and the page stays still under
      it. Without it the zoom holds the top edge of the window instead, so
      pinching on a figure halfway down pushed the figure away from the fingers
      — which is the opposite of what every other document viewer does, and of
      what the hand expects. */
  zoomBy(factor: number, focus?: { x: number; y: number }): void {
    const min = ZOOM_LADDER[0] / 100;
    const max = ZOOM_LADDER[ZOOM_LADDER.length - 1] / 100;
    const current = this.viewer.isEmpty ? this.settings.zoom : this.viewer.zoomPercent() / 100;
    const next = Math.max(min, Math.min(max, current * factor));
    if (Math.abs(next - current) < 0.0005) return;

    // A pinch produces a new zoom every frame; the file hears about the one it
    // comes to rest at, and hears about both halves of it at once.
    this.setSoon("fit_mode", "actual");
    this.setSoon("zoom", next);
    this.viewer.setFit("actual", next, focus);
    this.updateZoomLabel();
  }

  stepZoom(direction: 1 | -1): void {
    const percent = this.viewer.isEmpty
      ? this.settings.zoom * 100
      : this.viewer.zoomPercent();
    const next =
      direction === 1
        ? ZOOM_LADDER.find((step) => step > percent + 0.5) ?? ZOOM_LADDER.at(-1)!
        : [...ZOOM_LADDER].reverse().find((step) => step < percent - 0.5) ?? ZOOM_LADDER[0];
    this.setFit("actual", next / 100);
  }

  /* ------------------------------------------------------------ page jump */

  /** Put the cursor in the page number with the number already selected, so
      that reaching page 340 is the shortcut, three digits and Enter.

      There is nowhere to put the cursor when the toolbar is away, so the
      shortcut brings it into view itself rather than making the reader do
      that first — but only for as long as the field is in use: unlike ⌘T
      this does not change the setting, and the field's own `blur` handler
      puts the toolbar back into hiding once the jump is made or abandoned. */
  focusPageNumber(): void {
    if (this.viewer.isEmpty) return;
    if (!this.settings.show_toolbar) {
      this.toolbarPeeking = true;
      this.applyChrome();
      this.viewer.relayout();
    }
    el.pageNumber.focus();
    el.pageNumber.select();
  }

  /* --------------------------------------------------------------- search */

  openFind(): void {
    if (this.viewer.isEmpty) return;
    const reopening = el.findBar.hidden;
    el.findBar.hidden = false;
    el.find.setAttribute("aria-pressed", "true");
    document.addEventListener("pointerdown", this.onFindOutside, true);
    el.findInput.focus();
    el.findInput.select();
    // Closing the bar drops the matches but keeps the words, so a bar that
    // comes back with a query still in it has to go and find it again. Without
    // this the count reads "None" beside a word the document is full of, and
    // Enter — which steps through matches that are no longer there — does
    // nothing at all until the query is edited.
    if (reopening && el.findInput.value.trim().length > 0) void this.runSearch();
  }

  /** A note somebody left in the document, made readable.
   *
   * pdf.js paints the icon and the highlight into the page, so the note has
   * always been visible; what has not been is what it says. Reading one is
   * not annotating, and this window neither writes nor offers to. */
  private showNote(note: { by: string; text: string; page: number }): void {
    ui.showWindow(note.by || "Note", () => {
      const pane = document.createElement("div");
      pane.className = "pane";
      pane.append(
        ui.text("title", note.by || "Note"),
        ui.text("lede", `On page ${this.viewer.label(note.page)}.`),
      );
      const body = document.createElement("p");
      body.className = "note-text";
      body.textContent = note.text;
      pane.append(body);
      pane.append(
        ui.text(
          "note",
          "HyloPDF shows the notes a document already carries. It does not write them.",
        ),
      );
      return pane;
    });
  }

  /* ---------------------------------------------------------------- marks */

  /** The pins in the document that is open. */
  private marks(): Mark[] {
    if (!this.path) return [];
    return this.library.find((entry) => entry.path === this.path)?.marks ?? [];
  }

  /**
   * Put a pin in this page, or take the same pin out.
   *
   * Marks are not annotations — nothing is written into the document, and
   * nothing appears on it. They are the reader's own note of where they were
   * going back to, which is the half of what people ask annotations for that
   * a reader can honestly answer, and they live in `library.toml` beside the
   * page each document was left on.
   */
  private async toggleMark(page = this.viewer.pageNumber): Promise<void> {
    if (!this.path || this.viewer.isEmpty) return;
    const at = this.viewer.position();
    const offset = at.page === page ? at.offset : 0;
    const title = this.sidebar.sectionFor(page) || `Page ${this.viewer.label(page)}`;
    try {
      const { marked, marks } = await toggleMark(this.path, page, offset, title);
      const entry = this.library.find((item) => item.path === this.path);
      if (entry) entry.marks = marks;
      this.showMarks();
      ui.notice(
        marked
          ? `Marked page ${this.viewer.label(page)}. The Contents panel lists your marks.`
          : `Took the mark off page ${this.viewer.label(page)}.`,
        marked ? "done" : undefined,
      );
    } catch (error) {
      ui.notice(messageOf(error));
    }
  }

  /** Hand the marks to the panel that lists them. */
  private showMarks(): void {
    this.sidebar.showMarks(
      this.marks(),
      (mark) => this.viewer.jumpTo(mark.page, mark.offset),
      (mark) => void this.toggleMark(mark.page),
    );
  }

  /** Select the page being read, and say what was selected — the reader asked
      for everything, and this is not everything. */
  private selectThisPage(): void {
    const page = this.viewer.pageNumber;
    if (!this.viewer.selectPage(page)) {
      ui.notice("There is no text on this page to select.");
      return;
    }
    ui.notice(
      `Selected page ${this.viewer.label(page)}. HyloPDF holds one page at a time, so that is as far as a selection goes.`,
    );
  }

  /**
   * The selected words, with where they came from.
   *
   * Copying a sentence out of a paper and then going back to find the page it
   * was on is the small, constant tax of reading for work, and it is the one
   * thing "no annotations" does not have to mean. The page comes from the
   * selection rather than from the toolbar, because a selection that runs
   * across a page boundary began on the page it began on.
   */
  private async copyQuote(): Promise<void> {
    const selection = window.getSelection();
    const quoted = selection?.toString().trim() ?? "";
    if (!quoted) {
      ui.notice("Select something first, and this copies it with its page number.");
      return;
    }
    const node = selection?.anchorNode ?? null;
    const element = node instanceof Element ? node : node?.parentElement ?? null;
    const page = Number(element?.closest<HTMLElement>("#pages .page")?.dataset.page ?? 0);
    if (!page) {
      await this.copyToClipboard(quoted, "Copied.");
      return;
    }
    const name = el.title.textContent || "";
    const where = `${name ? `${name}, ` : ""}p. ${this.viewer.label(page)}`;
    await this.copyToClipboard(`“${quoted}” — ${where}`, `Copied, with ${where}.`);
  }

  /**
   * The document and nothing else.
   *
   * Full screen and a hidden toolbar are two switches that this app has had
   * all along, and nobody assembles two switches to give a talk. This is the
   * one item that does both, and the Escape that undoes both.
   *
   * It deliberately leaves the page progression alone. Continuous scrolling
   * is a strong default in this app and a mode nothing may change by
   * accident — a reader who wants one page at a time while presenting has
   * already said so, and one who has not did not mean to.
   */
  togglePresentation(on = !this.presenting): void {
    if (on === this.presenting) return;
    if (on) {
      this.presenting = true;
      this.toolbarBeforePresenting = this.settings.show_toolbar;
      void this.toggleFullscreen(true);
      this.toggleToolbar(false);
      ui.notice(
        `Presenting. Escape gives the app back, or ${isMac ? "⌘⇧P" : "Ctrl+Shift+P"}.`,
      );
      return;
    }
    this.presenting = false;
    void this.toggleFullscreen(false);
    if (this.toolbarBeforePresenting) this.toggleToolbar(true);
  }

  /** One page across, or two. */
  setSpread(spread: SpreadMode): void {
    this.set("spread_mode", spread);
    this.viewer.setSpread(spread);
  }

  /** Take the margins off the page, or put them back. */
  setTrimMargins(on: boolean): void {
    this.set("trim_margins", on);
    this.viewer.setTrimMargins(on);
  }

  /** Turn the document a quarter, and turn the thumbnails with it. */
  rotate(quarterTurns: number): void {
    if (this.viewer.isEmpty) return;
    this.viewer.rotate(quarterTurns);
    this.sidebar.rotated();
  }

  /**
   * Print, by handing the document to something that prints.
   *
   * HyloPDF does not print, and the honest thing is to say so rather than to
   * leave ⌘P inert — `print_document` in lib.rs has the reasoning, which
   * comes down to a print dialog this app does not have and the fact that
   * every way of skipping one ends with four hundred pages coming out of a
   * printer nobody chose.
   */
  async print(): Promise<void> {
    if (!this.path) return;
    const where = systemViewerName;
    try {
      await printDocument(this.path);
      ui.notice(`HyloPDF does not print. Opened in ${where} — print it from there.`);
    } catch (error) {
      ui.notice(messageOf(error));
    }
  }

  /* ------------------------------------------------------------- history */

  /** Back to where the last jump started.
   *
   * Silence would be the wrong answer at the end of the history: the reader
   * pressed something and nothing moved, and there is no way to tell that from
   * a shortcut that is not bound. So the end of the road says so once. */
  goBack(): void {
    if (this.viewer.isEmpty) return;
    if (!this.viewer.goBack()) ui.notice("Nowhere further back.");
  }

  goForward(): void {
    if (this.viewer.isEmpty) return;
    if (!this.viewer.goForward()) ui.notice("Nowhere further forward.");
  }

  /** The Search button is a switch, not a door: pressing it again puts the bar
      away, the same as Escape or the × does. */
  toggleFind(): void {
    if (el.findBar.hidden) this.openFind();
    else this.closeFind();
  }

  closeFind(): void {
    if (el.findBar.hidden) return;
    window.clearTimeout(this.searchTimer);
    el.findBar.hidden = true;
    el.find.setAttribute("aria-pressed", "false");
    document.removeEventListener("pointerdown", this.onFindOutside, true);
    // The index goes with the bar. It is what makes stepping through matches
    // instant and it costs a long book tens of megabytes for as long as it is
    // open, which is a fair trade only while the bar is actually up.
    this.search.forget();
    el.viewer.focus();
  }

  /** Everything the find bar is allowed to lose the pointer to and stay open.
      The bar itself, obviously; the top strip, because the buttons up there
      that close it do so themselves and the ones that do not are the reader
      changing the view around a search they are still in the middle of; the
      two layers that only ever open from up there anyway; and the list of
      results, which is this search rather than somewhere else. */
  private static readonly FIND_KEEPS_OPEN =
    "#find-bar, #toolbar, #title-drag, #toolbar-peek, #popovers, #windows," +
    // The list of results is the same search seen larger, so reaching into it
    // is not reaching past the bar — picking a result there would otherwise
    // close the thing that found it.
    " #results-panel, #tab-results";

  /** Reaching past the bar puts it away, the way the Theme and Settings menus
      do. Anything below the toolbar is somewhere else — the document, the
      contents, a link — and going there is done with the search, whether or
      not it was said out loud. */
  private onFindOutside = (event: PointerEvent): void => {
    const node = event.target as Node | null;
    const element = node instanceof Element ? node : node?.parentElement ?? null;
    if (element?.closest(App.FIND_KEEPS_OPEN)) return;
    this.closeFind();
  };

  /** Push the three switches out to the parts that answer to them, and show
      them as they stand. Called at startup and after any of them is thrown. */
  private applySearchOptions(): void {
    this.search.setOptions({
      matchCase: this.settings.search_match_case,
      wholeWords: this.settings.search_whole_words,
    });
    this.viewer.setHighlightAll(this.settings.search_highlight_all);
    el.findHighlight.setAttribute("aria-pressed", String(this.settings.search_highlight_all));
    el.findCase.setAttribute("aria-pressed", String(this.settings.search_match_case));
    el.findWords.setAttribute("aria-pressed", String(this.settings.search_whole_words));
  }

  /** How many results to list. Long enough to be a list of everything for a
      real query, short enough that it is a list rather than a second copy of
      the document. */
  private static readonly RESULTS_SHOWN = 200;

  private onSearchUpdate(state: SearchState): void {
    this.sidebar.showResults(
      this.search.results(App.RESULTS_SHOWN),
      state.total,
      state.index,
      (at) => this.search.goTo(at),
    );
    if (state.total === 0) {
      // "None" is the answer to "is this word in the document". It is the
      // wrong answer to "is there anything in this document to search", which
      // is what a scan that was never put through OCR is really being asked —
      // and three things go quiet at once on such a document: search finds
      // nothing, selection selects nothing, and the contents are empty. Two
      // of those look like the app is broken.
      el.findStatus.textContent = state.scanning
        ? "…"
        : state.textless
          ? "No text"
          : el.findInput.value
            ? "None"
            : "";
      if (state.textless && !this.saidTextless) {
        this.saidTextless = true;
        ui.notice(
          "There is no text in this document — it is a scan. Nothing can be searched or selected until it has been through OCR.",
        );
      }
    } else {
      el.findStatus.textContent = `${state.index + 1} of ${state.total}${
        state.capped ? "+" : ""
      }${state.scanning ? "…" : ""}`;
    }
  }

  /* ---------------------------------------------------------------- lists */

  renderRecents(): void {
    el.recents.replaceChildren();
    const files = this.library.slice(0, 6);
    if (files.length === 0) return;

    const title = document.createElement("div");
    title.className = "recents-title";
    title.textContent = "Recently read";
    el.recents.append(title);

    for (const entry of files) {
      const button = document.createElement("button");
      button.className = "recent";
      button.innerHTML = iconMarkup("document");

      const name = document.createElement("span");
      name.className = "recent-name";
      name.textContent = entry.title || entry.path.split(/[\\/]/).pop() || entry.path;
      name.title = entry.path;

      // Where you stopped, in its own quiet column. It is on every line, not
      // only the ones past page one, so the list has a straight edge rather
      // than a label that appears and disappears down the side of it.
      const page = document.createElement("span");
      page.className = "recent-page";
      page.textContent = `p. ${entry.page}`;
      page.title = `You stopped on page ${entry.page}`;

      const forget = document.createElement("span");
      forget.className = "recent-forget";
      forget.innerHTML = iconMarkup("close");
      forget.title = "Remove from this list";
      forget.addEventListener("click", async (event) => {
        event.stopPropagation();
        this.library = this.library.filter((item) => item.path !== entry.path);
        this.renderRecents();
        await forgetDocument(entry.path).catch(() => []);
      });

      button.append(name, page, forget);
      button.addEventListener("click", () => void this.open(entry.path));
      button.addEventListener("contextmenu", (event) => {
        event.preventDefault();
        this.showDocumentMenu(button, entry.path, entry.title || entry.path);
      });
      el.recents.append(button);
    }
  }

  /* ----------------------------------------------------------------- menus */

  /** Themes are a thing you try on rather than decide once, so nothing in here
      that only changes an appearance puts the menu away: it redraws in place,
      the tick moves, and the next theme is one click rather than four. Only
      the items that take you somewhere else close it. */
  showThemeMenu(): void {
    ui.showPopover(
      el.theme,
      (close) => {
        const menu = document.createElement("div");

        const render = () => {
          // The popover is the scrolling box; redrawing inside it would
          // otherwise snap a long theme list back to the top under the hand
          // that just picked something halfway down.
          const scroller = menu.parentElement;
          const keep = scroller?.scrollTop ?? 0;

          menu.replaceChildren();
          menu.append(
            ui.row(
              "Dark mode",
              ui.toggle(isDarkTheme(this.theme), (on) => {
                this.toggleDark(on);
                render();
              }),
              isMac ? "⌘D" : "Ctrl+D",
            ),
            ui.row(
              "Follow the system",
              ui.toggle(this.settings.follow_system_theme, (on) => {
                this.set("follow_system_theme", on);
                if (on) this.followSystemTheme();
                render();
              }),
              "Light by day, dark by night.",
            ),
            ui.divider(),
            ui.section("Themes"),
          );

          for (const theme of this.themes) {
            menu.append(
              ui.menuItem({
                label: theme.name,
                checked: theme.id === this.theme.id,
                lead: ui.swatch(theme.text, theme.recolor ? theme.background : "#ffffff"),
                note: theme.built_in ? "" : "Yours",
                onSelect: () => {
                  this.useTheme(theme);
                  render();
                },
              }),
            );
          }

          menu.append(ui.divider());
          menu.append(
            ui.menuItem({
              label: "New theme…",
              icon: "plusCircle",
              onSelect: () => {
                close();
                showSettingsWindow(this, { page: "appearance", edit: { from: null } });
              },
            }),
          );
          menu.append(
            ui.menuItem({
              label: this.theme.built_in ? "Make a copy of this theme…" : "Edit this theme…",
              icon: "edit",
              onSelect: () => {
                close();
                showSettingsWindow(this, { page: "appearance", edit: { from: this.theme } });
              },
            }),
          );
          if (!this.theme.built_in) {
            menu.append(
              ui.menuItem({
                label: "Delete this theme",
                icon: "trash",
                onSelect: async () => {
                  close();
                  const removed = this.theme;
                  if (!(await ui.confirmDeleteTheme(removed.name))) return;
                  try {
                    this.themes = await deleteTheme(removed.id);
                    this.useTheme(this.replacementFor(removed));
                    ui.notice(`Deleted ${removed.name}.`);
                  } catch (error) {
                    ui.notice(messageOf(error));
                  }
                },
              }),
            );
          }

          menu.append(ui.divider());
          menu.append(
            ui.menuItem({
              label: "All appearance settings…",
              icon: "settings",
              onSelect: () => {
                close();
                showSettingsWindow(this, { page: "appearance" });
              },
            }),
          );
          hydrateIcons(menu);
          if (scroller) scroller.scrollTop = keep;
        };

        render();
        return menu;
      },
      "right",
    );
  }

  /** What can be done with a document: offered wherever one is named, which is
   *  the title in the toolbar and the recently-read list on the start screen.
   *  Opening something else — the picker, a new window, a paper read before —
   *  is the Open button's menu, not this one: this is about the document
   *  named, not about what else there is to read. `current` is the one thing
   *  that tells the two callers apart, because it flips both of the items
   *  that only make sense for one of them — opening a document a second time
   *  beside itself, and asking a document what it says about itself when it
   *  is not the one on screen. */
  showDocumentMenu(anchor: HTMLElement, path: string, name: string, current = false): void {
    ui.showPopover(anchor, (close) => {
      const menu = document.createElement("div");
      menu.append(
        ui.menuItem({
          label: `Show in ${fileManagerName}`,
          icon: "folder",
          onSelect: () => {
            close();
            void revealDocument(path).catch((error) => ui.notice(messageOf(error)));
          },
        }),
        ui.menuItem({
          label: "Mark this page",
          icon: "mark",
          note: isMac ? "⌘⇧B" : "Ctrl+Shift+B",
          checked: this.marks().some((mark) => mark.page === this.viewer.pageNumber),
          onSelect: () => {
            close();
            void this.toggleMark();
          },
        }),
        ui.menuItem({
          label: "Print…",
          icon: "print",
          note: isMac ? "⌘P" : "Ctrl+P",
          onSelect: () => {
            close();
            void this.print();
          },
        }),
        ui.menuItem({
          label: "Copy name",
          icon: "copy",
          onSelect: () => {
            close();
            void this.copyToClipboard(name, "Name copied.");
          },
        }),
        ui.menuItem({
          label: "Copy path",
          icon: "copy",
          onSelect: () => {
            close();
            void this.copyToClipboard(path, "Path copied.");
          },
        }),
      );

      // A window of its own, and only where the menu is about a document that
      // is not the one already on screen — offering to open what you are
      // reading a second time is what the Open button's menu is for.
      if (!current) {
        menu.append(
          ui.menuItem({
            label: "Open in a new window",
            icon: "window",
            onSelect: () => {
              close();
              void this.newWindow(path);
            },
          }),
        );
      }

      if (current) {
        menu.append(
          ui.divider(),
          ui.menuItem({
            label: "What this document says about itself…",
            icon: "info",
            onSelect: () => {
              close();
              void this.showDocumentDetails(name);
            },
          }),
        );
      }
      return menu;
    });
  }

  /** Every way to bring a document onto the screen, kept apart from what can
   *  be done with the one already there — that split is the whole reason this
   *  is not one item longer in `showDocumentMenu`. The picker, a second
   *  window, and the papers read before all belong to "open something";
   *  marking a page or printing belong to the document itself. */
  showOpenMenu(): void {
    ui.showPopover(el.open, (close) => {
      const menu = document.createElement("div");
      menu.append(
        ui.menuItem({
          label: "Open a document…",
          icon: "folder",
          note: isMac ? "⌘O" : "Ctrl+O",
          onSelect: () => {
            close();
            void this.openDialog();
          },
        }),
        // The two-documents-at-once route, one step: pick the second one and
        // it arrives beside the first rather than on top of it.
        ui.menuItem({
          label: "Open a document in a new window…",
          icon: "window",
          onSelect: () => {
            close();
            void this.openInNewWindow();
          },
        }),
        // And the empty one, for a reader who would rather start the second
        // window from its own recents than from the picker.
        ui.menuItem({
          label: "New window",
          icon: "window",
          note: isMac ? "⌘N" : "Ctrl+N",
          onSelect: () => {
            close();
            void this.newWindow();
          },
        }),
      );

      // The one already open, if there is one, is not offered again here —
      // reopening it in this same window is a no-op, and opening it in a
      // second one is what its own title menu is for.
      const recents = this.library.filter((entry) => entry.path !== this.path).slice(0, 8);
      if (recents.length > 0) {
        menu.append(ui.divider(), ui.section("Recently read"));
        for (const entry of recents) {
          const title = entry.title || entry.path.split(/[\\/]/).pop() || entry.path;
          const openInWindow = document.createElement("span");
          openInWindow.className = "popover-item-action";
          openInWindow.innerHTML = iconMarkup("window");
          openInWindow.title = "Open in a new window";
          openInWindow.addEventListener("click", (event) => {
            event.stopPropagation();
            close();
            void this.newWindow(entry.path);
          });
          menu.append(
            ui.menuItem({
              label: title,
              icon: "document",
              trail: openInWindow,
              onSelect: () => {
                close();
                void this.open(entry.path);
              },
            }),
          );
        }
      }
      return menu;
    });
  }

  /** Title, author, how many pages, how big a page is, what made it — the
   *  answer to "get info", which every reader has and this one did not. Only
   *  the fields the document actually fills in: a window of eleven rows, nine
   *  of them empty, tells the reader nothing except that the app has a list.
   */
  private async showDocumentDetails(name: string): Promise<void> {
    if (this.viewer.isEmpty) return;
    const { info, pages, size } = await this.viewer.details();
    const text = (key: string): string => {
      const value = info[key];
      return typeof value === "string" ? value.trim() : "";
    };

    ui.showWindow("Document", () => {
      const pane = document.createElement("div");
      pane.className = "pane";
      pane.append(ui.text("title", text("Title") || name));

      const rows: [string, string][] = [
        ["Author", text("Author")],
        ["Subject", text("Subject")],
        ["Keywords", text("Keywords")],
        ["Pages", String(pages)],
        ["Page size", size],
        ["Made with", text("Creator")],
        ["Written by", text("Producer")],
        ["PDF version", text("PDFFormatVersion")],
        ["Created", readableDate(text("CreationDate"))],
        ["Changed", readableDate(text("ModDate"))],
      ];
      for (const [label, value] of rows) {
        if (value) pane.append(ui.field(label, selectable(value)));
      }
      if (this.path) pane.append(ui.field("File", selectable(this.path)));
      return pane;
    });
  }

  private async copyToClipboard(text: string, said: string): Promise<void> {
    try {
      await copyText(text);
      ui.notice(said, "done");
    } catch (error) {
      ui.notice(messageOf(error));
    }
  }

  /** Zoom and rotation are things you try on, the same as a theme — nothing
      in here takes you anywhere else, so nothing in here puts the menu away;
      it redraws in place and the ticks move. */
  showZoomMenu(): void {
    ui.showPopover(
      el.zoomLevel,
      () => {
        const menu = document.createElement("div");

        const render = () => {
          menu.replaceChildren();

          const modes: [FitMode, string, string, string][] = [
            ["width", "Fit width", "fitWidth", isMac ? "⌘0" : "Ctrl+0"],
            ["page", "Fit page", "fitPage", isMac ? "⌘2" : "Ctrl+2"],
          ];
          for (const [mode, label, icon, keys] of modes) {
            menu.append(
              ui.menuItem({
                label,
                icon,
                note: keys,
                checked: this.settings.fit_mode === mode,
                onSelect: () => {
                  this.setFit(mode);
                  render();
                },
              }),
            );
          }
          // The page at the size it was made, which is what somebody checking
          // a figure against print is asking for. It was in the list of
          // presets below as "100%" and nowhere else, which is not where
          // anybody looks for it.
          menu.append(
            ui.menuItem({
              label: "Actual size",
              icon: "actualSize",
              note: isMac ? "⌘1" : "Ctrl+1",
              checked:
                this.settings.fit_mode === "actual" &&
                Math.round(this.settings.zoom * 100) === 100,
              onSelect: () => {
                this.setFit("actual", 1);
                render();
              },
            }),
          );
          menu.append(
            ui.divider(),
            ui.menuItem({
              label: "Rotate right",
              icon: "rotateRight",
              note: isMac ? "⌘R" : "Ctrl+R",
              onSelect: () => {
                this.rotate(1);
                render();
              },
            }),
            ui.menuItem({
              label: "Rotate left",
              icon: "rotateLeft",
              note: isMac ? "⌘L" : "Ctrl+L",
              onSelect: () => {
                this.rotate(-1);
                render();
              },
            }),
          );

          menu.append(ui.divider());
          // The presets below are the common answers; this is the rest of
          // them. It starts from what is actually on screen rather than from
          // the remembered zoom, because in a fit mode those are different
          // numbers and the one being looked at is the one to start typing
          // over.
          menu.append(
            ui.row(
              "Zoom to",
              ui.stepper(
                Math.round(
                  this.viewer.isEmpty ? this.settings.zoom * 100 : this.viewer.zoomPercent(),
                ),
                { min: ZOOM_LADDER[0], max: ZOOM_LADDER[ZOOM_LADDER.length - 1], step: 25 },
                (value) => {
                  this.setFit("actual", value / 100);
                  render();
                },
                "%",
              ),
            ),
          );
          for (const percent of [50, 75, 100, 125, 150, 200, 300]) {
            menu.append(
              ui.menuItem({
                label: `${percent}%`,
                checked:
                  this.settings.fit_mode === "actual" &&
                  Math.round(this.settings.zoom * 100) === percent,
                onSelect: () => {
                  this.setFit("actual", percent / 100);
                  render();
                },
              }),
            );
          }
          hydrateIcons(menu);
        };

        render();
        return menu;
      },
      "right",
    );
  }

  showSettingsMenu(anchor: HTMLElement): void {
    ui.showPopover(
      anchor,
      (close) => {
        const menu = document.createElement("div");

        // The toolbar switch comes first. It is the one setting whose absence
        // hides every other way of reaching this menu, so it should never be
        // something you have to go looking for.
        menu.append(ui.section("Window"));
        menu.append(
          ui.row(
            "Show toolbar",
            // And then leave: this menu hangs off a button in the toolbar, so
            // turning the toolbar off leaves it anchored to nothing, floating
            // over the document with no way back to what opened it.
            ui.toggle(this.settings.show_toolbar, (on) => {
              this.toggleToolbar(on);
              if (!on) close();
            }),
            isMac ? "⌘T" : "Ctrl+T",
          ),
          ui.row(
            "Full screen",
            ui.toggle(this.settings.fullscreen, (on) => void this.toggleFullscreen(on)),
            FULLSCREEN_KEYS,
          ),
        );

        menu.append(ui.divider(), ui.section("Reading"));
        menu.append(
          ui.menuItem({
            label: "Continuous scrolling",
            checked: this.settings.scroll_mode === "continuous",
            note: "Default",
            onSelect: () => {
              this.setScrollMode("continuous");
              close();
            },
          }),
          ui.menuItem({
            label: "One page at a time",
            checked: this.settings.scroll_mode === "paged",
            onSelect: () => {
              this.setScrollMode("paged");
              close();
            },
          }),
        );

        menu.append(ui.divider(), ui.section("Pages side by side"));
        const spreads: [SpreadMode, string][] = [
          ["single", "One page across"],
          ["two", "Two side by side"],
          ["cover", "Two, cover alone"],
        ];
        for (const [value, label] of spreads) {
          menu.append(
            ui.menuItem({
              label,
              checked: this.settings.spread_mode === value,
              onSelect: () => {
                this.setSpread(value);
                close();
              },
            }),
          );
        }

        menu.append(ui.divider());
        menu.append(
          ui.row(
            "Space between pages",
            ui.stepper(
              this.settings.page_gap,
              { min: 0, max: 64, step: 4 },
              (value) => this.setPageGap(value),
              "px",
            ),
          ),
          ui.row(
            "Trim the margins",
            ui.toggle(this.settings.trim_margins, (on) => this.setTrimMargins(on)),
            "Fit the words rather than the paper.",
          ),
          ui.row(
            "Recolour pictures too",
            ui.toggle(this.settings.recolor_images, (on) => this.setRecolorImages(on)),
            "Off leaves them as printed.",
          ),
          ui.row(
            "Come back to where I stopped",
            ui.toggle(this.settings.remember_position, (on) =>
              this.set("remember_position", on),
            ),
          ),
          ui.row(
            "Show page count while scrolling",
            ui.toggle(this.settings.show_page_pill, (on) => this.set("show_page_pill", on)),
            "Only when the toolbar is hidden.",
          ),
        );

        menu.append(ui.divider());
        menu.append(
          ui.menuItem({
            label: "All settings…",
            icon: "settings",
            note: isMac ? "⌘," : "Ctrl+,",
            onSelect: () => {
              close();
              // The front door, and it opens on the same page every time. The
              // way into Appearance is the Theme menu next door; this one
              // carries on where the switches above it stop, which is Reading.
              showSettingsWindow(this, { page: "reading" });
            },
          }),
        );
        return menu;
      },
      "right",
    );
  }

  /* ----------------------------------------------------------------- wiring */

  private wire(): void {
    // Anything in the bar that opens something of its own closes search on the
    // way: two panels claiming the same corner of the screen, one of them
    // still holding the keyboard, is not a place anyone meant to be. The
    // buttons that merely move around the document leave it alone.
    const opens = (run: () => void) => () => {
      this.closeFind();
      run();
    };

    el.open.addEventListener("click", opens(() => this.showOpenMenu()));
    el.welcomeOpen.addEventListener("click", () => void this.openDialog());
    // The close handler runs on the way out, so this saves what a quit saves.
    el.newWindow.addEventListener("click", opens(() => void this.newWindow()));
    el.quit.addEventListener("click", () => void closeWindow());
    el.contents.addEventListener("click", opens(() => this.toggleSidebar()));
    el.closeDoc.addEventListener("click", opens(() => this.closeDocument()));
    el.theme.addEventListener("click", opens(() => this.showThemeMenu()));
    el.zoomLevel.addEventListener("click", opens(() => this.showZoomMenu()));
    el.zoomIn.addEventListener("click", () => this.stepZoom(1));
    el.zoomOut.addEventListener("click", () => this.stepZoom(-1));
    el.prevPage.addEventListener("click", () => this.viewer.previousPage());
    el.nextPage.addEventListener("click", () => this.viewer.nextPage());
    el.find.addEventListener("click", () => this.toggleFind());

    el.settings.addEventListener("click", opens(() => this.showSettingsMenu(el.settings)));

    const titleMenu = (event: Event) => {
      if (!this.path) return;
      event.preventDefault();
      this.closeFind();
      this.showDocumentMenu(el.title, this.path, el.title.textContent || this.path, true);
    };
    el.title.addEventListener("click", titleMenu);
    el.title.addEventListener("contextmenu", titleMenu);

    // The webview's own menu is for web pages: it offers to reload the app and
    // to open the inspector, neither of which belongs to a reader. Text that
    // has been selected keeps its menu, because copying is worth having.
    document.addEventListener("contextmenu", (event) => {
      const target = event.target as HTMLElement | null;
      const editable = target?.closest("input, textarea, [contenteditable='true']");
      const selected = !(window.getSelection()?.isCollapsed ?? true);
      if (!editable && !selected) event.preventDefault();
    });

    // The two side buttons on a mouse. They are `auxclick`, like the middle
    // one, and they are what a hand reaches for before it reaches for ⌘[.
    el.viewer.addEventListener("auxclick", (event) => {
      if (event.button === 3) {
        event.preventDefault();
        this.goBack();
      } else if (event.button === 4) {
        event.preventDefault();
        this.goForward();
      }
    });

    el.pageNumber.title = `Go to a page — ${JUMP_KEYS}, or g`;
    el.pageNumber.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        // What was typed is read as a page label first and a position in the
        // file second, which is the order that makes "go to page 314" find
        // what the index meant by it. A number that names neither is left on
        // screen rather than silently thrown away.
        const page = this.viewer.pageForLabel(el.pageNumber.value);
        if (page !== null) this.viewer.goToPage(page);
        else el.pageNumber.value = this.viewer.label(this.viewer.pageNumber);
        el.viewer.focus();
      } else if (event.key === "Escape") {
        // As in the find field: swallow it, or AppKit takes the window out of
        // full screen for us.
        event.preventDefault();
        el.pageNumber.value = this.viewer.label(this.viewer.pageNumber);
        el.viewer.focus();
      }
      event.stopPropagation();
    });
    el.pageNumber.addEventListener("blur", () => {
      el.pageNumber.value = this.viewer.label(this.viewer.pageNumber);
      if (this.toolbarPeeking) {
        this.toolbarPeeking = false;
        this.applyChrome();
        this.viewer.relayout();
      }
    });

    // One search per pause, not one per keystroke: a scan of a long document
    // that is thrown away by the next letter is work nobody asked for.
    el.findInput.addEventListener("input", () => {
      this.searchPending = true;
      window.clearTimeout(this.searchTimer);
      this.searchTimer = window.setTimeout(() => void this.runSearch(), 180);
    });
    el.findInput.addEventListener("keydown", (event) => {
      // Enter on something just typed means "search this now", not "step
      // through what the last query found".
      if (event.key === "Enter" && this.searchPending) void this.runSearch();
      else if (event.key === "Enter") this.search.step(event.shiftKey ? -1 : 1);
      else if (event.key === "Escape") {
        // Left to itself, an unhandled Escape walks out of the webview and
        // into AppKit, which reads it as "leave full screen". Search closing
        // is the whole answer to this key press.
        event.preventDefault();
        this.closeFind();
      }
      event.stopPropagation();
    });
    el.findNext.addEventListener("click", () => this.search.step(1));
    el.findPrev.addEventListener("click", () => this.search.step(-1));
    el.findClose.addEventListener("click", () => this.closeFind());
    // The count is the way to the list behind it. "3 of 128" answers "is it in
    // here" and not "which one did I mean", and the second question is the one
    // somebody searching a long document is usually asking.
    el.findStatus.addEventListener("click", () => {
      if (this.search.total === 0) return;
      if (!this.settings.show_sidebar) this.toggleSidebar(true);
      this.sidebar.showResultsTab();
    });

    // Two of the three change what counts as a match, so the query has to be
    // asked again; "Highlight all" only changes what is painted, and the
    // viewer has already repainted by the time this returns. Either way the
    // field keeps the keyboard: a switch is thrown in the middle of typing.
    const option = (
      button: HTMLButtonElement,
      key: "search_highlight_all" | "search_match_case" | "search_whole_words",
      rescan: boolean,
    ) =>
      button.addEventListener("click", () => {
        this.set(key, !this.settings[key]);
        this.applySearchOptions();
        if (rescan) void this.runSearch();
        el.findInput.focus();
      });
    option(el.findHighlight, "search_highlight_all", false);
    option(el.findCase, "search_match_case", true);
    option(el.findWords, "search_whole_words", true);

    this.wireSidebarResize();
    this.wireToolbarPeek();
    this.wireKeyboard();
    this.wireWindow();
  }

  /** A link in a document that points at the web. */
  private async openLink(url: string): Promise<void> {
    try {
      await openExternal(url);
      ui.notice(`Opened ${hostOf(url)} in your browser.`);
    } catch (error) {
      ui.notice(messageOf(error));
    }
  }

  private async runSearch(): Promise<void> {
    window.clearTimeout(this.searchTimer);
    this.searchPending = false;
    const doc = this.viewer.document;
    if (!doc) return;
    await this.search.find(el.findInput.value, doc);
  }

  private wireSidebarResize(): void {
    let startX = 0;
    let startWidth = 0;
    const move = (event: PointerEvent) => {
      const width = Math.max(160, Math.min(460, startWidth + event.clientX - startX));
      el.sidebar.style.width = `${width}px`;
      this.viewer.relayout();
    };
    const up = () => {
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", up);
      this.set("sidebar_width", el.sidebar.offsetWidth);
      // The thumbnails are drawn for a panel width, so they follow it — once
      // the edge has come to rest, rather than on every pixel of the drag.
      this.sidebar.resize();
    };
    el.sidebarGrip.addEventListener("pointerdown", (event) => {
      startX = event.clientX;
      startWidth = el.sidebar.offsetWidth;
      document.addEventListener("pointermove", move);
      document.addEventListener("pointerup", up);
      event.preventDefault();
    });
  }

  /** Everything the app listens for, as a table of named actions.
   *
   * The keys themselves are in `keys.ts`, with whatever the reader's
   * `keys.toml` has to say over the top; this is only what each name does. A
   * handler takes no arguments on purpose — an action that needed to know
   * which key summoned it would be two actions, and the two would be
   * separately bindable. That is why "next match" and "previous match" are
   * two entries here where they were one branch and a `shiftKey` test
   * before. */
  private handlers?: Record<Action, () => void>;

  private actions(): Record<Action, () => void> {
    // Built once. Nothing in here reads the event, so there is nothing to
    // rebuild per keystroke.
    if (this.handlers) return this.handlers;
    return (this.handlers = {
      open: () => void this.openDialog(),
      // ⌘P did nothing at all, which reads as a broken app rather than as a
      // missing feature — and printing is not a power tool, it is what people
      // do with a boarding pass. `print` says what this app can and cannot do
      // and hands the document to something that can.
      print: () => void this.print(),
      settings: () => showSettingsWindow(this),
      // The list of everything the app listens for was three clicks behind a
      // cog, which is a strange place to keep the answer to "what can this
      // thing do". F1 is where every application puts it, and ⌘/ is where the
      // ones without an F1 key put it.
      help: () => showSettingsWindow(this, { page: "keyboard" }),
      "new-window": () => void this.newWindow(),
      "close-window": () => void closeWindow(),
      quit: () => void quitApp(),
      find: () => this.openFind(),
      "find-next": () => this.search.step(1),
      "find-previous": () => this.search.step(-1),
      // ⌘A had nothing good to select: only the pages near the window are in
      // the DOM, so "everything" was a page and a half of document plus the
      // contents panel and whatever else was on screen. A page is a unit
      // somebody means, and it is the largest one this app can honestly
      // offer.
      "select-page": () => this.selectThisPage(),
      // The one thing people do with a selection in a document they are
      // reading for work: quote it, and say where it came from.
      "copy-quote": () => void this.copyQuote(),
      mark: () => void this.toggleMark(),
      dismiss: () => {
        // One thing per press, closest layer first.
        if (!el.findBar.hidden) this.closeFind();
        else if (this.presenting) this.togglePresentation(false);
        else if (this.settings.fullscreen) void this.toggleFullscreen(false);
      },

      "next-page": () => this.viewer.nextPage(),
      "previous-page": () => this.viewer.previousPage(),
      "scroll-down": () => this.viewer.scrollByStep(1),
      "scroll-up": () => this.viewer.scrollByStep(-1),
      "half-screen-down": () => this.viewer.scrollByViewport(1, 0.5),
      "half-screen-up": () => this.viewer.scrollByViewport(-1, 0.5),
      "screen-down": () => this.viewer.scrollByViewport(1),
      "screen-up": () => this.viewer.scrollByViewport(-1),
      "first-page": () => this.viewer.goToPage(1),
      "last-page": () => this.viewer.goToPage(this.viewer.pageCount),
      "go-to-page": () => this.focusPageNumber(),
      back: () => this.goBack(),
      forward: () => this.goForward(),

      "zoom-in": () => this.stepZoom(1),
      "zoom-out": () => this.stepZoom(-1),
      "fit-width": () => this.setFit("width"),
      "actual-size": () => this.setFit("actual", 1),
      "fit-page": () => this.setFit("page"),
      "rotate-right": () => this.rotate(1),
      "rotate-left": () => this.rotate(-1),
      dark: () => this.toggleDark(),
      sidebar: () => this.toggleSidebar(),
      toolbar: () => this.toggleToolbar(),
      fullscreen: () => void this.toggleFullscreen(),
      present: () => this.togglePresentation(),
    });
  }

  /** Read `keys.toml`, work out what is bound, and say what could not be.
   *
   * Called at startup and again from the Keyboard page's Reload button. The
   * problems are shown there, in full, beside the keys they are about; the
   * notice here is for the reader who is not in Settings and would otherwise
   * find out by pressing a key that does nothing. */
  async reloadKeys(announce = true): Promise<void> {
    const { bindings, problems } = await loadKeys();
    this.keymap = buildKeymap(bindings);
    const all = [...problems, ...this.keymap.problems];
    this.keymap.problems = all;
    if (!announce || all.length === 0) return;
    ui.notice(
      all.length === 1
        ? `keys.toml: ${all[0]}`
        : `keys.toml: ${all[0]} And ${all.length - 1} more — see Settings → Keyboard.`,
    );
  }

  private wireKeyboard(): void {
    document.addEventListener("keydown", (event) => {
      const target = event.target as HTMLElement | null;
      if (target && /^(INPUT|TEXTAREA)$/.test(target.tagName)) return;
      // Space and Enter belong to whatever is focused, if that is something
      // that can be pressed: it is how a button is pressed without a mouse,
      // and reading Space as "down a screen" there means the bar cannot be
      // used from the keyboard at all.
      if (
        (event.key === " " || event.key === "Enter") &&
        target?.closest("button, a, [role='button'], [role='tab']")
      ) {
        return;
      }
      if (ui.isWindowOpen()) return;

      const chords = chordsOf(event);
      if (chords.length === 0) return;

      for (const chord of chords) {
        // Half way through a sequence: `g` has been pressed and this is what
        // came next. A chord that does not continue it is not a mistake —
        // `g` then ⌘F is a reader changing their mind — so the pending
        // prefix is dropped and the chord is tried on its own below.
        const continued = this.pendingChord ? `${this.pendingChord} ${chord}` : "";
        const binding = continued && this.keymap.byBinding.has(continued) ? continued : chord;
        const action = this.keymap.byBinding.get(binding);
        if (action) {
          // An open menu has its own capturing key handler and Escape is its
          // way out. Whatever key `dismiss` is on, it is the menu's first —
          // and an Escape the webview leaves unhandled reaches AppKit, which
          // drops the window out of full screen behind our back.
          if (action === "dismiss" && ui.isMenuOpen()) return;
          if (needsDocument(action) && this.viewer.isEmpty) break;
          this.pendingChord = "";
          event.preventDefault();
          this.actions()[action]();
          return;
        }
        const prefix = continued && this.keymap.prefixes.has(continued) ? continued : chord;
        if (this.keymap.prefixes.has(prefix)) {
          this.pendingChord = prefix;
          event.preventDefault();
          // A sequence half pressed and then abandoned must not lie in wait:
          // `g`, a minute of reading, and then `g` again is two presses, not
          // one gesture.
          window.clearTimeout(this.pendingTimer);
          this.pendingTimer = window.setTimeout(() => (this.pendingChord = ""), 1200);
          return;
        }
      }
      this.pendingChord = "";
    });

    // Pinch and ctrl+wheel are zoom everywhere else; they should be here too.
    //
    // A trackpad pinch arrives as a stream of small events and a mouse wheel as
    // one large one, so neither can be a step on the ladder: that is what sent
    // 125% to 400% in one gesture. Each event becomes a proportion instead,
    // and the proportions collected within a frame are applied together.
    let pendingZoom = 1;
    let zoomFrame = 0;
    // Where the gesture is. The last one seen within a frame wins, which is
    // the one the fingers are on now.
    let zoomAt: { x: number; y: number } | undefined;
    // Set by the pinch handlers below; read by the wheel handler above them.
    let pinching = false;
    let lastScale = 1;
    const queueZoom = (factor: number, at?: { x: number; y: number }) => {
      pendingZoom *= factor;
      if (at) zoomAt = at;
      if (zoomFrame) return;
      zoomFrame = requestAnimationFrame(() => {
        zoomFrame = 0;
        const collected = pendingZoom;
        const where = zoomAt;
        pendingZoom = 1;
        zoomAt = undefined;
        this.zoomBy(collected, where);
      });
    };

    el.viewer.addEventListener(
      "wheel",
      (event) => {
        if (!event.ctrlKey && !event.metaKey) return;
        event.preventDefault();
        // A pinch that WebKit has taken over is not also a wheel: see below.
        if (pinching) return;
        const perLine = event.deltaMode === 1 ? 16 : 1;
        const delta = Math.max(-60, Math.min(60, event.deltaY * perLine));
        queueZoom(Math.exp(-delta / 320), { x: event.clientX, y: event.clientY });
      },
      { passive: false },
    );

    /* A trackpad pinch, which is not a wheel for most of its life.
     *
     * WebKit opens a pinch with a handful of ctrl+wheel events — six of them,
     * over about sixty milliseconds — and then stops sending them and carries
     * on with its own `gesturechange` instead, all the way to the end of the
     * gesture. An app that listens only for the wheel therefore zooms for the
     * first fraction of a second and then goes deaf, however long the fingers
     * keep moving. That is the whole of "it only zooms one step".
     *
     * `scale` on these events is cumulative from the start of the gesture, so
     * the step is the ratio against the last one seen. While a gesture is in
     * flight the wheel path stands down, or the opening events would be
     * counted twice. Nothing outside WebKit fires these at all, so the wheel
     * remains the path for a mouse, and for Chromium. */
    const scaleOf = (event: Event) => (event as Event & { scale?: number }).scale ?? 1;
    /** Where a pinch is on the screen. WebKit puts the middle of the two
        fingers on the gesture event, in client coordinates. */
    const centreOf = (event: Event) => {
      const { clientX, clientY } = event as Event & { clientX?: number; clientY?: number };
      return clientX === undefined || clientY === undefined
        ? undefined
        : { x: clientX, y: clientY };
    };

    el.viewer.addEventListener(
      "gesturestart",
      (event) => {
        event.preventDefault();
        pinching = true;
        lastScale = scaleOf(event);
      },
      { passive: false },
    );
    el.viewer.addEventListener(
      "gesturechange",
      (event) => {
        event.preventDefault();
        const scale = scaleOf(event);
        if (!pinching || scale <= 0 || lastScale <= 0) return;
        queueZoom(scale / lastScale, centreOf(event));
        lastScale = scale;
      },
      { passive: false },
    );
    for (const kind of ["gestureend", "gesturecancel"]) {
      el.viewer.addEventListener(
        kind,
        (event) => {
          event.preventDefault();
          pinching = false;
        },
        { passive: false },
      );
    }
  }

  private wireWindow(): void {
    // Laying the pages out again resizes the very box being watched, which the
    // observer then reports as another change — the loop the browser warns
    // about. Doing the work on the next frame, and only when the box really
    // changed size, breaks it.
    let width = 0;
    let height = 0;
    let relayoutFrame = 0;
    const resize = new ResizeObserver((entries) => {
      const box = entries[entries.length - 1]?.contentRect;
      if (box) {
        if (box.width === width && box.height === height) return;
        width = box.width;
        height = box.height;
      }
      cancelAnimationFrame(relayoutFrame);
      relayoutFrame = requestAnimationFrame(() => {
        relayoutFrame = 0;
        this.viewer.relayout();
      });
    });
    resize.observe(el.viewer);

    void onWindowGeometryChange(() => {
      window.clearTimeout(this.geometryTimer);
      this.geometryTimer = window.setTimeout(() => void saveWindowState(), 600);
      // Every resize is a candidate, but only the one it settles on is asked
      // about: the answer costs a trip into Rust.
      window.clearTimeout(this.fullscreenTimer);
      this.fullscreenTimer = window.setTimeout(() => void this.syncFullscreen(), 150);
    });

    // The last thing that happens before the window goes. Everything here is
    // awaited: a write still in flight when the window is destroyed is a write
    // that never happened, and "come back to where I stopped" is the one
    // promise this app makes about what survives a quit.
    void onCloseRequested(async () => {
      await this.savePosition();
      await this.flushSettings();
      await saveWindowState().catch(() => {});
    });

    // In a plain browser there is no Tauri drag-and-drop, so fall back to the
    // DOM version. Harmless when the real one is in charge.
    if (!hasBackend) {
      document.addEventListener("dragover", (event) => event.preventDefault());
      document.addEventListener("drop", (event) => {
        event.preventDefault();
        const file = event.dataTransfer?.files[0];
        if (file) void this.open(registerBrowserFile(file));
      });
    }
  }
}

/** The part of a web address worth reading back to someone. */
/** A value in the document window: text the reader can select and copy, since
    half the reason to open that window is to take something out of it. */
function selectable(value: string): HTMLElement {
  const element = document.createElement("span");
  element.className = "field-note";
  element.style.userSelect = "text";
  element.style.textAlign = "right";
  element.style.maxWidth = "440px";
  element.style.wordBreak = "break-word";
  element.textContent = value;
  return element;
}

/** A PDF date — `D:20240131120000+01'00'` — as something a person reads.
    Anything that is not that shape is handed back as it came: a date this
    cannot parse is still a date somebody wrote down. */
function readableDate(value: string): string {
  const match = /^D:(\d{4})(\d{2})(\d{2})(?:(\d{2})(\d{2}))?/.exec(value);
  if (!match) return value;
  const [, year, month, day, hour, minute] = match;
  const date = new Date(
    Number(year),
    Number(month) - 1,
    Number(day),
    Number(hour ?? 0),
    Number(minute ?? 0),
  );
  if (Number.isNaN(date.getTime())) return value;
  return hour
    ? date.toLocaleString(undefined, { dateStyle: "long", timeStyle: "short" })
    : date.toLocaleDateString(undefined, { dateStyle: "long" });
}

/** Whether a document's own title is better than the name of its file.
    See `adoptDocumentTitle` for why so much of this is refusal. */
function worthCalling(title: string, fileName: string): boolean {
  if (title.length < 4 || title.length > 200) return false;
  const stem = fileName.replace(/\.pdf$/i, "").toLowerCase();
  const folded = title.toLowerCase();
  if (folded === stem || folded === fileName.toLowerCase()) return false;
  if (/^untitled\b|^document\d*$|^microsoft word\s*-/i.test(title)) return false;
  // A title that is a file name is a file name, whatever file it names.
  if (/\.(pdf|docx?|tex|indd|pptx?|odt|rtf|ps|dvi)$/i.test(title)) return false;
  return true;
}

function hostOf(url: string): string {
  try {
    return new URL(url).host || url;
  } catch {
    return url;
  }
}

function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong.";
}

const app = new App();
void app.start().catch((error) => {
  console.error("startup failed", error);
  ui.notice(messageOf(error));
  void signalReady();
});

