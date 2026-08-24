/* The panel on the left: the document's own table of contents, and a column
   of page thumbnails. Both are built lazily — a thumbnail is only drawn once
   it is about to be looked at. */

import type {
  PDFDocumentProxy,
  PDFPageProxy,
  RenderTask,
} from "pdfjs-dist/types/src/display/api";

import type { Theme } from "./api";
import { recolor } from "./themes";
import { isRenderCancelled, type Viewer } from "./viewer";

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
  private tab: "outline" | "pages" = "outline";
  private thumbs = new Map<number, HTMLButtonElement>();
  /** The thumbnails that carry a picture, oldest first — which is the order
      they were scrolled past in, and so the order to give them back in. */
  private drawn = new Map<number, HTMLCanvasElement>();
  /** The renders in flight, so a theme change can call off the one it is
      about to replace rather than starting a second render into the same
      canvas — which pdf.js refuses outright. */
  private tasks = new Map<number, RenderTask>();
  /** The panel width the pictures on screen were drawn for. */
  private drawnAt = 0;
  private observer: IntersectionObserver | null = null;
  private outlineButtons: { el: HTMLButtonElement; page: number }[] = [];
  private page = 1;

  constructor(
    private outlinePanel: HTMLElement,
    private pagesPanel: HTMLElement,
    private tabs: HTMLButtonElement[],
    private viewer: Viewer,
  ) {
    for (const tab of this.tabs) {
      tab.addEventListener("click", () => this.showTab(tab.dataset.tab as "outline" | "pages"));
    }
    this.showTab("outline");
  }

  showTab(name: "outline" | "pages"): void {
    this.tab = name;
    for (const tab of this.tabs) {
      tab.setAttribute("aria-selected", String(tab.dataset.tab === name));
    }
    this.outlinePanel.hidden = name !== "outline";
    this.pagesPanel.hidden = name !== "pages";
    if (name === "pages") this.revealCurrentThumb();
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

  private redrawVisible(): void {
    for (const [page, button] of this.thumbs) {
      const canvas = button.querySelector("canvas");
      if (!canvas) continue;
      const visible = this.isVisible(button);
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
    this.observer?.disconnect();
    this.observer = null;
    // Everything, released rather than merely dropped: a column of thumbnails
    // is the same kind of memory a column of pages is.
    for (const page of [...this.drawn.keys()]) this.forget(page, true);
    this.thumbs.clear();
    this.drawn.clear();
    this.tasks.clear();
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
      button.append(canvas, document.createTextNode(String(page)));
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
    this.trim();
    const doc = this.doc;
    const theme = this.theme;
    let proxy: PDFPageProxy | null = null;
    try {
      proxy = await doc.getPage(page);
      if (this.doc !== doc) return;
      const base = proxy.getViewport({ scale: 1 });
      const ratio = Math.min(window.devicePixelRatio || 1, 2);
      const width = this.thumbWidth();
      this.drawnAt = width;
      const viewport = proxy.getViewport({ scale: (width * ratio) / base.width });
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
    }
  }

  private isVisible(element: HTMLElement): boolean {
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
