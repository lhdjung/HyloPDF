/* HyloPDF.
 *
 * One object holds the state — settings, themes, the open document — and every
 * change goes through it, so a setting written to disk and a setting shown in
 * the interface can never disagree. Settings are written one key at a time;
 * nothing here ever saves the whole blob. */

import {
  type Bootstrap,
  type LibraryEntry,
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
  quitApp,
  registerBrowserFile,
  rememberPosition,
  revealDocument,
  saveWindowState,
  focusWindow,
  setFullscreen,
  setSettings,
  setTitlebarButtons,
  setWindowTitle,
  signalReady,
  log,
} from "./api";

import { hydrateIcons, iconMarkup } from "./icons";
import { type SearchState, Search } from "./search";
import { isEditingTheme, refreshSettingsWindow, showSettingsWindow } from "./settings";
import { Sidebar } from "./sidebar";
import { applyTheme, isDarkTheme, unreadableColors } from "./themes";
import * as ui from "./ui";
import { Cancelled, type FitMode, Viewer } from "./viewer";

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
  title: byId<HTMLDivElement>("doc-title"),
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
  viewer: byId<HTMLDivElement>("viewer"),
  pages: byId<HTMLDivElement>("pages"),
  welcome: byId<HTMLElement>("welcome"),
  welcomeOpen: byId<HTMLButtonElement>("welcome-open"),
  quit: byId<HTMLButtonElement>("quit"),
  recents: byId<HTMLDivElement>("recents"),
  findBar: byId<HTMLDivElement>("find-bar"),
  findInput: byId<HTMLInputElement>("find-input"),
  findStatus: byId<HTMLSpanElement>("find-status"),
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

  constructor() {
    this.viewer = new Viewer(el.viewer, el.pages, {
      onPageChange: (page, count) => this.onPageChange(page, count),
      onScroll: () => this.onScroll(),
      onError: (message) => ui.notice(message),
      onExternalLink: (url) => void this.openLink(url),
      onPassword: (wrong) => ui.askForPassword(wrong),
    });
    this.sidebar = new Sidebar(
      el.outlinePanel,
      el.pagesPanel,
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

    applyTheme(this.theme);
    this.applyChrome();
    this.reportUnreadableColors(this.theme);
    this.viewer.setTheme(this.theme, !this.settings.recolor_images);
    this.viewer.setGap(this.settings.page_gap);
    this.viewer.setScrollMode(this.settings.scroll_mode);
    this.viewer.setFit(this.settings.fit_mode, this.settings.zoom);
    this.applySearchOptions();
    this.renderRecents();
    this.wire();

    // Listen before reporting in: the answer to `ready` may itself be a
    // document, and anything arriving after it comes through as an event.
    await this.listenForDocuments();
    await this.listenForFileChanges();
    const startWith = await signalReady();
    if (startWith) await this.open(startWith);

    // Starting up in full screen with the toolbar away means starting up with
    // nothing on screen to press, so say once how to get back out.
    if (this.settings.fullscreen && !this.settings.show_toolbar) {
      ui.notice(`Full screen. Escape or ${FULLSCREEN_KEYS} comes back.`);
    }
  }

  /** Documents from the OS: "Open with", a file dropped on the icon, or one
      named on the command line. */
  private async listenForDocuments(): Promise<void> {
    await onExternalDocument((path) => void this.open(path));
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
      void this.sidebar.setDocument(doc, this.theme);

      const start = this.settings.remember_position ? opened : { page: 1, offset: 0 };
      this.viewer.scrollTo(start.page, start.offset);
      this.library = [
        { path, title: opened.name, page: start.page, offset: start.offset, opened_at: 0 },
        ...this.library.filter((entry) => entry.path !== path),
      ];
      this.renderRecents();
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

  private onPageChange(page: number, count: number): void {
    if (document.activeElement !== el.pageNumber) {
      el.pageNumber.value = count > 0 ? String(page) : "";
    }
    el.pageCount.textContent = count > 0 ? `of ${count}` : "";
    el.pagePill.textContent = count > 0 ? `${page} of ${count}` : "";
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
    if (!show) {
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
    if (on && !this.settings.show_toolbar) {
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

  useTheme(theme: Theme, remember = true): void {
    this.theme = theme;
    applyTheme(theme);
    this.viewer.setTheme(theme, !this.settings.recolor_images);
    this.sidebar.setTheme(theme);
    this.reportUnreadableColors(theme);
    if (!remember) return;
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
      changing the view around a search they are still in the middle of; and
      the two layers that only ever open from up there anyway. */
  private static readonly FIND_KEEPS_OPEN =
    "#find-bar, #toolbar, #title-drag, #toolbar-peek, #popovers, #windows";

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

  private onSearchUpdate(state: SearchState): void {
    if (state.total === 0) {
      el.findStatus.textContent = state.scanning ? "…" : el.findInput.value ? "None" : "";
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

  /** What can be done with a document from outside the app. Offered wherever a
      document is named: the title in the toolbar, and the recently-read list. */
  showDocumentMenu(anchor: HTMLElement, path: string, name: string): void {
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
      return menu;
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

  showZoomMenu(): void {
    ui.showPopover(
      el.zoomLevel,
      (close) => {
        const menu = document.createElement("div");
        const modes: [FitMode, string, string][] = [
          ["width", "Fit width", "fitWidth"],
          ["page", "Fit page", "fitPage"],
        ];
        for (const [mode, label, icon] of modes) {
          menu.append(
            ui.menuItem({
              label,
              icon,
              checked: this.settings.fit_mode === mode,
              onSelect: () => {
                this.setFit(mode);
                close();
              },
            }),
          );
        }
        menu.append(ui.divider(), ui.section("Zoom"));
        // The presets below are the common answers; this is the rest of them.
        // It starts from what is actually on screen rather than from the
        // remembered zoom, because in a fit mode those are different numbers
        // and the one being looked at is the one to start typing over.
        menu.append(
          ui.row(
            "Zoom to",
            ui.stepper(
              Math.round(this.viewer.isEmpty ? this.settings.zoom * 100 : this.viewer.zoomPercent()),
              { min: ZOOM_LADDER[0], max: ZOOM_LADDER[ZOOM_LADDER.length - 1], step: 25 },
              (value) => this.setFit("actual", value / 100),
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
                close();
              },
            }),
          );
        }
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

    el.open.addEventListener("click", opens(() => void this.openDialog()));
    el.welcomeOpen.addEventListener("click", () => void this.openDialog());
    // The close handler runs on the way out, so this saves what a quit saves.
    el.quit.addEventListener("click", () => void quitApp());
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

    el.title.addEventListener("contextmenu", (event) => {
      if (!this.path) return;
      event.preventDefault();
      this.showDocumentMenu(el.title, this.path, el.title.textContent || this.path);
    });

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
        const page = Number.parseInt(el.pageNumber.value, 10);
        if (Number.isFinite(page)) this.viewer.goToPage(page);
        el.viewer.focus();
      } else if (event.key === "Escape") {
        // As in the find field: swallow it, or AppKit takes the window out of
        // full screen for us.
        event.preventDefault();
        el.pageNumber.value = String(this.viewer.pageNumber);
        el.viewer.focus();
      }
      event.stopPropagation();
    });
    el.pageNumber.addEventListener("blur", () => {
      el.pageNumber.value = String(this.viewer.pageNumber);
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
      const meta = isMac ? event.metaKey : event.ctrlKey;

      if (meta && event.key === ",") {
        event.preventDefault();
        showSettingsWindow(this);
        return;
      }
      if (meta && event.key.toLowerCase() === "o") {
        event.preventDefault();
        void this.openDialog();
        return;
      }
      // Full screen is settled before search, because both are on F: ⌘⇧F here
      // and, on a Mac, the system's own ⌃⌘F. Neither may be read as a plain
      // ⌘F and drop the reader into the search bar.
      if (
        event.key === "F11" ||
        (meta && event.shiftKey && event.key.toLowerCase() === "f") ||
        (isMac && event.metaKey && event.ctrlKey && event.key.toLowerCase() === "f")
      ) {
        event.preventDefault();
        void this.toggleFullscreen();
        return;
      }
      if (meta && !event.shiftKey && event.key.toLowerCase() === "f") {
        event.preventDefault();
        this.openFind();
        return;
      }
      if (meta && event.key.toLowerCase() === "d") {
        event.preventDefault();
        this.toggleDark();
        return;
      }
      if (meta && event.key.toLowerCase() === "b") {
        event.preventDefault();
        this.toggleSidebar();
        return;
      }
      if (meta && !event.shiftKey && event.key.toLowerCase() === "t") {
        event.preventDefault();
        this.toggleToolbar();
        return;
      }
      if (meta && (event.key === "+" || event.key === "=")) {
        event.preventDefault();
        this.stepZoom(1);
        return;
      }
      if (meta && event.key === "-") {
        event.preventDefault();
        this.stepZoom(-1);
        return;
      }
      if (meta && event.key === "0") {
        event.preventDefault();
        this.setFit("width");
        return;
      }
      // ⌥⌘G before ⌘G, and by `code` rather than by `key`: Option turns a G
      // into a © on a Mac, so the letter is not there to test any more.
      if (meta && event.altKey && event.code === "KeyG") {
        event.preventDefault();
        this.focusPageNumber();
        return;
      }
      if (meta && !event.altKey && event.key.toLowerCase() === "g") {
        event.preventDefault();
        this.search.step(event.shiftKey ? -1 : 1);
        return;
      }
      // Back and forward through the jumps. Two bindings, because two
      // traditions: ⌘[ and ⌘] are what Preview answers to, ⌥← and ⌥→ what
      // Acrobat, Sumatra and Okular answer to, and neither camp thinks to try
      // the other's. Both are free here.
      if (
        (meta && event.key === "[") ||
        (event.altKey && !meta && event.key === "ArrowLeft")
      ) {
        event.preventDefault();
        this.goBack();
        return;
      }
      if (
        (meta && event.key === "]") ||
        (event.altKey && !meta && event.key === "ArrowRight")
      ) {
        event.preventDefault();
        this.goForward();
        return;
      }
      if (event.key === "Escape") {
        if (ui.isMenuOpen()) return;
        // Escape does one thing per press, closest layer first, and always
        // claims the key: an Escape the webview leaves unhandled reaches
        // AppKit, which drops the window out of full screen behind our back.
        event.preventDefault();
        if (!el.findBar.hidden) this.closeFind();
        else if (this.settings.fullscreen) void this.toggleFullscreen(false);
        return;
      }
      if (this.viewer.isEmpty) return;

      // Everything below is a bare key, and only a bare key. Anything still
      // holding a modifier at this point was not caught above and belongs to
      // the system: ⌘↓ and ⌘← mean "end of document" and "start of line"
      // everywhere else on a Mac and were turning pages here, and ⌥j was
      // scrolling. A shortcut this app does want is added above, where the
      // modifier is part of the test.
      if (event.metaKey || event.ctrlKey || event.altKey) return;

      switch (event.key) {
        // Left and right turn pages, in every scroll mode. Continuous
        // scrolling makes a page boundary easy to lose, and landing on the top
        // of one is the whole reason to reach for these keys rather than the
        // ones that move by a screen.
        case "ArrowRight":
          event.preventDefault();
          this.viewer.nextPage();
          break;
        case "ArrowLeft":
          event.preventDefault();
          this.viewer.previousPage();
          break;
        // Up and down are the small move, and they were the browser's own
        // until now, which is why one of them travelled further than the
        // other. One stride, two directions.
        case "ArrowDown":
          event.preventDefault();
          this.viewer.scrollByStep(1);
          break;
        case "ArrowUp":
          event.preventDefault();
          this.viewer.scrollByStep(-1);
          break;
        case "PageDown":
          event.preventDefault();
          this.viewer.scrollByViewport(1);
          break;
        case "PageUp":
          event.preventDefault();
          this.viewer.scrollByViewport(-1);
          break;
        case "Home":
          event.preventDefault();
          this.viewer.goToPage(1);
          break;
        case "End":
          event.preventDefault();
          this.viewer.goToPage(this.viewer.pageCount);
          break;
        case " ":
          event.preventDefault();
          this.viewer.scrollByViewport(event.shiftKey ? -1 : 1);
          break;
        // j and k claim the key like every other movement key here: an
        // unhandled one carries on to whatever else is listening.
        case "j":
          event.preventDefault();
          this.viewer.scrollByStep(1);
          break;
        case "k":
          event.preventDefault();
          this.viewer.scrollByStep(-1);
          break;
        case "g":
          event.preventDefault();
          this.focusPageNumber();
          break;
        default:
          break;
      }
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

