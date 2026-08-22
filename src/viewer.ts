/* The reader itself: layout, rendering and scrolling.
 *
 * Pages are laid out once, in advance, from their real dimensions, and the
 * scroll container is given the full height straight away. Only the pages near
 * the viewport exist in the DOM, so a nine hundred page book costs the same as
 * a two page letter, and the scrollbar tells the truth from the first frame. */

import {
  getDocument,
  GlobalWorkerOptions,
  PasswordResponses,
  PDFDataRangeTransport,
  PixelsPerInch,
  RenderingCancelledException,
  TextLayer,
} from "pdfjs-dist";
import type {
  PDFDocumentLoadingTask,
  PDFDocumentProxy,
  PDFPageProxy,
  RenderTask,
} from "pdfjs-dist/types/src/display/api";
// The minified worker, deliberately. Vite copies a `?url` import through
// untouched — it is an asset rather than part of the module graph, so it never
// meets the minifier — and importing the development build shipped a megabyte
// of whitespace and comments that the worker then had to parse at every open.
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

import { openForReading, readRange, type Theme } from "./api";
import { recolor, type Rect, restoreImages } from "./themes";

GlobalWorkerOptions.workerSrc = workerUrl;

/** An absolute URL for one of pdf.js's runtime data directories.
 *
 * These are handed to the worker, where a relative address would be resolved
 * against the worker script rather than the page — quietly out of reach. The
 * worker then cannot fetch what it needs, and the pages that need it come out
 * missing pieces: scanned documents lose their text, which lives in image
 * masks, and CJK documents lose their glyphs. */
const asset = (path: string): string => new URL(path, window.location.href).href;

/** Thrown when the reader is asked for a document's password and would rather
    not give one. Not a failure — there is nothing to report and nothing to put
    right — so the only thing anyone does with this is recognise it and say
    nothing. */
export class Cancelled extends Error {
  constructor() {
    super("The document was not opened.");
  }
}

export type FitMode = "width" | "page" | "actual";
export type ScrollMode = "continuous" | "paged";

export type Match = {
  page: number;
  itemStart: number;
  offsetStart: number;
  itemEnd: number;
  offsetEnd: number;
};

const PAD_X = 20;
const PAD_Y = 20;
/** How much of the document to ask for at a time. pdf.js's own default; big
    enough that a page rarely needs two, small enough that the end of a file is
    cheap to reach. */
const RANGE_CHUNK = 64 * 1024;
/** How many page proxies to keep. A proxy holds its parsed operator list —
    every decoded image on the page — until it is cleaned up, so this is the
    ceiling on what a long book costs after being read end to end. */
const PAGE_CACHE = 48;
/** How far beyond the viewport pages are kept alive, in viewport heights. */
const OVERSCAN = 0.6;
/** How far a wheel has to push past the end of a page before it turns it.
    Enough that resting against the edge does nothing. */
const WHEEL_TURN = 60;
/** Canvases larger than this are scaled down; browsers refuse to allocate
    beyond roughly this many pixels, and nothing is gained past it anyway. */
const MAX_CANVAS_PIXELS = 12_000_000;

type Slot = {
  index: number;
  el: HTMLDivElement;
  canvas: HTMLCanvasElement | null;
  textLayer: TextLayer | null;
  textEl: HTMLDivElement | null;
  highlightEl: HTMLDivElement | null;
  linkEl: HTMLDivElement | null;
  task: RenderTask | null;
  renderedKey: string;
};

type Box = { top: number; left: number; width: number; height: number; scale: number };

/** A place in the document and where on the screen it was, so a zoom can put
    it back under the same finger. */
type Point = {
  index: number;
  fx: number;
  fy: number;
  viewX: number;
  viewY: number;
};

/** One link on a page, in fractions of the page, and where it leads. */
type Link = {
  x: number;
  y: number;
  width: number;
  height: number;
  url?: string;
  dest?: unknown;
};

export type ViewerCallbacks = {
  onPageChange(page: number, count: number): void;
  onScroll(): void;
  onError(message: string): void;
  /** A link in the document that points somewhere outside it. */
  onExternalLink(url: string): void;
  /** The document is encrypted. Ask for the password, or return null to give
      up; `wrong` is true when the last answer was refused. */
  onPassword(wrong: boolean): Promise<string | null>;
};

/**
 * Reading a document in pieces.
 *
 * pdf.js is given the length of the file and a way to ask for parts of it, so
 * it fetches the cross-reference table at the end and then only the pages
 * being looked at. The alternative — handing it the whole file — meant three
 * copies of every document in memory at once, and reading all of a five
 * hundred megabyte scan before showing any of it.
 */
class FileRange extends PDFDataRangeTransport {
  private cancelled = false;

  constructor(
    private path: string,
    length: number,
  ) {
    super(length, null, false);
  }

  requestDataRange(begin: number, end: number): void {
    if (this.cancelled) return;
    void readRange(this.path, begin, end - begin)
      .then((chunk) => {
        if (!this.cancelled) this.onDataRange(begin, chunk);
      })
      .catch(() => {
        // A range that cannot be read is a document that cannot be read.
        // pdf.js reports that itself, through the load or the render.
      });
  }

  abort(): void {
    this.cancelled = true;
  }
}

export class Viewer {
  private doc: PDFDocumentProxy | null = null;
  /** The load in flight or the one that finished. Kept so it can be destroyed:
      `doc` is only assigned once the load resolves, so a document abandoned
      while it was still loading was left with nothing referring to it — and a
      pdf.js loading task owns a worker, which then ran on to finish parsing a
      document nobody would read, and held the result. Two impatient clicks in
      the recents list was enough. */
  private loading: PDFDocumentLoadingTask | null = null;
  private sizes: { width: number; height: number }[] = [];
  private boxes: Box[] = [];
  private slots = new Map<number, Slot>();
  private pageCache = new Map<number, Promise<PDFPageProxy>>();
  private linkCache = new Map<number, Link[]>();
  /** A match asked for but not yet shown, because its page had no text layer
      when it was asked for. Index into `matches`; -1 when there is none. */
  private pendingReveal = -1;
  /** One wheel gesture, for turning pages: when it was last heard from, how
      far it has pushed past the edge, and whether it has already turned. */
  private wheel = { at: 0, accumulated: 0, turned: false };
  /** Whether the page-turning wheel listener is attached. See `setScrollMode`. */
  private wheelBound = false;
  private queue: number[] = [];
  private rendering = false;
  private frame = 0;
  private contentWidth = 0;

  private theme: Theme | null = null;
  private preserveImages = false;
  private fit: FitMode = "width";
  private zoomFactor = 1;
  private gap = 16;
  private mode: ScrollMode = "continuous";

  private current = 1;
  private matches: Match[] = [];
  private currentMatch = -1;
  private highlightAll = true;
  /** Bumped whenever the background measuring should stop, so a document put
      down mid-measure does not go on laying out the one after it. */
  private measuring = 0;

  constructor(
    private container: HTMLElement,
    private pagesEl: HTMLElement,
    private callbacks: ViewerCallbacks,
  ) {
    this.container.addEventListener("scroll", this.onScroll, { passive: true });
    this.watchDensity();
  }

  /* ------------------------------------------------------------ lifecycle */

  async load(path: string): Promise<PDFDocumentProxy> {
    this.close();
    const length = await openForReading(path);
    const task = getDocument({
      range: new FileRange(path, length),
      rangeChunkSize: RANGE_CHUNK,
      // Ask for what is needed and nothing else. Without these two, pdf.js
      // reads the file from one end to the other in the background as well,
      // which is exactly the cost the range transport exists to avoid.
      disableAutoFetch: true,
      disableStream: true,
      cMapUrl: asset("pdfjs/cmaps/"),
      cMapPacked: true,
      standardFontDataUrl: asset("pdfjs/standard_fonts/"),
      iccUrl: asset("pdfjs/iccs/"),
      wasmUrl: asset("pdfjs/wasm/"),
    });
    this.loading = task;
    // The rejection that a decline produces travels back through the worker
    // and comes out the other side as something else entirely, so whether the
    // reader declined is remembered here rather than read off the error.
    let declined = false;

    // An encrypted document asks rather than fails. Left to itself this comes
    // back as a rejected promise indistinguishable from a corrupt file, and
    // "Something went wrong" is the wrong thing to tell someone whose PDF is
    // merely locked.
    task.onPassword = (respond: (password: string | Error) => void, reason: number) => {
      void this.callbacks
        .onPassword(reason === PasswordResponses.INCORRECT_PASSWORD)
        .then((password) => {
          if (password === null) {
            // Declined. This has to be an Error rather than an empty string:
            // pdf.js treats any string as another attempt, so answering "" got
            // the question asked again, and neither Escape nor "Not now" could
            // ever get out of it. An Error rejects the load, which is what
            // giving up means.
            declined = true;
            respond(new Error("cancelled"));
            return;
          }
          respond(password);
        });
    };

    let doc: PDFDocumentProxy;
    try {
      doc = await task.promise;
    } catch (error) {
      throw declined ? new Cancelled() : error;
    }
    this.doc = doc;

    // Measure the first page and paint. The rest are measured behind the
    // reader's back.
    //
    // Every page used to be fetched and measured before anything appeared, on
    // the grounds that a page proxy is cheap next to a render — which is true
    // of one page and not of two thousand. Nothing was on screen until the
    // last of them came back. Most documents are one size throughout, so the
    // first page is a good guess for all of them, and where it is wrong the
    // correction arrives within a second and moves pages the reader has not
    // reached yet.
    const first = await this.page(0);
    if (this.doc !== doc) return doc;
    const view = first.getViewport({ scale: 1 });
    const estimate = { width: view.width, height: view.height };
    this.sizes = new Array(doc.numPages).fill(estimate);
    this.relayout();

    void this.measureRest(doc, estimate);
    return doc;
  }

  /** Measure the pages the first one was standing in for, in batches, letting
      the app breathe between them. A page whose real size differs moves the
      pages below it, so the layout is redone — but only when something
      actually changed, and never more than once a batch. */
  private async measureRest(
    doc: PDFDocumentProxy,
    estimate: { width: number; height: number },
  ): Promise<void> {
    const run = ++this.measuring;
    const batch = 24;
    let changed = false;

    for (let start = 1; start < doc.numPages; start += batch) {
      const pending: Promise<void>[] = [];
      for (let n = start; n < Math.min(start + batch, doc.numPages); n++) {
        pending.push(
          this.page(n)
            .then((page) => {
              const view = page.getViewport({ scale: 1 });
              if (view.width !== estimate.width || view.height !== estimate.height) {
                changed = true;
              }
              this.sizes[n] = { width: view.width, height: view.height };
            })
            .catch(() => {
              // A page that cannot be measured keeps the estimate; the render
              // is where an unreadable page is reported.
            }),
        );
      }
      await Promise.all(pending);
      if (this.doc !== doc || this.measuring !== run) return;
      if (changed) {
        this.relayout();
        changed = false;
      }
    }
  }

  close(): void {
    this.measuring++;
    for (const slot of this.slots.values()) this.discard(slot);
    this.slots.clear();
    this.queue = [];
    this.releasePages();
    this.linkCache.clear();
    this.pendingReveal = -1;
    this.matches = [];
    this.currentMatch = -1;
    this.sizes = [];
    this.boxes = [];
    this.pagesEl.replaceChildren();
    this.pagesEl.style.height = "0px";
    // Destroy the load, not just the document: the two are the same thing once
    // it has resolved, and only the load exists before that.
    const task = this.loading;
    this.loading = null;
    this.doc = null;
    void task?.destroy().catch(() => {});
    // Deliberately not closing the file here. `load` calls `close` first, and
    // a fire-and-forget close could land after the open that followed it and
    // shut the document it had just opened — `open_for_reading` replaces
    // whatever was there anyway, so there is nothing to close first. Putting
    // the document down for good goes through `clearDocument`, which is the
    // only place that means it.
    this.current = 1;
  }

  /** A page proxy, kept only while it is worth keeping.
   *
   * pdf.js holds a page's parsed operator list — every decoded image on it —
   * from the first render until `cleanup()` is called, and nothing called it.
   * Keeping every proxy therefore meant keeping the render state of every page
   * ever looked at: a long illustrated book grew for as long as it was being
   * read and never gave any of it back. */
  private page(index: number): Promise<PDFPageProxy> {
    const known = this.pageCache.get(index);
    if (known) {
      // Re-inserting moves it to the end, which is what makes this an LRU.
      this.pageCache.delete(index);
      this.pageCache.set(index, known);
      return known;
    }
    const pending = this.doc!.getPage(index + 1);
    this.pageCache.set(index, pending);
    this.trimPages();
    return pending;
  }

  /** Drop the least recently wanted proxies, never one that is on screen. */
  private trimPages(): void {
    if (this.pageCache.size <= PAGE_CACHE) return;
    for (const index of [...this.pageCache.keys()]) {
      if (this.pageCache.size <= PAGE_CACHE) break;
      if (this.slots.has(index)) continue;
      const pending = this.pageCache.get(index)!;
      this.pageCache.delete(index);
      // `cleanup` defers while a render is running, so this cannot pull a page
      // out from under one.
      void pending.then((page) => page.cleanup()).catch(() => {});
    }
  }

  private releasePages(): void {
    for (const pending of this.pageCache.values()) {
      void pending.then((page) => page.cleanup()).catch(() => {});
    }
    this.pageCache.clear();
  }

  /* -------------------------------------------------------------- getters */

  get pageCount(): number {
    return this.sizes.length;
  }

  get pageNumber(): number {
    return this.current;
  }

  get document(): PDFDocumentProxy | null {
    return this.doc;
  }

  get isEmpty(): boolean {
    return this.doc === null;
  }

  /** Where the reader is, precisely enough to come back to it at any zoom. */
  position(): { page: number; offset: number } {
    const box = this.boxes[this.current - 1];
    if (!box) return { page: this.current, offset: 0 };
    const offset = (this.container.scrollTop - box.top) / Math.max(box.height, 1);
    return { page: this.current, offset: Math.max(-0.2, Math.min(1, offset)) };
  }

  /** The zoom actually in force, as a fraction of true size. */
  zoomPercent(): number {
    const box = this.boxes[this.current - 1];
    if (!box) return this.zoomFactor * 100;
    return (box.scale / PixelsPerInch.PDF_TO_CSS_UNITS) * 100;
  }

  /* -------------------------------------------------------------- options */

  setTheme(theme: Theme, preserveImages: boolean): void {
    const changed =
      this.theme?.id !== theme.id ||
      this.theme?.text !== theme.text ||
      this.theme?.background !== theme.background ||
      this.theme?.recolor !== theme.recolor ||
      this.theme?.link !== theme.link ||
      this.theme?.accent !== theme.accent ||
      this.preserveImages !== preserveImages;
    this.theme = theme;
    this.preserveImages = preserveImages;
    if (changed) this.repaint();
  }

  /** Change the zoom, optionally keeping a point on the screen still.
   *
   * `focus` is where the gesture is — the middle of a pinch, or the pointer
   * under a ctrl+wheel — in client coordinates. Without it a zoom keeps the
   * top edge of the window, which is what `position()` describes, so pinching
   * on a figure halfway down the page pushed the figure away from the fingers
   * doing the pinching. */
  setFit(fit: FitMode, zoom = this.zoomFactor, focus?: { x: number; y: number }): void {
    this.fit = fit;
    this.zoomFactor = zoom;
    this.relayout(focus);
  }

  setGap(gap: number): void {
    this.gap = gap;
    this.relayout();
  }

  /** Continuous or one page at a time.
   *
   * The wheel listener comes and goes with the mode, and that is the point of
   * doing this here. It has to be non-passive — turning a page means stopping
   * the window rubber-banding against an edge it is about to leave — and a
   * non-passive wheel listener on a scroll container makes the browser wait
   * for the main thread before it will scroll at all. Left permanently
   * attached, continuous scrolling paid for a page-turning gesture it does not
   * have. */
  setScrollMode(mode: ScrollMode): void {
    if (mode === this.mode && this.wheelBound === (mode === "paged")) {
      this.relayout();
      return;
    }
    this.mode = mode;
    this.bindWheel(mode === "paged");
    this.relayout();
  }

  private bindWheel(on: boolean): void {
    if (on === this.wheelBound) return;
    this.wheelBound = on;
    if (on) this.container.addEventListener("wheel", this.onWheel, { passive: false });
    else this.container.removeEventListener("wheel", this.onWheel);
  }

  /** Repaint when the window moves to a screen of a different density.
   *
   * How sharply a page is drawn comes from `devicePixelRatio`, and the density
   * is part of what identifies a rendered page — so a canvas drawn for a
   * Retina screen is wrong on the 1× monitor next to it, and vice versa.
   * Nothing announces this: `matchMedia` on the current resolution fires once
   * when it stops being the current one, and then has to be asked again about
   * the new one. */
  private watchDensity(): void {
    const arm = () => {
      const ratio = window.devicePixelRatio || 1;
      const query = window.matchMedia(`(resolution: ${ratio}dppx)`);
      const once = () => {
        query.removeEventListener("change", once);
        arm();
        this.repaint();
      };
      query.addEventListener("change", once);
    };
    arm();
  }

  /* --------------------------------------------------------------- layout */

  relayout(focus?: { x: number; y: number }): void {
    if (!this.doc || this.sizes.length === 0) return;
    const held = focus ? this.pointAt(focus) : null;
    const anchor = this.position();
    // The side margin frames a page that is narrower than the window. Fit
    // width is the one mode whose whole job is that there is no such gap, so
    // it does not get one: charging the page for a margin it is meant to fill
    // left a strip of ground down each side of a page that had supposedly
    // reached both edges.
    const padX = this.fit === "width" ? 0 : PAD_X;
    const availableWidth = Math.max(this.container.clientWidth - padX * 2, 120);
    const availableHeight = Math.max(this.container.clientHeight - PAD_Y * 2, 120);

    const scaleFor = (size: { width: number; height: number }): number => {
      switch (this.fit) {
        case "width":
          return availableWidth / size.width;
        case "page":
          return Math.min(availableWidth / size.width, availableHeight / size.height);
        default:
          return PixelsPerInch.PDF_TO_CSS_UNITS * this.zoomFactor;
      }
    };

    const visible = this.mode === "paged" ? [this.current - 1] : this.sizes.map((_, i) => i);
    const boxes: Box[] = new Array(this.sizes.length);
    let width = 0;
    for (const index of visible) {
      const size = this.sizes[index];
      const scale = scaleFor(size);
      width = Math.max(width, Math.round(size.width * scale));
    }
    this.contentWidth = Math.max(width, availableWidth) + padX * 2;

    let top = PAD_Y;
    for (const index of visible) {
      const size = this.sizes[index];
      const scale = scaleFor(size);
      const pageWidth = Math.round(size.width * scale);
      const pageHeight = Math.round(size.height * scale);
      boxes[index] = {
        top,
        left: Math.round((this.contentWidth - pageWidth) / 2),
        width: pageWidth,
        height: pageHeight,
        scale,
      };
      top += pageHeight + this.gap;
    }
    this.boxes = boxes;

    this.pagesEl.style.width = `${this.contentWidth}px`;
    this.pagesEl.style.height = `${Math.max(top - this.gap + PAD_Y, 0)}px`;

    // Every mounted page moved; re-place and re-render them where they landed.
    for (const slot of this.slots.values()) this.place(slot);
    if (held) this.restorePoint(held);
    else this.scrollTo(anchor.page, anchor.offset);
    this.update();
  }

  /** The spot in the document under a point on the screen, described so that
      it survives a change of scale: which page, and how far across and down
      it — plus where on the screen it was, so it can be put back there. */
  private pointAt(focus: { x: number; y: number }): Point | null {
    const view = this.container.getBoundingClientRect();
    const docY = this.container.scrollTop + (focus.y - view.top);
    const docX = this.container.scrollLeft + (focus.x - view.left);
    const index =
      this.mode === "paged" ? this.current - 1 : this.lastBoxStartingAbove(docY);
    const box = this.boxes[index];
    if (!box) return null;
    return {
      index,
      fx: (docX - box.left) / Math.max(box.width, 1),
      fy: (docY - box.top) / Math.max(box.height, 1),
      viewX: focus.x - view.left,
      viewY: focus.y - view.top,
    };
  }

  private restorePoint(point: Point): void {
    const box = this.boxes[point.index];
    if (!box) return;
    const top = box.top + point.fy * box.height - point.viewY;
    const left = box.left + point.fx * box.width - point.viewX;
    this.container.scrollTop = Math.max(0, top);
    this.container.scrollLeft = Math.max(0, left);
    this.trackCurrentPage();
  }

  private place(slot: Slot): void {
    const box = this.boxes[slot.index];
    if (!box) return;
    slot.el.style.transform = `translate(${box.left}px, ${box.top}px)`;
    slot.el.style.width = `${box.width}px`;
    slot.el.style.height = `${box.height}px`;
    slot.el.style.setProperty("--total-scale-factor", String(box.scale));
  }

  /* ------------------------------------------------------------ scrolling */

  private onScroll = (): void => {
    this.callbacks.onScroll();
    this.update();
  };

  /** Turning the page with the wheel, one page at a time.
   *
   * In paged mode the scroll container holds exactly one page, so a page that
   * fits the window cannot be scrolled at all and a taller one stops dead at
   * its own bottom edge. Either way the reader pushes and nothing happens,
   * which is the one gesture everybody tries first. Past the edge, the scroll
   * turns the page instead.
   *
   * One page per gesture: a trackpad flick keeps sending events for about a
   * second after the fingers have gone, and a page each would be a flick
   * through the whole chapter. A gap in the events is what counts as letting
   * go, and the threshold keeps a nudge against the end of a page from
   * turning it. */
  private onWheel = (event: WheelEvent): void => {
    if (this.mode !== "paged" || event.deltaY === 0) return;
    const now = performance.now();
    const gesture = this.wheel;
    if (now - gesture.at > 140) {
      gesture.accumulated = 0;
      gesture.turned = false;
    }
    gesture.at = now;
    if (gesture.turned) return;

    const room = this.container.scrollHeight - this.container.clientHeight;
    const down = event.deltaY > 0;
    const atEdge = down
      ? this.container.scrollTop >= room - 1
      : this.container.scrollTop <= 1;
    if (!atEdge) {
      gesture.accumulated = 0;
      return;
    }
    if ((down && this.current >= this.pageCount) || (!down && this.current <= 1)) return;

    gesture.accumulated += event.deltaY;
    if (Math.abs(gesture.accumulated) < WHEEL_TURN) return;
    gesture.accumulated = 0;
    gesture.turned = true;
    // Stop the window rubber-banding against an edge it is about to leave.
    event.preventDefault();
    if (down) this.nextPage();
    else this.previousPage();
  };

  private update(): void {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.mount();
      this.trackCurrentPage();
    });
  }

  private mount(): void {
    if (!this.doc || this.boxes.length === 0) return;
    const top = this.container.scrollTop;
    const height = this.container.clientHeight;
    const from = top - height * OVERSCAN;
    const to = top + height * (1 + OVERSCAN);

    // Boxes are in order down the page, so the visible run can be found rather
    // than looked for. Scanning all of them cost a pass over the whole
    // document on every frame of every scroll — nine hundred pages of work to
    // discover that three of them are on screen, growing with the length of
    // the book, which is the one thing this layout was built not to do.
    const wanted: number[] = [];
    if (this.mode === "paged") {
      // One page is laid out and the rest of `boxes` is empty, so there is
      // nothing to search for.
      if (this.boxes[this.current - 1]) wanted.push(this.current - 1);
    } else {
      for (let index = this.firstBoxEndingAfter(from); index < this.boxes.length; index++) {
        const box = this.boxes[index];
        if (!box || box.top > to) break;
        wanted.push(index);
      }
    }

    for (const [index, slot] of this.slots) {
      if (!wanted.includes(index)) {
        this.discard(slot);
        this.slots.delete(index);
      }
    }

    for (const index of wanted) {
      if (this.slots.has(index)) continue;
      const slot = this.createSlot(index);
      this.slots.set(index, slot);
      // Links do not wait for paint. They are placed in fractions of the page,
      // so they are right the moment the page has a size — and asking for them
      // here rather than at the end of a render means a link answers a click
      // as soon as it is on screen, instead of a second later when the queue
      // has worked its way round to it.
      void this.attachLinks(slot);
    }

    // Nearest to the middle of the viewport first: what the reader is looking
    // at should sharpen before what they are scrolling towards.
    const middle = top + height / 2;
    this.queue = wanted
      .filter((index) => {
        const slot = this.slots.get(index)!;
        return slot.renderedKey !== this.keyFor(index);
      })
      .sort((a, b) => {
        const da = Math.abs(this.boxes[a].top + this.boxes[a].height / 2 - middle);
        const db = Math.abs(this.boxes[b].top + this.boxes[b].height / 2 - middle);
        return da - db;
      });
    void this.drain();
  }

  private trackCurrentPage(): void {
    if (this.boxes.length === 0 || this.mode === "paged") return;
    const probe = this.container.scrollTop + this.container.clientHeight * 0.35;
    const page = this.lastBoxStartingAbove(probe) + 1;
    if (page !== this.current) {
      this.current = page;
      this.callbacks.onPageChange(page, this.pageCount);
    }
  }

  /* Both searches below assume `boxes` runs in order down the page and has no
     holes, which is true in continuous mode and is why neither is used in
     paged mode — there, one page is laid out and the rest of the array is
     empty. */

  /** The first page whose bottom edge is at or below `y`. */
  private firstBoxEndingAfter(y: number): number {
    let low = 0;
    let high = this.boxes.length - 1;
    let found = this.boxes.length;
    while (low <= high) {
      const middle = (low + high) >> 1;
      const box = this.boxes[middle];
      if (!box) break;
      if (box.top + box.height >= y) {
        found = middle;
        high = middle - 1;
      } else {
        low = middle + 1;
      }
    }
    return found;
  }

  /** The last page whose top edge is at or above `y`; page one if none is. */
  private lastBoxStartingAbove(y: number): number {
    let low = 0;
    let high = this.boxes.length - 1;
    let found = 0;
    while (low <= high) {
      const middle = (low + high) >> 1;
      const box = this.boxes[middle];
      if (!box) break;
      if (box.top <= y) {
        found = middle;
        low = middle + 1;
      } else {
        high = middle - 1;
      }
    }
    return found;
  }

  scrollTo(page: number, offset = 0, smooth = false): void {
    const index = Math.max(1, Math.min(page, this.pageCount)) - 1;
    if (this.mode === "paged" && index + 1 !== this.current) {
      this.current = index + 1;
      this.callbacks.onPageChange(this.current, this.pageCount);
      this.relayout();
      this.container.scrollTop = 0;
      return;
    }
    const box = this.boxes[index];
    if (!box) return;
    // Landing on a page means the space above it starts at the top of the
    // window and the page follows: the gap between two pages, or the margin
    // above the first. Taken from where the page before this one actually
    // ends rather than from a constant — those are two different distances,
    // and using the margin between pages left a strip of the previous page
    // showing above a page the reader had just turned to.
    const previous = this.boxes[index - 1];
    const above = previous ? box.top - (previous.top + previous.height) : PAD_Y;
    const target = box.top + offset * box.height - (offset === 0 ? above : 0);
    this.container.scrollTo({ top: Math.max(0, target), behavior: smooth ? "smooth" : "auto" });
    this.current = index + 1;
    this.callbacks.onPageChange(this.current, this.pageCount);
    this.update();
  }

  goToPage(page: number): void {
    this.scrollTo(page, 0);
  }

  nextPage(): void {
    this.scrollTo(Math.min(this.current + 1, this.pageCount), 0);
  }

  previousPage(): void {
    this.scrollTo(Math.max(this.current - 1, 1), 0);
  }

  /** A nudge: the same distance whichever way it goes, and a proportion of the
      window so it feels the same in a small one as in a large one. */
  scrollByStep(direction: 1 | -1): void {
    const step = Math.max(60, Math.round(this.container.clientHeight * 0.12));
    this.container.scrollBy({ top: direction * step, behavior: "auto" });
  }

  scrollByViewport(direction: 1 | -1): void {
    if (this.mode === "paged") {
      const room = this.container.scrollHeight - this.container.clientHeight;
      const at = this.container.scrollTop;
      if ((direction === 1 && at >= room - 2) || (direction === -1 && at <= 2)) {
        direction === 1 ? this.nextPage() : this.previousPage();
        return;
      }
    }
    this.container.scrollBy({
      top: direction * (this.container.clientHeight - 60),
      behavior: "auto",
    });
  }

  /* ------------------------------------------------------------ rendering */

  /** Identity of a rendered page: change any part of it and the page is
      repainted, leave it alone and the canvas is reused.
   *
   * The screen's density belongs in here as much as the zoom does — it is half
   * of how many pixels the canvas gets. Without it, a window dragged from a
   * Retina display to the 1× monitor beside it kept every bitmap it already
   * had, and no amount of scrolling or resizing would produce a different key
   * to replace them with: the pages stayed soft, or over-sharp the other way,
   * for as long as the document was open. */
  private keyFor(index: number): string {
    const box = this.boxes[index];
    const theme = this.theme;
    const themeKey = theme?.recolor
      ? `${theme.text}|${theme.background}|${linkColor(theme)}|${
          this.preserveImages ? "img" : ""
        }`
      : `plain|${theme ? linkColor(theme) : ""}`;
    const density = window.devicePixelRatio || 1;
    return `${box ? box.scale.toFixed(3) : "0"}@${density}|${themeKey}`;
  }

  private createSlot(index: number): Slot {
    const el = document.createElement("div");
    el.className = "page";
    el.dataset.page = String(index + 1);
    const slot: Slot = {
      index,
      el,
      canvas: null,
      textLayer: null,
      textEl: null,
      highlightEl: null,
      linkEl: null,
      task: null,
      renderedKey: "",
    };
    this.place(slot);
    this.pagesEl.append(el);
    return slot;
  }

  private discard(slot: Slot): void {
    slot.task?.cancel();
    slot.task = null;
    slot.textLayer?.cancel();
    slot.textLayer = null;
    // Hand the bitmap back now rather than when the collector gets round to
    // it. A page canvas is tens of megabytes of GPU-backed surface, and three
    // of them are live at any moment; waiting for a collection that has no
    // particular reason to run is how a scroll through a long book climbs and
    // stays climbed.
    release(slot.canvas);
    slot.canvas = null;
    slot.el.remove();
  }

  private async drain(): Promise<void> {
    if (this.rendering) return;
    this.rendering = true;
    try {
      for (;;) {
        const index = this.queue.shift();
        if (index === undefined) {
          // A page can lose its render along the way — the layout changed
          // under it and its slot was replaced. Pick up whatever is still
          // unpainted rather than leaving a blank page on screen.
          const pending = [...this.slots.values()]
            .filter((slot) => slot.renderedKey !== this.keyFor(slot.index))
            .map((slot) => slot.index);
          if (pending.length === 0) break;
          this.queue = pending;
          continue;
        }
        const slot = this.slots.get(index);
        if (!slot) continue;
        const key = this.keyFor(index);
        if (slot.renderedKey === key) continue;
        await this.renderSlot(slot, key);
      }
    } finally {
      this.rendering = false;
    }
  }

  private async renderSlot(slot: Slot, key: string): Promise<void> {
    const box = this.boxes[slot.index];
    if (!box || !this.doc) {
      // No box means the layout has moved on without this slot — paged mode
      // lays out one page, so every other mounted page loses its box until
      // the next frame discards it. Marking it done matters: `drain` refills
      // an empty queue from whatever is still unpainted, and a page that can
      // never be painted would be handed straight back. That loop is all
      // microtasks, so no frame would ever run to clear the slot away, and
      // the window would stop dead.
      slot.renderedKey = key;
      return;
    }
    const theme = this.theme;
    const doc = this.doc;

    let page: PDFPageProxy;
    try {
      page = await this.page(slot.index);
    } catch {
      // Nothing to draw and nothing to be gained by trying again.
      slot.renderedKey = key;
      this.callbacks.onError(`Page ${slot.index + 1} could not be read.`);
      return;
    }
    if (this.doc !== doc || this.slots.get(slot.index) !== slot) return;

    const ratio = window.devicePixelRatio || 1;
    const area = box.width * box.height * ratio * ratio;
    const density = area > MAX_CANVAS_PIXELS ? ratio * Math.sqrt(MAX_CANVAS_PIXELS / area) : ratio;
    const viewport = page.getViewport({ scale: box.scale * density });

    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.floor(viewport.width));
    canvas.height = Math.max(1, Math.floor(viewport.height));
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) {
      slot.renderedKey = key;
      return;
    }

    const wantsImages = Boolean(theme?.recolor) && this.preserveImages;
    const task = page.render({
      canvas,
      canvasContext: ctx,
      viewport,
      background: "#ffffff",
      recordImages: wantsImages,
    });
    slot.task = task;

    try {
      await task.promise;
    } catch (error) {
      if (!(error instanceof RenderingCancelledException)) {
        // Give up on this page rather than retry it forever.
        slot.renderedKey = key;
        this.callbacks.onError(`Page ${slot.index + 1} could not be drawn.`);
      }
      return;
    } finally {
      if (slot.task === task) slot.task = null;
    }
    if (this.doc !== doc || this.slots.get(slot.index) !== slot) return;

    if (theme) {
      // Links are coloured under every theme, including the ones that leave the
      // document alone otherwise. A link that reads exactly like the sentence
      // around it is a link nobody can see, and whether the page has been
      // recoloured is beside that point.
      const links = await this.linksFor(slot.index, page);
      if (this.doc !== doc || this.slots.get(slot.index) !== slot) return;
      const coordinates: ArrayLike<number> | null = wantsImages
        ? task.imageCoordinates
        : null;
      const hasImages = Boolean(coordinates && coordinates.length > 0);
      // A copy of the page as it was drawn, to paint back over the recolouring.
      // Only a recoloured page has anything to undo, and the copy is the whole
      // canvas — at a high zoom that is tens of megabytes, so a theme that
      // leaves the document alone must not pay for it. Every zoom step
      // repaints every visible page, which is where that cost would land.
      const pristine = theme.recolor && (hasImages || links.length > 0)
        ? copyCanvas(canvas)
        : null;
      if (theme.recolor) recolor(ctx, canvas.width, canvas.height, theme);
      if (links.length > 0) {
        tintLinks(ctx, pristine, canvas.width, canvas.height, links, theme);
      }
      if (pristine && coordinates && hasImages) {
        restoreImages(ctx, pristine, canvas.width, canvas.height, coordinates);
      }
      // As big as the page it copied, and finished with.
      release(pristine);
    }

    const replaced = slot.canvas;
    slot.canvas = canvas;
    slot.el.prepend(canvas);
    replaced?.remove();
    release(replaced);
    slot.renderedKey = key;

    await this.renderText(slot, page, box.scale);
    // Links do not hold up the queue: the page is already readable without
    // them, and they are placed in fractions of the page, so they are correct
    // whenever they arrive and stay correct at every zoom afterwards.
    void this.renderLinks(slot, page);
  }

  /** The selectable text over a page.
   *
   * Built once per mounted page and then only rescaled. pdf.js lays its spans
   * out in percentages and sizes them from `--total-scale-factor`, which
   * `place()` sets on every layout — so a zoom has already moved the text
   * layer by the time anything else happens, and `update` only has to agree
   * about the number. Rebuilding it instead meant streaming the page's text
   * out of the worker again and laying out several hundred absolutely
   * positioned spans, per visible page, per step of a zoom — and it threw away
   * whatever the reader had selected while doing it. */
  private async renderText(slot: Slot, page: PDFPageProxy, scale: number): Promise<void> {
    const viewport = page.getViewport({ scale });

    if (slot.textLayer && slot.textEl) {
      slot.textLayer.update({ viewport });
      this.paintHighlights(slot);
      this.finishReveal(slot.index + 1);
      return;
    }

    slot.textEl?.remove();
    const container = document.createElement("div");
    container.className = "textLayer";
    slot.el.append(container);
    slot.textEl = container;

    const layer = new TextLayer({
      textContentSource: page.streamTextContent(),
      container,
      viewport,
    });
    slot.textLayer = layer;
    try {
      await layer.render();
    } catch {
      // A text layer that could not be built is not one to reuse.
      if (slot.textLayer === layer) slot.textLayer = null;
      return;
    }
    if (slot.textEl !== container) return;
    this.paintHighlights(slot);
    // The page a search was waiting on has just become measurable.
    this.finishReveal(slot.index + 1);
  }

  /* ----------------------------------------------------------------- links */

  /** Where the links on a page are, in fractions of the page, and where each
      one leads. Read once per page and kept: the canvas is repainted at every
      zoom and every change of theme, and asking the document again each time
      would be work for an answer that cannot have changed. */
  private async linksFor(index: number, page: PDFPageProxy): Promise<Link[]> {
    const known = this.linkCache.get(index);
    if (known) return known;

    let annotations: { subtype?: string; rect?: number[]; dest?: unknown; url?: string }[];
    try {
      annotations = await page.getAnnotations({ intent: "display" });
    } catch {
      annotations = []; // A document with unreadable annotations still reads fine.
    }

    const view = page.getViewport({ scale: 1 });
    const links: Link[] = [];
    for (const annotation of annotations) {
      if (annotation.subtype !== "Link" || !annotation.rect) continue;
      const url = annotation.url;
      const dest = annotation.dest;
      if (!url && !dest) continue;

      const [x1, y1, x2, y2] = view.convertToViewportRectangle(annotation.rect);
      const width = Math.abs(x2 - x1);
      const height = Math.abs(y2 - y1);
      if (width < 1 || height < 1) continue;

      links.push({
        x: Math.min(x1, x2) / view.width,
        y: Math.min(y1, y2) / view.height,
        width: width / view.width,
        height: height / view.height,
        url,
        dest,
      });
    }
    this.linkCache.set(index, links);
    return links;
  }

  /** Fetch a page's proxy for its links alone, off the render queue. */
  private async attachLinks(slot: Slot): Promise<void> {
    if (slot.linkEl) return;
    const doc = this.doc;
    try {
      const page = await this.page(slot.index);
      if (this.doc !== doc || this.slots.get(slot.index) !== slot) return;
      await this.renderLinks(slot, page);
    } catch {
      // A page that cannot be read has no links to place, and the render path
      // is the one that says so.
    }
  }

  /** The document's own links, laid over the page.
   *
   * Positions are percentages of the page, not pixels, so the layer survives a
   * zoom or a window resize without being rebuilt — the page box changes size
   * and the links move with it. The colour of a link is not this layer's
   * business: it is painted into the page itself, where the ink is. */
  private async renderLinks(slot: Slot, page: PDFPageProxy): Promise<void> {
    // Built once per mounted page. A zoom repaints the canvas, but the links
    // are placed in fractions of the page and are already where they belong.
    if (slot.linkEl) return;
    const doc = this.doc;
    const links = await this.linksFor(slot.index, page);
    if (this.doc !== doc || this.slots.get(slot.index) !== slot) return;
    // Mounting and rendering both ask for the links; whichever gets here first
    // builds them, and the other finds them already up.
    if (slot.linkEl || links.length === 0) return;

    const layer = document.createElement("div");
    layer.className = "link-layer";

    for (const { x, y, width, height, url, dest } of links) {
      const link = document.createElement("a");
      link.style.left = `${x * 100}%`;
      link.style.top = `${y * 100}%`;
      link.style.width = `${width * 100}%`;
      link.style.height = `${height * 100}%`;

      // Deliberately not an `href`.
      //
      // An anchor that carries the address navigates on a middle click, and on
      // a modifier click on some platforms, neither of which goes anywhere
      // near the click handler — so the webview left the app, taking the open
      // document with it, and landed on whatever the PDF pointed at. The
      // address is not needed here: every destination goes out through
      // `onExternalLink`, which is the only thing allowed to decide what
      // opening a link means.
      link.setAttribute("role", "link");
      link.tabIndex = 0;
      const follow = (event: Event) => {
        event.preventDefault();
        if (url) this.callbacks.onExternalLink(url);
        else void this.goToDestination(dest);
      };
      if (url) link.title = url;
      link.addEventListener("click", follow);
      // Middle click and the rest of the buttons, which never fire `click`.
      link.addEventListener("auxclick", follow);
      link.addEventListener("keydown", (event) => {
        if (event.key === "Enter" || event.key === " ") follow(event);
      });
      layer.append(link);
    }

    if (layer.childElementCount === 0) return;
    slot.el.append(layer);
    slot.linkEl = layer;
  }

  /** Follow a link inside the document. A destination names a page and, more
      often than not, a place on it. */
  async goToDestination(dest: unknown): Promise<void> {
    const doc = this.doc;
    if (!doc) return;
    try {
      const explicit = typeof dest === "string" ? await doc.getDestination(dest) : dest;
      if (!Array.isArray(explicit) || explicit.length === 0) return;
      const target = explicit[0];
      const index =
        typeof target === "number" ? target : await doc.getPageIndex(target as never);
      if (this.doc !== doc || index < 0 || index >= this.pageCount) return;
      this.scrollTo(index + 1, await this.offsetWithin(index, explicit));
    } catch {
      this.callbacks.onError("That link does not lead anywhere in this document.");
    }
  }

  /** How far down the page a destination sits, as a fraction of its height.
      `XYZ` and the `Fit*` forms that name a top edge are the ones that say;
      the rest mean the top of the page. */
  private async offsetWithin(index: number, dest: unknown[]): Promise<number> {
    const kind = (dest[1] as { name?: string } | undefined)?.name;
    const top =
      kind === "XYZ" ? dest[3] : kind === "FitH" || kind === "FitBH" ? dest[2] : undefined;
    if (typeof top !== "number") return 0;
    try {
      const page = await this.page(index);
      const view = page.getViewport({ scale: 1 });
      const left = typeof dest[2] === "number" && kind === "XYZ" ? dest[2] : 0;
      const [, y] = view.convertToViewportPoint(left, top);
      return Math.max(0, Math.min(0.95, y / view.height));
    } catch {
      return 0;
    }
  }

  /** Redraw every mounted page, e.g. after a theme change. */
  private repaint(): void {
    for (const slot of this.slots.values()) slot.renderedKey = "";
    this.update();
  }

  /* -------------------------------------------------------------- matches */

  setMatches(matches: Match[], current: number): void {
    this.matches = matches;
    this.currentMatch = current;
    if (this.pendingReveal >= matches.length) this.pendingReveal = -1;
    for (const slot of this.slots.values()) this.paintHighlights(slot);
  }

  /** Whether every match is marked, or only the one being looked at. Off, a
      search for a common word stops turning the page into a striped thing and
      leaves one mark to follow. */
  setHighlightAll(on: boolean): void {
    if (this.highlightAll === on) return;
    this.highlightAll = on;
    for (const slot of this.slots.values()) this.paintHighlights(slot);
  }

  /** Go to a match and put it under the reader's eyes.
   *
   * Landing on the right page is the easy half. The hard half is that a match
   * is a rectangle in the text layer, and the text layer of a page that was
   * not already on screen does not exist yet — it is built after the canvas
   * has been drawn, which is a render away. Scrolling to the top of the page
   * and stopping there is what "it went to the page but I cannot see the
   * word" looks like. So the reveal is remembered, and whichever comes second
   * — this frame or the text layer — finishes the job. */
  revealMatch(index: number): void {
    const match = this.matches[index];
    if (!match) return;
    this.currentMatch = index;
    this.pendingReveal = index;

    if (this.mode === "paged") {
      // One page at a time: the other pages have no place on the page strip at
      // all, so the page has to be turned before anything can be shown.
      if (match.page !== this.current) this.scrollTo(match.page, 0);
    } else {
      const box = this.boxes[match.page - 1];
      if (!box) return;
      // Anywhere in view is close enough to leave alone — a page taller than
      // the window is still "here", and jumping to its top to reach a match
      // further down it would be a scroll in the wrong direction first.
      const seen =
        box.top < this.container.scrollTop + this.container.clientHeight &&
        box.top + box.height > this.container.scrollTop;
      if (!seen) this.scrollTo(match.page, 0);
    }

    this.update();
    requestAnimationFrame(() => this.finishReveal(match.page));
  }

  /** Paint the match and scroll to it, if the page it is on is ready. Called
      from the frame after a reveal and again when a text layer arrives, so
      whichever happens last is the one that lands. */
  private finishReveal(page: number): void {
    if (this.pendingReveal < 0) return;
    if (this.matches[this.pendingReveal]?.page !== page) return;
    const slot = this.slots.get(page - 1);
    if (!slot?.textLayer) return;
    this.pendingReveal = -1;
    this.paintHighlights(slot, true);
  }

  private paintHighlights(slot: Slot, scrollIntoView = false): void {
    slot.highlightEl?.remove();
    slot.highlightEl = null;
    const layer = slot.textLayer;
    if (!layer || this.matches.length === 0) return;

    const divs = layer.textDivs;
    const strings = layer.textContentItemsStr;
    if (divs.length === 0 || divs.length !== strings.length) return;

    const overlay = document.createElement("div");
    overlay.className = "find-layer";
    const pageRect = slot.el.getBoundingClientRect();
    let scrollTarget: HTMLDivElement | null = null;

    for (let i = 0; i < this.matches.length; i++) {
      const match = this.matches[i];
      if (match.page !== slot.index + 1) continue;
      if (!this.highlightAll && i !== this.currentMatch) continue;
      const range = rangeFor(divs, match);
      if (!range) continue;
      for (const rect of range.getClientRects()) {
        const mark = document.createElement("div");
        mark.className = i === this.currentMatch ? "find-highlight current" : "find-highlight";
        mark.style.left = `${rect.left - pageRect.left}px`;
        mark.style.top = `${rect.top - pageRect.top}px`;
        mark.style.width = `${rect.width}px`;
        mark.style.height = `${rect.height}px`;
        overlay.append(mark);
        if (i === this.currentMatch && !scrollTarget) scrollTarget = mark;
      }
    }

    if (overlay.childElementCount === 0) return;
    slot.el.append(overlay);
    slot.highlightEl = overlay;

    if (scrollIntoView && scrollTarget) {
      const box = this.boxes[slot.index];
      const y = box.top + parseFloat(scrollTarget.style.top);
      const view = this.container.scrollTop;
      const height = this.container.clientHeight;
      if (y < view + 60 || y > view + height - 120) {
        this.container.scrollTo({ top: Math.max(0, y - height * 0.32) });
      }
    }
  }
}

/** The colour a link takes on a recoloured page. */
function linkColor(theme: Theme): string {
  return theme.link ?? theme.accent ?? theme.text;
}

/**
 * Colour the links on a page that has just been recoloured.
 *
 * The obvious way — a tinted box over each link, blended into the ink below —
 * is at the mercy of the compositor, and where the blend is dropped the reader
 * gets a solid band across the line instead of a coloured word. So the tint is
 * painted into the bitmap: the untouched page is put back inside the link's
 * rectangle and recoloured again, this time towards the link colour rather
 * than the text colour. The paper maps to the same background either way, so
 * only the letters change, and the edges of the rectangle leave no seam.
 */
function tintLinks(
  ctx: CanvasRenderingContext2D,
  pristine: CanvasImageSource | null,
  width: number,
  height: number,
  links: Link[],
  theme: Theme,
): void {
  const rects: Rect[] = links.map((link) => ({
    x: link.x * width,
    y: link.y * height,
    w: link.width * width,
    h: link.height * height,
  }));

  ctx.save();
  ctx.beginPath();
  for (const rect of rects) ctx.rect(rect.x, rect.y, rect.w, rect.h);
  ctx.clip();
  // On a page that was never recoloured the canvas is already the untouched
  // one, and there is nothing to put back.
  if (pristine) ctx.drawImage(pristine, 0, 0, width, height);
  // On a recoloured page the link's paper has to land on the same background
  // as the rest of the page, or the rectangle shows as a patch. On a page left
  // as it was printed, the paper is the white pdf.js drew it on, and mapping
  // it back to white is what keeps the seam invisible there.
  // The rectangles are handed over as well as clipped: where the engine
  // cannot blend, `recolor` works on pixels, and pixels do not honour a clip.
  recolor(
    ctx,
    width,
    height,
    {
      ...theme,
      text: linkColor(theme),
      background: theme.recolor ? theme.background : "#ffffff",
      recolor: true,
    },
    rects,
  );
  ctx.restore();
}

/** Let go of a canvas's backing store at once.
 *
 * Dropping the reference is enough to make it collectable and not enough to
 * make it collected; resizing it to nothing is what actually frees the surface
 * the compositor is holding. */
function release(canvas: HTMLCanvasElement | null): void {
  if (!canvas) return;
  canvas.width = 0;
  canvas.height = 0;
}

function copyCanvas(source: HTMLCanvasElement): HTMLCanvasElement {
  const copy = document.createElement("canvas");
  copy.width = source.width;
  copy.height = source.height;
  copy.getContext("2d")?.drawImage(source, 0, 0);
  return copy;
}

/** A DOM range covering one match, which may run across several text runs. */
function rangeFor(divs: HTMLElement[], match: Match): Range | null {
  const startNode = divs[match.itemStart]?.firstChild;
  const endNode = divs[match.itemEnd]?.firstChild;
  if (!startNode || !endNode) return null;
  try {
    const range = document.createRange();
    range.setStart(startNode, Math.min(match.offsetStart, startNode.textContent?.length ?? 0));
    range.setEnd(endNode, Math.min(match.offsetEnd, endNode.textContent?.length ?? 0));
    return range;
  } catch {
    return null;
  }
}
