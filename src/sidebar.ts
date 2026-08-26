/* The panel on the left: the document's own table of contents, and a column
   of page thumbnails. Both are built lazily — a thumbnail is only drawn once
   it is about to be looked at. */

import type {
  PDFDocumentProxy,
  PDFPageProxy,
  RenderTask,
} from "pdfjs-dist/types/src/display/api";

import type { Mark, Theme } from "./api";
import { hydrateIcons } from "./icons";
import { recolor } from "./themes";
import { isRenderCancelled, type Viewer } from "./viewer";

/** The three things the panel can be showing. Results only exists while there
    is a search to show, and its tab is hidden the rest of the time. */
type Tab = "outline" | "pages" | "results";

type OutlineNode = {
  title: string;
  items: OutlineNode[];
  dest: unknown;
  url: string | null;
};

/** The size an empty thumbnail holds before it is drawn, so the column has
    its shape from the first frame. */
const THUMB_PLACEHOLDER = 168;
/** The widest a thumbnail is ever drawn. The panel can be dragged wider than
    this; past it the picture is scaled up, which nobody reading a thumbnail
    will mind, and the memory a whole column of them costs stays bounded. */
const THUMB_MAX = 400;
/**
 * How many drawn thumbnails to keep.
 *
 * A thumbnail is a canvas, and at the widths this panel can be dragged to —
 * 400 CSS pixels, twice that on a retina screen — one is about a megabyte. A
 * nine hundred page book scrolled end to end in this column was nine hundred
 * of them, held for as long as the document was open, because nothing here
 * ever handed one back. The viewer has said for a long time that dropping a
 * canvas reference is not the same as freeing it; this column was the one
 * place that had not heard.
 *
 * Forty is several screens' worth, so scrolling back a little never redraws,
 * and it is bounded, which is the point.
 */
const THUMB_CACHE = 40;

export class Sidebar {
  private doc: PDFDocumentProxy | null = null;
  private theme: Theme | null = null;
  private tab: Tab = "outline";
  private thumbs = new Map<number, HTMLButtonElement>();
  /** The thumbnails that carry a picture, oldest first — which is the order
      they were scrolled past in, and so the order to give them back in. */
  private drawn = new Map<number, HTMLCanvasElement>();
  /** The renders in flight, so a theme change can call off the one it is
      about to replace rather than starting a second render into the same
      canvas — which pdf.js refuses outright. */
  private tasks = new Map<number, RenderTask>();
  /** One token per page currently being drawn, so a `draw()` evicted by
      someone else's `trim()` while still awaiting `doc.getPage` — before it
      has a `RenderTask` for `forget()` to cancel — notices and stops, rather
      than finishing unwatched and permanently escaping `THUMB_CACHE`. */
  private flights = new Map<number, symbol>();
  /** The panel width the pictures on screen were drawn for. */
  private drawnAt = 0;
  private observer: IntersectionObserver | null = null;
  private outlineButtons: { el: HTMLButtonElement; page: number }[] = [];
  /** The reader's own marks, above the document's contents. See `showMarks`. */
  private marksEl: HTMLElement | null = null;
  private page = 1;

  constructor(
    private outlinePanel: HTMLElement,
    private pagesPanel: HTMLElement,
    private resultsPanel: HTMLElement,
    private tabs: HTMLButtonElement[],
    private viewer: Viewer,
  ) {
    for (const tab of this.tabs) {
      tab.addEventListener("click", () => this.showTab(tab.dataset.tab as Tab));
    }
    this.showTab("outline");
  }

  showTab(name: Tab): void {
    this.tab = name;
    for (const tab of this.tabs) {
      tab.setAttribute("aria-selected", String(tab.dataset.tab === name));
    }
    this.outlinePanel.hidden = name !== "outline";
    this.pagesPanel.hidden = name !== "pages";
    this.resultsPanel.hidden = name !== "results";
    if (name === "pages") this.revealCurrentThumb();
  }

  /**
   * The results of a search, as a list to read rather than a count to step
   * through.
   *
   * "3 of 128" is the answer to "is it in here" and no answer at all to "which
   * one did I mean" — which is what somebody searching a long document is
   * usually asking. Every other reader shows the hits with a line of context;
   * this one showed a number.
   *
   * The tab appears with the first result and goes when the search does. It
   * does not steal the panel from the contents unless the reader was not
   * looking at anything else — a search is a thing you run while reading, and
   * having the chapter list vanish under you is not what was asked for.
   */
  showResults(
    results: { at: number; page: number; before: string; hit: string; after: string }[],
    total: number,
    current: number,
    onPick: (at: number) => void,
  ): void {
    const tab = this.tabs.find((button) => button.dataset.tab === "results");
    if (tab) tab.hidden = results.length === 0;
    if (results.length === 0) {
      this.resultsPanel.replaceChildren();
      if (this.tab === "results") this.showTab(this.hasOutline ? "outline" : "pages");
      return;
    }

    const list = document.createDocumentFragment();
    for (const result of results) {
      const button = document.createElement("button");
      button.className = result.at === current ? "result current" : "result";

      const where = document.createElement("span");
      where.className = "result-page";
      where.textContent = this.viewer.label(result.page);

      const line = document.createElement("span");
      line.className = "result-line";
      const hit = document.createElement("mark");
      hit.textContent = result.hit;
      line.append(
        result.before ? `…${result.before}` : "",
        hit,
        result.after ? `${result.after}…` : "",
      );

      button.append(where, line);
      button.addEventListener("click", () => onPick(result.at));
      list.append(button);
    }

    if (total > results.length) {
      const more = document.createElement("p");
      more.className = "sidebar-empty";
      more.textContent = `…and ${total - results.length} more. Ask for something narrower.`;
      list.append(more);
    }

    this.resultsPanel.replaceChildren(list);
    const showing = this.resultsPanel.querySelector(".result.current");
    showing?.scrollIntoView({ block: "nearest" });
  }

  /** Bring the results forward, which is what opening the panel from the find
      bar means. */
  showResultsTab(): void {
    if (this.resultsPanel.childElementCount > 0) this.showTab("results");
  }

  /** The chapter a page falls in, if the document says. A mark named for the
      section it sits in is worth a great deal more than one named "Page 214",
      and the outline has already been walked. */
  sectionFor(page: number): string {
    let best: { el: HTMLButtonElement; page: number } | null = null;
    for (const entry of this.outlineButtons) {
      if (entry.page <= page && (!best || entry.page >= best.page)) best = entry;
    }
    return best?.el.textContent?.trim() ?? "";
  }

  /** True when the document has a table of contents worth showing. */
  get hasOutline(): boolean {
    return this.outlineButtons.length > 0;
  }

  async setDocument(doc: PDFDocumentProxy | null, theme: Theme): Promise<void> {
    this.reset();
    this.doc = doc;
    this.theme = theme;
    if (!doc) return;

    this.buildThumbs(doc);
    const outline = (await doc.getOutline()) as OutlineNode[] | null;
    if (this.doc !== doc) return;
    await this.buildOutline(doc, outline);
    if (!this.hasOutline) this.showTab("pages");
  }

  /** A copy, for the reason `Viewer.setTheme` keeps one: the theme editor
      previews by handing over the draft it goes on editing in place. */
  setTheme(theme: Theme): void {
    const changed =
      this.theme?.text !== theme.text ||
      this.theme?.background !== theme.background ||
      this.theme?.recolor !== theme.recolor;
    this.theme = { ...theme };
    if (!changed) return;
    // Thumbnails carry the theme too, so the panel and the page agree.
    this.redrawVisible();
  }

  /** The panel has been resized, so the pictures in it are drawn for the width
      it used to be. Anything on screen is redrawn at the new one; the rest are
      drawn as they come into view, which they were going to be anyway. */
  resize(): void {
    // A drag arrives a pixel at a time, and redrawing a column of pages on
    // every one of them would be a great deal of work for a picture the width
    // of a thumb. Wait until the change is worth seeing.
    if (Math.abs(this.thumbWidth() - this.drawnAt) < 24) return;
    this.redrawVisible();
  }

  /**
   * Only a page that already carries a picture needs anything done to it: an
   * undrawn thumbnail is still a placeholder, tied to no theme, and will draw
   * under the current one whenever it next comes into view on its own.
   * Walking `this.thumbs` instead — every page in the document — cost a
   * `getBoundingClientRect` per page on every call, and `setTheme` calls this
   * on every tick of a colour dragged live in the editor. Walking `this.drawn`
   * bounds it to `THUMB_CACHE`.
   */
  private redrawVisible(): void {
    for (const [page, canvas] of [...this.drawn]) {
      const button = this.thumbs.get(page);
      const visible = button ? this.isVisible(button) : false;
      // A picture drawn under the old theme is wrong wherever it is. The ones
      // on screen are redrawn now; the rest are forgotten and will be drawn
      // when they next come into view, which they were going to be anyway.
      // The visible ones keep their bitmap until the new one lands over it,
      // so the column does not blink.
      this.forget(page, !visible);
      if (visible) void this.draw(page, canvas);
    }
  }

  /**
   * Stop drawing a thumbnail and forget that it was drawn.
   *
   * `release` hands the bitmap back as well, by resizing the canvas — which is
   * what actually frees the surface, where dropping the reference only makes
   * it collectable. Back to the placeholder size rather than to nothing: the
   * column takes its shape from the canvas's own proportions, so a 0×0 one
   * would collapse the row and jump everything below it.
   */
  private forget(page: number, release: boolean): void {
    this.tasks.get(page)?.cancel();
    this.tasks.delete(page);
    this.flights.delete(page);
    const canvas = this.drawn.get(page);
    this.drawn.delete(page);
    if (release && canvas) {
      canvas.width = THUMB_PLACEHOLDER;
      canvas.height = Math.round(THUMB_PLACEHOLDER * 1.414);
    }
  }

  /** Give back the oldest thumbnails once there are too many, never one that
      is on screen. */
  private trim(): void {
    if (this.drawn.size <= THUMB_CACHE) return;
    for (const page of [...this.drawn.keys()]) {
      if (this.drawn.size <= THUMB_CACHE) break;
      const button = this.thumbs.get(page);
      if (button && this.isVisible(button)) continue;
      this.forget(page, true);
    }
  }

  /** How wide to draw a thumbnail: as wide as the panel gives it, within
      reason. Read at draw time rather than kept, so the first thumbnail after
      a resize is already the right size. */
  private thumbWidth(): number {
    const room = this.pagesPanel.clientWidth - 20;
    return Math.max(120, Math.min(THUMB_MAX, room || THUMB_PLACEHOLDER));
  }

  /**
   * The places the reader has put a pin in, above the document's own contents.
   *
   * Above rather than beside: a mark is the reader's own note of where they
   * were going back to, and there are never many — a section of four entries
   * over a chapter list of two hundred is the right way round. The panel is
   * still Contents, because that is what both halves of it are.
   */
  showMarks(marks: Mark[], onPick: (mark: Mark) => void, onDrop: (mark: Mark) => void): void {
    this.marksEl?.remove();
    this.marksEl = null;
    if (marks.length === 0) return;

    const box = document.createElement("div");
    box.className = "marks";
    const heading = document.createElement("p");
    heading.className = "marks-title";
    heading.textContent = "Marked";
    box.append(heading);

    for (const mark of marks) {
      const row = document.createElement("div");
      row.className = "mark";

      const go = document.createElement("button");
      go.className = "mark-go";
      go.textContent = mark.title || `Page ${this.viewer.label(mark.page)}`;
      go.title = `Page ${this.viewer.label(mark.page)}`;
      go.addEventListener("click", () => onPick(mark));

      const drop = document.createElement("button");
      drop.className = "mark-drop";
      drop.setAttribute("aria-label", `Remove the mark on page ${this.viewer.label(mark.page)}`);
      drop.title = "Remove this mark";
      drop.dataset.icon = "close";
      drop.addEventListener("click", () => onDrop(mark));

      row.append(go, drop);
      box.append(row);
    }

    hydrateIcons(box);
    this.marksEl = box;
    this.outlinePanel.prepend(box);
    // The panel opens on the contents when a document has none; a document
    // with marks in it has something to show there after all.
    if (this.tab === "pages" && !this.hasOutline) this.showTab("outline");
  }

  /** The reader turned the document; the column follows. */
  rotated(): void {
    this.redrawVisible();
  }

  /** The document turned out to number its own pages. Said once, when the
      labels arrive — which is usually a moment after the column is built. */
  relabel(): void {
    for (const [page, button] of this.thumbs) {
      const number = button.querySelector(".thumb-number");
      if (number) number.textContent = this.viewer.label(page);
    }
  }

  setPage(page: number): void {
    this.page = page;
    for (const [number, button] of this.thumbs) {
      button.classList.toggle("current", number === page);
    }
    let best: { el: HTMLButtonElement; page: number } | null = null;
    for (const entry of this.outlineButtons) {
      entry.el.classList.remove("current");
      if (entry.page <= page && (!best || entry.page >= best.page)) best = entry;
    }
    best?.el.classList.add("current");
    if (this.tab === "pages") this.revealCurrentThumb();
  }

  private reset(): void {
    this.marksEl?.remove();
    this.marksEl = null;
    this.showResults([], 0, -1, () => {});
    this.observer?.disconnect();
    this.observer = null;
    // Everything, released rather than merely dropped: a column of thumbnails
    // is the same kind of memory a column of pages is.
    for (const page of [...this.drawn.keys()]) this.forget(page, true);
    this.thumbs.clear();
    this.drawn.clear();
    this.tasks.clear();
    this.flights.clear();
    this.outlineButtons = [];
    this.outlinePanel.replaceChildren();
    this.pagesPanel.replaceChildren();
  }

  /* --------------------------------------------------------- thumbnails */

  private buildThumbs(doc: PDFDocumentProxy): void {
    const fragment = document.createDocumentFragment();
    for (let page = 1; page <= doc.numPages; page++) {
      const button = document.createElement("button");
      button.className = "thumb";
      button.dataset.page = String(page);
      const canvas = document.createElement("canvas");
      canvas.width = THUMB_PLACEHOLDER;
      canvas.height = Math.round(THUMB_PLACEHOLDER * 1.414);
      const number = document.createElement("span");
      number.className = "thumb-number";
      number.textContent = this.viewer.label(page);
      button.append(canvas, number);
      button.addEventListener("click", () => this.viewer.goToPage(page));
      this.thumbs.set(page, button);
      fragment.append(button);
    }
    this.pagesPanel.append(fragment);

    this.observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          const button = entry.target as HTMLButtonElement;
          const page = Number(button.dataset.page);
          const canvas = button.querySelector("canvas");
          if (canvas) void this.draw(page, canvas);
        }
      },
      { root: this.pagesPanel, rootMargin: "200px" },
    );
    for (const button of this.thumbs.values()) this.observer.observe(button);
  }

  private async draw(page: number, canvas: HTMLCanvasElement): Promise<void> {
    if (this.drawn.has(page) || !this.doc) return;
    this.drawn.set(page, canvas);
    // A token for this flight, so it can tell — once `doc.getPage` hands
    // control back — whether it is still the one `this.drawn` wants. Between
    // now and then there is no `RenderTask` yet for `forget()` to cancel, so
    // a `trim()` from an unrelated page's `draw()` can otherwise evict this
    // entry — release the canvas, drop it from `this.drawn` — while this
    // coroutine sails on regardless, finishes unwatched, and the picture it
    // leaves behind never counts against `THUMB_CACHE` again.
    const flight = Symbol();
    this.flights.set(page, flight);
    this.trim();
    const doc = this.doc;
    const theme = this.theme;
    let proxy: PDFPageProxy | null = null;
    try {
      proxy = await doc.getPage(page);
      if (this.doc !== doc || this.flights.get(page) !== flight) return;
      // Turned the way the page is turned: a thumbnail column that stays
      // sideways under a document the reader has straightened is a column of
      // pictures of something else.
      const rotation = proxy.rotate + this.viewer.turned;
      const base = proxy.getViewport({ scale: 1, rotation });
      const ratio = Math.min(window.devicePixelRatio || 1, 2);
      const width = this.thumbWidth();
      this.drawnAt = width;
      const viewport = proxy.getViewport({ scale: (width * ratio) / base.width, rotation });
      canvas.width = Math.floor(viewport.width);
      canvas.height = Math.floor(viewport.height);
      const ctx = canvas.getContext("2d", { alpha: false });
      if (!ctx) return;
      const task = proxy.render({ canvas, canvasContext: ctx, viewport, background: "#ffffff" });
      this.tasks.set(page, task);
      try {
        await task.promise;
      } finally {
        if (this.tasks.get(page) === task) this.tasks.delete(page);
      }
      if (this.doc !== doc) return;
      if (theme) recolor(ctx, canvas.width, canvas.height, theme);
    } catch (error) {
      // A cancelled render has already been forgotten by whoever cancelled it,
      // and forgetting it again here would undo the redraw that replaced it.
      if (!isRenderCancelled(error)) this.drawn.delete(page);
    } finally {
      // The reason the viewer has an LRU and two caps: pdf.js holds a page's
      // parsed operator list — every decoded image on it — from the first
      // render until this is called. Scrolling this column past a scanned book
      // parsed every page of it and gave none of it back, which is exactly the
      // cost `IMAGE_PAGE_CACHE` was measured into existence to prevent, coming
      // in through a door the viewer's accounting cannot see. `cleanup` defers
      // while a render is running, so it cannot pull a page out from under a
      // render of the same page — but the viewer's own page can be idle and
      // still mounted, which `cleanup` has no way to see; `isMounted` is what
      // the viewer's own `trimPages` checks before evicting, and this is that
      // same rule applied through the door the viewer's accounting cannot see.
      if (!this.viewer.isMounted(page)) proxy?.cleanup();
      if (this.flights.get(page) === flight) this.flights.delete(page);
    }
  }

  /**
   * Whether a thumbnail's button is close enough to the viewport to count as
   * on screen.
   *
   * `offsetParent` is null exactly when something up the tree is
   * `display: none` — the sidebar closed, or the Outline tab showing instead
   * of Pages — and that check has to come first. Every rect collapses to
   * `0,0,0,0` under `display: none`, panel included, and the comparison below
   * is true for `0,0` against `0,0`: every thumbnail in the document read as
   * on screen the moment the panel was not, so a theme change with the
   * sidebar closed — the default — drew and decoded every page in the book.
   */
  private isVisible(element: HTMLElement): boolean {
    if (element.offsetParent === null) return false;
    const panel = this.pagesPanel.getBoundingClientRect();
    const rect = element.getBoundingClientRect();
    return rect.bottom > panel.top - 200 && rect.top < panel.bottom + 200;
  }

  private revealCurrentThumb(): void {
    const button = this.thumbs.get(this.page);
    if (!button) return;
    const panel = this.pagesPanel.getBoundingClientRect();
    const rect = button.getBoundingClientRect();
    if (rect.top < panel.top || rect.bottom > panel.bottom) {
      button.scrollIntoView({ block: "center" });
    }
  }

  /* ------------------------------------------------------------ outline */

  private async buildOutline(
    doc: PDFDocumentProxy,
    outline: OutlineNode[] | null,
  ): Promise<void> {
    if (!outline || outline.length === 0) {
      const empty = document.createElement("p");
      empty.className = "sidebar-empty";
      empty.textContent = "This document has no table of contents.";
      this.outlinePanel.append(empty);
      return;
    }

    const build = async (nodes: OutlineNode[], depth: number, into: HTMLElement) => {
      for (const node of nodes) {
        const page = await pageOf(doc, node.dest);
        if (this.doc !== doc) return;
        const row = document.createElement("div");
        const button = document.createElement("button");
        button.className = "outline-item";
        button.style.paddingLeft = `${8 + depth * 14}px`;
        button.textContent = node.title.trim() || "Untitled";
        button.title = button.textContent;
        if (page) {
          button.dataset.page = String(page);
          button.addEventListener("click", () => this.viewer.goToPage(page));
          this.outlineButtons.push({ el: button, page });
        }
        row.append(button);
        into.append(row);
        if (node.items && node.items.length > 0) {
          const children = document.createElement("div");
          children.className = "outline-children";
          row.append(children);
          await build(node.items, depth + 1, children);
        }
      }
    };

    await build(outline, 0, this.outlinePanel);
    this.setPage(this.page);
  }
}

async function pageOf(doc: PDFDocumentProxy, dest: unknown): Promise<number | null> {
  try {
    const explicit = typeof dest === "string" ? await doc.getDestination(dest) : dest;
    if (!Array.isArray(explicit) || explicit.length === 0) return null;
    const target = explicit[0];
    if (typeof target === "number") return target + 1;
    if (target && typeof target === "object") {
      return (await doc.getPageIndex(target as never)) + 1;
    }
  } catch {
    /* a broken destination is not worth a message */
  }
  return null;
}
