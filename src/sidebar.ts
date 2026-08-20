/* The panel on the left: the document's own table of contents, and a column
   of page thumbnails. Both are built lazily — a thumbnail is only drawn once
   it is about to be looked at. */

import type { PDFDocumentProxy } from "pdfjs-dist/types/src/display/api";

import type { Theme } from "./api";
import { recolor } from "./themes";
import type { Viewer } from "./viewer";

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

export class Sidebar {
  private doc: PDFDocumentProxy | null = null;
  private theme: Theme | null = null;
  private tab: "outline" | "pages" = "outline";
  private thumbs = new Map<number, HTMLButtonElement>();
  private drawn = new Set<number>();
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

  setTheme(theme: Theme): void {
    const changed =
      this.theme?.text !== theme.text ||
      this.theme?.background !== theme.background ||
      this.theme?.recolor !== theme.recolor;
    this.theme = theme;
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
    this.drawn.clear();
    for (const [page, button] of this.thumbs) {
      const canvas = button.querySelector("canvas");
      if (canvas && this.isVisible(button)) void this.draw(page, canvas);
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
    this.thumbs.clear();
    this.drawn.clear();
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
    this.drawn.add(page);
    const doc = this.doc;
    const theme = this.theme;
    try {
      const proxy = await doc.getPage(page);
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
      await proxy.render({ canvas, canvasContext: ctx, viewport, background: "#ffffff" }).promise;
      if (this.doc !== doc) return;
      if (theme) recolor(ctx, canvas.width, canvas.height, theme);
    } catch {
      this.drawn.delete(page);
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
