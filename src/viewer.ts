/* The reader itself: layout, rendering and scrolling.
 *
 * Pages are laid out once, in advance, from their real dimensions, and the
 * scroll container is given the full height straight away. Only the pages near
 * the viewport exist in the DOM, so a nine hundred page book costs the same as
 * a two page letter, and the scrollbar tells the truth from the first frame. */

import {
  AnnotationEditorType,
  AnnotationType,
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
  TextItem,
} from "pdfjs-dist/types/src/display/api";
import type { PageViewport } from "pdfjs-dist/types/src/display/display_utils";
// The minified worker, deliberately. Vite copies a `?url` import through
// untouched — it is an asset rather than part of the module graph, so it never
// meets the minifier — and importing the development build shipped a megabyte
// of whitespace and comments that the worker then had to parse at every open.
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

import { openForReading, readRange, type Highlight, type HighlightStyle, type Theme } from "./api";
// The one thing this file borrows from the search: a query and a quote are
// the same kind of question — "these words, however they were typeset" — and
// re-anchoring a highlight into a rebuilt document has to fold its quote
// exactly the way a search folds what the reader typed, or a ligature the new
// run of LaTeX chose differently loses the passage.
import { fold } from "./search";
import {
  duotone,
  luminance,
  markupWashColor,
  parseColor,
  recolor,
  type Rect,
  restoreImages,
  selectionArea,
  selectionInk,
  toHex,
} from "./themes";

GlobalWorkerOptions.workerSrc = workerUrl;

/** An absolute URL for one of pdf.js's runtime data directories.
 *
 * These are handed to the worker, where a relative address would be resolved
 * against the worker script rather than the page — quietly out of reach. The
 * worker then cannot fetch what it needs, and the pages that need it come out
 * missing pieces: scanned documents lose their text, which lives in image
 * masks, and CJK documents lose their glyphs. */
const asset = (path: string): string => new URL(path, window.location.href).href;

/** Whether a render ended because something called it off rather than because
 *  it failed.
 *
 * Exported because the sidebar needs the same question answered and must not
 * import pdf.js to ask it: this file is the only one that reaches for the
 * library rather than its types, which is what keeps swapping the renderer a
 * change to one file. */
export function isRenderCancelled(error: unknown): boolean {
  return error instanceof RenderingCancelledException;
}

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
/** How many pages stand side by side. `cover` is two side by side with the
    first page on its own, which is how a book falls open: page one is a right
    hand page, and pairing it with page two puts every spread out by one. */
export type SpreadMode = "single" | "two" | "cover";

export type Match = {
  page: number;
  itemStart: number;
  offsetStart: number;
  itemEnd: number;
  offsetEnd: number;
  /** Where the match sits in the page's own text, which is what a line of
      context either side of it is cut from. Two numbers rather than the
      context itself: a hundred thousand matches carrying a hundred characters
      each is ten megabytes of strings nobody will read. */
  rawStart: number;
  rawEnd: number;
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
/**
 * …and how many of those may be pages that turned out to carry pictures.
 *
 * The count above is the wrong unit on its own: a page of type costs a few
 * kilobytes of parsed operator list, where a scanned page is one decoded image
 * held until `cleanup()`. So pages with pictures get a cap of their own —
 * which is which is remembered once found, see `holdsPictures`.
 *
 * Three, because `OVERSCAN` keeps three mounted and a mounted page is never
 * evicted: three is the mounted set and no room behind it, which is what the
 * measurement says costs. It charges one page decode when a reader scrolls
 * further back than the screen. On forty pages of scan, three against
 * forty-eight is 263MB against 338MB; on twenty-seven pages of photographs,
 * 231MB against 579MB. Photographs are the case that needs it.
 */
const IMAGE_PAGE_CACHE = 3;
/** How far beyond the viewport pages are kept alive, in viewport heights. */
const OVERSCAN = 0.6;
/** How far a wheel has to push past the end of a page before it turns it.
    Enough that resting against the edge does nothing. */
const WHEEL_TURN = 60;
/** Canvases larger than this are scaled down; browsers refuse to allocate
    beyond roughly this many pixels, and nothing is gained past it anyway. */
const MAX_CANVAS_PIXELS = 12_000_000;

/**
 * Trimming the margins: how many pages decide where the ink begins, how much
 * blank is left around it, and how much of a side may be taken away.
 *
 * One crop for the whole document rather than one per page: a per-page crop
 * changes the scale from page to page, which in continuous scrolling is a
 * document that breathes as you read it. The union over a sample is exact for
 * anything typeset; the pad and the ceiling keep it honest for the rest.
 */
const CROP_SAMPLE = 8;
/** Blank left around the ink, as a fraction of the page. */
const CROP_PAD = 0.012;
/** The most that may be taken off any one side. A page whose margins are
    wider than this is more likely to be a page this has misread. */
const CROP_MAX = 0.3;
/** Below this there is nothing worth trimming, and the answer is to leave the
    page as it is rather than to move it by a hair. */
const CROP_MIN = 0.03;
/** How wide a page is drawn when it is being measured rather than read. */
const CROP_PROBE_WIDTH = 160;
/** Where paper stops and ink begins, on the 0-255 scale `WHITE_POINT` uses
    for the same question. */
const CROP_INK = 235;

/** How many places back a reader can step. Deep enough to walk out of a chain
    of cross-references, shallow enough that it is a history rather than a log. */
const HISTORY_LIMIT = 50;

/** The part of a page worth showing: an origin and a size, both as fractions
    of the whole page. `{x: 0, y: 0, width: 1, height: 1}` is the whole of it,
    which is what `null` means. */
type Crop = { x: number; y: number; width: number; height: number };

/** A place in the document: a page and how far down it the window sat. The
    same pair `position()` reports and `scrollTo` takes. */
type Place = { page: number; offset: number };

type Slot = {
  index: number;
  el: HTMLDivElement;
  canvas: HTMLCanvasElement | null;
  textLayer: TextLayer | null;
  textEl: HTMLDivElement | null;
  highlightEl: HTMLDivElement | null;
  selectionEl: HTMLDivElement | null;
  /** The runs of selected type drawn over this page, by what they were drawn
      from, so a drag redraws only what moved. */
  selectionRuns: Map<string, HTMLCanvasElement>;
  linkEl: HTMLDivElement | null;
  noteEl: HTMLDivElement | null;
  markupEl: HTMLDivElement | null;
  task: RenderTask | null;
  renderedKey: string;
};

/** A selection captured by `Viewer.captureSelection`, opaque to callers
    outside this file: which page each group of rectangles is on, and the
    rectangles themselves. See `captureSelection` for why this is captured
    rather than read again when a colour is finally chosen. */
export type MarkupSelection = [Slot, DOMRect[]][];

/** Markup on one page, as `/QuadPoints` wants it: four points per marked
    line — x then y, line after line — in that page's own PDF coordinate
    space. What `quadsFor` produces from a selection, what `findQuote`
    produces from words, and what `markQuads` writes. */
export type MarkupRun = { page: number; quads: number[] };

/** A run of quads with the colour to draw it in: what `markQuads` writes.
    The colour is per mark rather than per call because restoring markup into
    a rebuilt document puts back whatever colours the reader chose at the
    time, in one write. */
export type MarkupMark = MarkupRun & { color: string; opacity: number };

type Box = {
  top: number;
  left: number;
  width: number;
  height: number;
  scale: number;
  /** The empty space directly above this page — the gap from the row before,
      or the margin at the top of the document. Recorded at layout time
      because it cannot be read back off the boxes once pages sit side by
      side: the box before this one may be its neighbour rather than the row
      above, and share its top exactly. */
  above: number;
};

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

/** One saved highlight, underline, strike-out or squiggly on a page, in
    fractions of the page like a link's rectangle — one entry per run of
    `/QuadPoints`, so a highlight spanning two lines is two of these. This is
    the drawing path's own shape, not the journal's: `Highlight` in `api.ts`
    keeps the raw PDF-space quads for writing back to the file, and this keeps
    only what painting a run needs. */
type MarkupRegion = {
  x: number;
  y: number;
  width: number;
  height: number;
  color: string;
  opacity: number;
  style: HighlightStyle;
  /** The annotation's object id in the file — `null` for markup a document
      carries that this app did not itself write and so cannot rebuild away.
      What `renderMarkupHits` uses to tell a removable run from one that is
      not. */
  annotationId: string | null;
};

/** A note somebody else left in the document: a sticky note, or a comment on
    a highlight. Its rectangle is a fraction of the page, like a link's. */
type Note = {
  x: number;
  y: number;
  width: number;
  height: number;
  /** Small enough to be an icon rather than a passage of text. See
      `renderNotes` for why the difference matters. */
  icon: boolean;
  by: string;
  text: string;
};

export type ViewerCallbacks = {
  onPageChange(page: number, count: number): void;
  onScroll(): void;
  onError(message: string): void;
  /** A link in the document that points somewhere outside it. */
  onExternalLink(url: string): void;
  /** A note in the document, opened by the reader. */
  onNote(note: { by: string; text: string; page: number }): void;
  /** The reader clicked a run of this app's own coloured markup. `anchor` is
      the click target itself, there to position whatever is offered over it. */
  onMarkupClick(page: number, annotationId: string, anchor: HTMLElement): void;
  /** The document is encrypted. Ask for the password, or return null to give
      up; `wrong` is true when the last answer was refused. */
  onPassword(wrong: boolean): Promise<string | null>;
};

/**
 * Reading a document in pieces: pdf.js is given the file's length and a way to
 * ask for parts of it, so it fetches the cross-reference table and then only
 * the pages being looked at. Handing it the whole file meant three copies of
 * every document in memory, and reading all of a 500MB scan before showing any.
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
  /** Pages whose render reported pictures on them. See `IMAGE_PAGE_CACHE`. */
  private pictorial = new Set<number>();
  private linkCache = new Map<number, Link[]>();
  private noteCache = new Map<number, Note[]>();
  private markupCache = new Map<number, MarkupRegion[]>();
  /** A counter for `markSelection`'s own `annotationStorage` keys. See
      `ANNOTATION_EDITOR_PREFIX`. */
  private markupEditorId = 0;
  /** How long the open document is on disk, as of the last `load()` — which
      runs again after every write of this app's own, so this always names the
      length *before* whatever write is about to happen next. `App.lastWrite`
      is built from it, and that is the only reason it is kept: undoing a
      write means truncating the file back to the length it had before that
      write, and this is where "before" comes from. */
  private length = 0;
  /** Whether the document that is open asked for a password on the way in.
      Markup is kept beside such a document rather than written into it —
      see `App.markupStanding` for the reasoning, which is about how little
      this app has tested writing encrypted files rather than about pdf.js
      being unable to. */
  private askedForPassword = false;
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
  private spread: SpreadMode = "single";

  private current = 1;
  private matches: Match[] = [];
  /** The same matches, by the page they are on, each keeping its place in the
      list above. Painting a page's highlights used to walk every match in the
      document, per mounted page, on every flush of a scan that is still
      running — which is what kept the match limit low enough to be reached by
      an ordinary query. */
  private matchPages = new Map<number, { at: number; match: Match }[]>();
  private currentMatch = -1;
  private highlightAll = true;
  /** Bumped whenever the background measuring should stop, so a document put
      down mid-measure does not go on laying out the one after it. */
  private measuring = 0;
  /** Whether anything on a page is selected, and the frame that will repaint
      it. Pages that scroll into view under a live selection need their share
      of it drawn, which is the only reason mounting has to know. */
  private selected = false;
  private selectionFrame = 0;

  /** Where the reader was before each jump, most recent last, and the places a
      `back` has stepped out of. See `jumpTo`. */
  private past: Place[] = [];
  private future: Place[] = [];

  /** What the document calls its own pages, when that is not simply their
      position in the file. See `readLabels`. */
  private labels: string[] | null = null;

  /** Quarter turns clockwise on top of whatever the page says its own
      rotation is. See `rotate`. */
  private rotation = 0;

  /** Whether the reader wants the margins taken off, and where the ink turned
      out to be. Fractions of the page, in the orientation it is being read
      in. See `measureCrop`. */
  private trimming = false;
  private crop: Crop | null = null;
  /** Bumped when a crop measurement should stop mattering. */
  private cropping = 0;

  constructor(
    private container: HTMLElement,
    private pagesEl: HTMLElement,
    private callbacks: ViewerCallbacks,
  ) {
    this.container.addEventListener("scroll", this.onScroll, { passive: true });
    this.container.addEventListener("pointerdown", this.onPanStart);
    document.addEventListener("selectionchange", this.onSelectionChange);
    this.watchDensity();
  }

  private onSelectionChange = (): void => {
    // A drag reports every few pixels; one repaint a frame is enough.
    if (this.selectionFrame) return;
    this.selectionFrame = requestAnimationFrame(() => {
      this.selectionFrame = 0;
      this.paintSelection();
    });
  };

  /**
   * Draw the selected words again, in the theme's selection colours.
   *
   * Giving `::selection` a `color` is the obvious way and it is wrong here: it
   * puts pdf.js's text layer on screen, whose spans exist to be selected and
   * not seen — no weight, no style, a generic family, each stretched
   * horizontally to the total width the printer used. Bold stops being bold, a
   * mathematical symbol becomes a box, and every letter shifts.
   *
   * So the words coloured are the printed ones: the pixels under each
   * rectangle are copied off the page canvas, run through the same luminance
   * ramp that recolours a page, and laid back over the line. The copies are
   * only the lines the reader dragged across, and go with the selection.
   */
  private paintSelection(): void {
    const theme = this.theme;
    const selection = theme ? document.getSelection() : null;
    this.selected = false;

    // Which rectangles, on which page. A selection can run over a page break,
    // and the two pages are separate canvases with separate coordinates.
    const perSlot = new Map<Slot, DOMRect[]>();
    for (let index = 0; index < (selection?.rangeCount ?? 0); index++) {
      const range = selection!.getRangeAt(index);
      // A selection in the settings window is the chrome's business.
      if (range.collapsed || !this.pagesEl.contains(range.commonAncestorContainer)) continue;
      this.selected = true;
      for (const rect of range.getClientRects()) {
        if (rect.width < 0.5 || rect.height < 0.5) continue;
        const slot = this.slotAt(rect);
        if (!slot?.canvas) continue;
        const held = perSlot.get(slot);
        if (held) held.push(rect);
        else perSlot.set(slot, [rect]);
      }
    }

    const painting = theme ? this.selectionPaint(theme) : null;
    for (const slot of this.slots.values()) {
      const rects = perSlot.get(slot);
      if (!rects || !painting) this.clearSelection(slot);
      else this.drawSelection(slot, rects, painting);
    }
  }

  /** The pair of colours the copies are stretched between.
   *
   * Which way round they go depends on the page being read, not on the theme:
   * a recoloured dark page is already light ink on dark paper, so its bright
   * pixels are the words. The ramp always sends black to `text` and white to
   * `background`, so on such a page the two go over swapped. */
  private selectionPaint(theme: Theme): Theme {
    const ink = toHex(selectionInk(theme));
    const area = toHex(selectionArea(theme));
    const lit =
      theme.recolor &&
      luminance(parseColor(theme.text)) > luminance(parseColor(theme.background));
    return { ...theme, recolor: true, text: lit ? area : ink, background: lit ? ink : area };
  }

  /**
   * One page's share of the selection, redrawn from the page itself.
   *
   * A run already on screen at the same place, colours and density is the same
   * picture, so it is kept. That is what makes a drag cheap — only the line
   * that changed is new work. Repainting all of it every frame was thirty-odd
   * milliseconds on a page of a textbook, which visibly lags the pointer.
   */
  private drawSelection(slot: Slot, rects: DOMRect[], painting: Theme): void {
    const canvas = slot.canvas!;
    const pageRect = slot.el.getBoundingClientRect();
    // The canvas is drawn at the screen's density, and at a high zoom below
    // it — one number covers both.
    const density = canvas.width / Math.max(pageRect.width, 1);
    const colours = `${painting.text}|${painting.background}|${density.toFixed(3)}`;

    let overlay = slot.selectionEl;
    if (!overlay) {
      overlay = document.createElement("div");
      overlay.className = "selection-layer";
      slot.el.append(overlay);
      slot.selectionEl = overlay;
    }

    const kept = slot.selectionRuns;
    const next = new Map<string, HTMLCanvasElement>();
    for (const run of joinRuns(rects, pageRect)) {
      const key = `${run.x},${run.y},${run.w},${run.h}|${colours}`;
      const held = next.get(key) ?? kept.get(key);
      if (held) kept.delete(key);
      const copy = held ?? this.drawRun(canvas, run, density, painting);
      if (!copy) continue;
      next.set(key, copy);
      overlay.append(copy);
    }
    for (const stale of kept.values()) {
      release(stale);
      stale.remove();
    }
    slot.selectionRuns = next;
  }

  /** One line of selected type, copied off the page and recoloured. */
  private drawRun(
    canvas: HTMLCanvasElement,
    run: Rect,
    density: number,
    painting: Theme,
  ): HTMLCanvasElement | null {
    const copy = document.createElement("canvas");
    copy.width = Math.max(1, Math.round(run.w * density));
    copy.height = Math.max(1, Math.round(run.h * density));
    const ctx = copy.getContext("2d", { alpha: false });
    if (!ctx) return null;
    ctx.drawImage(
      canvas,
      run.x * density,
      run.y * density,
      copy.width,
      copy.height,
      0,
      0,
      copy.width,
      copy.height,
    );
    duotone(ctx, copy.width, copy.height, painting);
    copy.style.left = `${run.x}px`;
    copy.style.top = `${run.y}px`;
    copy.style.width = `${run.w}px`;
    copy.style.height = `${run.h}px`;
    return copy;
  }

  /** Take away a page's share of the selection, and the bitmaps with it. A
      line of type is a small canvas, but a book selected end to end is not. */
  private clearSelection(slot: Slot): void {
    for (const copy of slot.selectionRuns.values()) release(copy);
    slot.selectionRuns.clear();
    slot.selectionEl?.remove();
    slot.selectionEl = null;
  }

  /** Draw the selection again, if there is one. The rectangles it was drawn
      from are read off the page, so anything that moves a page or repaints one
      leaves them describing where the words used to be. */
  private refreshSelection(): void {
    if (this.selected) this.onSelectionChange();
  }

  /** The mounted page a rectangle is on, by where its middle falls. */
  private slotAt(rect: DOMRect): Slot | null {
    const x = rect.left + rect.width / 2;
    const y = rect.top + rect.height / 2;
    for (const slot of this.slots.values()) {
      const box = slot.el.getBoundingClientRect();
      if (x >= box.left && x <= box.right && y >= box.top && y <= box.bottom) return slot;
    }
    return null;
  }

  /**
   * The current selection's rectangles, grouped by the page each is on —
   * captured now rather than read again later.
   *
   * Clicking anything, the swatch included, collapses the browser's selection
   * before the click handler runs, so `window.getSelection()` inside that
   * handler sees nothing. The popover captures this the moment it opens and
   * hands the same snapshot to `markSelection` however long it stays open.
   *
   * `null` where there is nothing to capture — no selection, or one outside
   * the pages.
   */
  captureSelection(): MarkupSelection | null {
    const selection = window.getSelection();
    if (!selection) return null;

    const bySlot = new Map<Slot, DOMRect[]>();
    for (let index = 0; index < selection.rangeCount; index++) {
      const range = selection.getRangeAt(index);
      if (range.collapsed || !this.pagesEl.contains(range.commonAncestorContainer)) continue;
      for (const rect of range.getClientRects()) {
        if (rect.width < 0.5 || rect.height < 0.5) continue;
        const slot = this.slotAt(rect);
        if (!slot) continue;
        const held = bySlot.get(slot);
        if (held) held.push(rect);
        else bySlot.set(slot, [rect]);
      }
    }
    return bySlot.size > 0 ? [...bySlot.entries()] : null;
  }

  /**
   * Fresh incremental-update bytes for the document, with a captured
   * selection (see `captureSelection`) saved into it as one `/Highlight`
   * annotation per page the selection touches — `/QuadPoints` anchors to a
   * single page, so a selection that runs over a page break becomes two
   * annotations rather than one.
   *
   * This builds by hand the `annotationStorage` entry pdf.js's own highlight
   * editor would build. `HIGHLIGHT` is the *only* markup style this version can
   * create through `saveDocument()`: `saveNewAnnotations`'s switch has cases for
   * `FREETEXT`, `HIGHLIGHT`, `INK`, `STAMP` and `SIGNATURE` alone, so underline,
   * strike-out and squiggly stay readable (`markupOf`) and not writable.
   *
   * The shape below — `quadPoints`, `outlines`, `rect`, the
   * `pdfjs_internal_editor_` prefix the worker looks for — is read out of
   * `pdf.mjs` and `pdf.worker.mjs` rather than any documented API, which is the
   * risk this carries. If a version bump ever produces a file Preview cannot
   * open, that pair is where to look first.
   *
   * Returns `null` rather than throwing where nothing could be saved: "select
   * something first" is the caller's to say. Nothing here touches the disk or
   * the journal, and there is no in-place invalidation because the caller's
   * write reloads the document, which rebuilds every cache.
   */
  async markSelection(
    captured: MarkupSelection,
    color: string,
    opacity: number,
  ): Promise<Uint8Array | null> {
    const runs = await this.quadsFor(captured);
    return this.markQuads(runs.map((run) => ({ ...run, color, opacity })));
  }

  /**
   * A captured selection, restated as `/QuadPoints` — one entry per page the
   * selection touches, each a flat run of eight numbers per line, in that
   * page's own PDF coordinate space.
   *
   * Split out of `markSelection` because the journal wants the same numbers
   * with nothing written: a document that cannot be written at all still keeps
   * its markup beside it, and that entry needs real quads or nothing could put
   * it back into the file later.
   */
  async quadsFor(captured: MarkupSelection): Promise<MarkupRun[]> {
    const runs: MarkupRun[] = [];
    for (const [slot, rects] of captured) {
      const box = this.boxes[slot.index];
      const textRect = slot.textEl?.getBoundingClientRect();
      if (!box || !textRect) continue;
      const page = await this.page(slot.index);
      // The same viewport `renderText` positioned this page's text layer
      // with — scale `box.scale`, no crop offset, because the text layer is
      // always a whole page regardless of the crop. See `placeOverlay`.
      const viewport = this.viewportFor(page, box.scale);

      const quads: number[] = [];
      for (const run of joinRuns(rects, textRect)) {
        const corners = [
          viewport.convertToPdfPoint(run.x, run.y),
          viewport.convertToPdfPoint(run.x + run.w, run.y),
          viewport.convertToPdfPoint(run.x, run.y + run.h),
          viewport.convertToPdfPoint(run.x + run.w, run.y + run.h),
        ];
        const xs = corners.map((p) => p[0] as number);
        const ys = corners.map((p) => p[1] as number);
        quads.push(...quadOf(Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)));
      }
      if (quads.length > 0) runs.push({ page: slot.index + 1, quads });
    }
    return runs;
  }

  /**
   * Fresh incremental-update bytes with one `/Highlight` per run of quads —
   * the half of `markSelection` that touches pdf.js's annotation storage,
   * given the geometry rather than working it out.
   *
   * Three callers now, and the third is why `doc` can be given rather than
   * always being the one on screen: `App.removeHighlight` rebuilds a document
   * from its pristine backup (see `loadDetached`) to drop a highlight
   * `saveDocument()` cannot edit or delete, and every highlight kept from
   * before has to be replayed into *that* copy, not the one mounted here.
   * `App.restoreMarkup` is the second caller, and the reason this is its own
   * method to begin with: it puts markup back into a document that was
   * rebuilt underneath the reader, where the quads come from re-anchoring a
   * quote (`findQuote`) and there is no selection anywhere on screen. Every
   * entry goes into the storage before a single `saveDocument()`, so
   * restoring thirty highlights is one incremental update and one write
   * rather than thirty of each.
   */
  async markQuads(marks: MarkupMark[], doc: PDFDocumentProxy | null = this.doc): Promise<Uint8Array | null> {
    if (!doc || marks.length === 0) return null;

    let wrote = false;
    for (const { page, quads, color, opacity } of marks) {
      const [r, g, b] = parseColor(color);
      if (quads.length === 0) continue;
      const outlines: number[][] = [];
      let minXAll = Infinity;
      let maxXAll = -Infinity;
      let minYAll = Infinity;
      let maxYAll = -Infinity;
      for (let at = 0; at + 8 <= quads.length; at += 8) {
        const xs = [quads[at], quads[at + 2], quads[at + 4], quads[at + 6]];
        const ys = [quads[at + 1], quads[at + 3], quads[at + 5], quads[at + 7]];
        const minX = Math.min(...xs);
        const maxX = Math.max(...xs);
        const minY = Math.min(...ys);
        const maxY = Math.max(...ys);
        // A closed rectangle traversal for the appearance stream's fill —
        // any simple, non-self-intersecting order works under `f*`.
        outlines.push([minX, maxY, maxX, maxY, maxX, minY, minX, minY]);
        minXAll = Math.min(minXAll, minX);
        maxXAll = Math.max(maxXAll, maxX);
        minYAll = Math.min(minYAll, minY);
        maxYAll = Math.max(maxYAll, maxY);
      }
      if (outlines.length === 0) continue;

      const key = `${ANNOTATION_EDITOR_PREFIX}${this.markupEditorId++}`;
      doc.annotationStorage.setValue(key, {
        annotationType: AnnotationEditorType.HIGHLIGHT,
        color: [r, g, b],
        opacity,
        quadPoints: Float32Array.from(quads),
        outlines,
        rect: [minXAll, minYAll, maxXAll, maxYAll],
        rotation: 0,
        pageIndex: page - 1,
        popupRef: "",
      });
      wrote = true;
    }
    return wrote ? doc.saveDocument() : null;
  }

  /** How long the open document is on disk, as of the last `load()`. See the
      field this reads. */
  fileLength(): number {
    return this.length;
  }

  /**
   * Load a document from bytes already in memory, rather than from the disk
   * a window has open — `App.removeHighlight`'s way of getting a fresh,
   * writable copy of the pristine `.hylopdf-original` backup to replay
   * highlights into.
   *
   * Never mounted, never painted, and the caller's to `destroy()` once
   * `markQuads` has produced the rebuilt bytes: nothing here touches
   * `this.doc`, the slots, or any cache the reader is actually looking at,
   * which is what lets a rebuild happen without the page on screen so much
   * as flickering.
   */
  async loadDetached(bytes: Uint8Array): Promise<PDFDocumentProxy> {
    const task = getDocument({
      data: bytes,
      isOffscreenCanvasSupported: false,
      cMapUrl: asset("pdfjs/cmaps/"),
      cMapPacked: true,
      standardFontDataUrl: asset("pdfjs/standard_fonts/"),
      iccUrl: asset("pdfjs/iccs/"),
      wasmUrl: asset("pdfjs/wasm/"),
    });
    return task.promise;
  }

  /**
   * Where a quoted passage sits in the document now — the quads a highlight
   * of it would need, read out of the page's own text.
   *
   * The recompile path, and the one thing the journal exists for that a file
   * cannot do for itself: a paper rebuilt by LaTeX is a new file and every
   * annotation went with it, but the words usually did not. So a lost highlight
   * is offered back by looking its quote up through the same fold `search.ts`
   * matches with — a passage that moved has very often been re-typeset.
   *
   * The search starts at the page the highlight used to be on and works
   * outwards. `null` when nothing carries it: a passage that was rewritten is
   * not a passage that moved, and guessing would put markup on words nobody
   * marked.
   */
  async findQuote(quote: string, near: number): Promise<MarkupRun | null> {
    const doc = this.doc;
    const wanted = fold(quote).text.trim();
    if (!doc || wanted.length === 0) return null;

    for (const number of outwards(near, doc.numPages)) {
      let page: PDFPageProxy;
      try {
        page = await doc.getPage(number);
      } catch {
        continue;
      }
      let items: TextItem[] = [];
      try {
        items = await readTextItems(page);
      } catch {
        // A page with no readable text cannot hold the quote either.
      }
      page.cleanup();
      if (this.doc !== doc) return null;
      const quads = quadsAround(items, wanted);
      if (quads) return { page: number, quads };
    }
    return null;
  }
  /* ------------------------------------------------------------ lifecycle */

  async load(path: string): Promise<PDFDocumentProxy> {
    this.close();
    const length = await openForReading(path);
    this.length = length;
    const task = getDocument({
      range: new FileRange(path, length),
      rangeChunkSize: RANGE_CHUNK,
      // Ask for what is needed and nothing else. Without these two, pdf.js
      // reads the file from one end to the other in the background as well,
      // which is exactly the cost the range transport exists to avoid.
      disableAutoFetch: true,
      disableStream: true,
      // Let the worker hand over image *data* rather than ready-made bitmaps.
      //
      // On — pdf.js's default in a browser — the worker expands every image to
      // RGBA and transfers an `ImageBitmap`, which the page proxy holds until
      // `cleanup()` at four bytes a pixel in the GPU process whatever the image
      // was: a bitonal 3600×4400 scan page arrives as sixty megabytes of a
      // picture that is one bit per pixel on disk.
      //
      // Off, the worker sends the decoded data as it stands and the main thread
      // builds the mask canvas per render, freed when the render ends. Forty
      // pages of bitonal scan: 630MB to 263MB, flat. Twenty-seven pages of
      // photographs: 348MB to 231MB. A single 12000×16000 page: 2489MB to
      // 248MB.
      //
      // Not a trade against speed — the page is quicker to draw, what crosses
      // out of the worker being compressed rather than expanded (92ms to 71ms
      // on the scan). It is a trade against *where* the work happens: the
      // expansion is on the main thread, so an image-heavy page costs one frame
      // of about 60ms that used to be the worker's. The pixels are identical,
      // at an RMSE of zero under both themes.
      //
      // What it gives up is `ImageResizer`, which runs only on this path and
      // shrinks an image the browser could not make a canvas for. The
      // 12000×16000 case is better without it.
      isOffscreenCanvasSupported: false,
      cMapUrl: asset("pdfjs/cmaps/"),
      cMapPacked: true,
      standardFontDataUrl: asset("pdfjs/standard_fonts/"),
      iccUrl: asset("pdfjs/iccs/"),
      wasmUrl: asset("pdfjs/wasm/"),
    });
    this.loading = task;
    this.askedForPassword = false;
    // The rejection that a decline produces travels back through the worker
    // and comes out the other side as something else entirely, so whether the
    // reader declined is remembered here rather than read off the error.
    let declined = false;

    // An encrypted document asks rather than fails. Left to itself this comes
    // back as a rejected promise indistinguishable from a corrupt file, and
    // "Something went wrong" is the wrong thing to tell someone whose PDF is
    // merely locked.
    task.onPassword = (respond: (password: string | Error) => void, reason: number) => {
      this.askedForPassword = true;
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

    // Measure the first page and paint; the rest are measured behind the
    // reader's back. A page proxy is cheap next to a render, which is true of
    // one page and not of two thousand — measuring all of them first left
    // nothing on screen until the last came back. Most documents are one size
    // throughout, and where the guess is wrong the correction arrives within a
    // second, on pages the reader has not reached.
    this.readLabels(doc);

    const first = await this.page(0);
    if (this.doc !== doc) return doc;
    const view = first.getViewport({ scale: 1 });
    const estimate = { width: view.width, height: view.height };
    this.sizes = new Array(doc.numPages).fill(estimate);
    this.relayout();

    void this.measureRest(doc, estimate);
    // Where the ink on *this* document is. The setting outlives documents and
    // the answer does not, so this is where it is asked, rather than in
    // `setTrimMargins` — which is called once at startup, before there is a
    // document to measure.
    if (this.trimming) void this.measureCrop();
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
    this.noteCache.clear();
    this.markupCache.clear();
    this.pendingReveal = -1;
    this.matches = [];
    this.matchPages.clear();
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
    this.past = [];
    this.future = [];
    this.labels = null;
    this.rotation = 0;
    this.crop = null;
    this.cropping++;
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

  /** Drop the least recently wanted proxies, never one that is on screen.
   *
   * Two caps rather than one: everything is held to `PAGE_CACHE`, and the
   * pages carrying pictures are held to `IMAGE_PAGE_CACHE` on top of that,
   * because those are the ones that are measured in tens of megabytes each. A
   * book of type is unaffected — it never reaches the second cap — and a
   * scanned one stops at half a gigabyte instead of three. */
  private trimPages(): void {
    let pictures = 0;
    for (const index of this.pageCache.keys()) {
      if (this.pictorial.has(index)) pictures++;
    }
    if (this.pageCache.size <= PAGE_CACHE && pictures <= IMAGE_PAGE_CACHE) return;

    // Oldest first: `page()` re-inserts on every hit, so this is the LRU order.
    for (const index of [...this.pageCache.keys()]) {
      if (this.pageCache.size <= PAGE_CACHE && pictures <= IMAGE_PAGE_CACHE) break;
      // Never one that is on screen, whichever cap is over.
      if (this.slots.has(index)) continue;
      const picture = this.pictorial.has(index);
      // Under the count cap, only pictures are worth evicting.
      if (this.pageCache.size <= PAGE_CACHE && !picture) continue;
      const pending = this.pageCache.get(index)!;
      this.pageCache.delete(index);
      if (picture) pictures--;
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
    this.pictorial.clear();
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

  /** Whether the document that is open needed a password to be read at all.
      One of the things that decides where markup on it goes — see
      `App.markupStanding`. */
  get encrypted(): boolean {
    return this.askedForPassword;
  }

  get isEmpty(): boolean {
    return this.doc === null;
  }

  /** Whether a page (1-based) is currently mounted on screen. A page's proxy
      must never be cleaned up out from under a slot drawing it — this is what
      lets callers outside the viewer, like the sidebar, obey the same rule
      `trimPages` does for its own cache. */
  isMounted(page: number): boolean {
    return this.slots.has(page - 1);
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

  /** Take a theme, and repaint every page if it is not the one already in use.
   *
   * The copy is the point: the theme editor previews by handing over the draft
   * and then editing it in place, so holding the object meant holding the new
   * colours under the name of the old — every comparison found the two sides
   * equal and nothing repainted.
   *
   * Two questions, not one. The pages repaint when something baked into a
   * bitmap moves; the selection is drawn from the page every time and repaints
   * on its own terms. Asking only the first left the selection colours off the
   * list entirely. */
  setTheme(theme: Theme, preserveImages: boolean): void {
    const before = this.theme;
    const repaint =
      before?.id !== theme.id ||
      before?.text !== theme.text ||
      before?.background !== theme.background ||
      before?.recolor !== theme.recolor ||
      before?.link !== theme.link ||
      before?.accent !== theme.accent ||
      this.preserveImages !== preserveImages;
    const reselect =
      repaint ||
      before?.selection_area !== theme.selection_area ||
      before?.selection_text !== theme.selection_text;
    this.theme = { ...theme };
    this.preserveImages = preserveImages;
    if (repaint) this.repaint();
    // A repaint redraws the selection when the new pixels land, so this is
    // only for the case where nothing else changed.
    else if (reselect) this.refreshSelection();
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

  /** One page across, or two. */
  setSpread(spread: SpreadMode): void {
    if (spread === this.spread) return;
    this.spread = spread;
    this.relayout();
  }

  /**
   * The pages that stand together, in order.
   *
   * One each in single mode. In `cover`, page one is alone and every pair
   * after it is (even, odd) — which is how a book falls open, page one being a
   * right-hand page. In `two` the pairs start from the first page, which is
   * what a document of slides or a scan of two-up photocopies wants.
   */
  private rows(): number[][] {
    const count = this.sizes.length;
    if (this.spread === "single") return this.sizes.map((_, index) => [index]);
    const rows: number[][] = [];
    let index = 0;
    if (this.spread === "cover" && count > 0) {
      rows.push([0]);
      index = 1;
    }
    for (; index < count; index += 2) {
      rows.push(index + 1 < count ? [index, index + 1] : [index]);
    }
    return rows;
  }

  /** The row a page is standing in, and the pages standing with it. */
  private rowOf(index: number): number[] {
    if (this.spread === "single") return [index];
    if (this.spread === "cover") {
      if (index === 0) return [0];
      const first = index % 2 === 1 ? index : index - 1;
      return first + 1 < this.sizes.length ? [first, first + 1] : [first];
    }
    const first = index - (index % 2);
    return first + 1 < this.sizes.length ? [first, first + 1] : [first];
  }

  /** Continuous or one page at a time.
   *
   * The wheel listener comes and goes with the mode, which is the point of
   * doing it here: it has to be non-passive — turning a page means stopping the
   * rubber-band against an edge it is about to leave — and a non-passive wheel
   * listener makes the browser wait for the main thread before it will scroll
   * at all. Left attached, continuous scrolling pays for a gesture it does not
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
   * The density is part of what identifies a rendered page, so a canvas drawn
   * for a Retina screen is wrong on the 1× monitor beside it. Nothing announces
   * this: `matchMedia` on the current resolution fires once when it stops being
   * current, and has to be re-armed for the new one. */
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

    const scaleFor = (
      size: { width: number; height: number },
      room = availableWidth,
    ): number => {
      switch (this.fit) {
        case "width":
          return room / size.width;
        case "page":
          return Math.min(room / size.width, availableHeight / size.height);
        default:
          return PixelsPerInch.PDF_TO_CSS_UNITS * this.zoomFactor;
      }
    };

    // A row is what has to fit: one page, or two side by side with the gap
    // between them. Everything below works in rows, and a document without
    // spreads is a document of rows of one.
    const rows = this.mode === "paged" ? [this.rowOf(this.current - 1)] : this.rows();
    const boxes: Box[] = new Array(this.sizes.length);

    // The gap between two pages of a spread is a distance on the screen, like
    // the gap between rows — it is not part of the page and does not grow
    // with the zoom. So it comes off the room available before the scale is
    // worked out, rather than being scaled along with the paper.
    const gapsIn = (row: number[]) => (row.length - 1) * this.gap;
    const paperWidth = (row: number[]) =>
      row.reduce((sum, index) => sum + this.sizeOf(index).width, 0);
    const rowHeight = (row: number[]) =>
      row.reduce((tallest, index) => Math.max(tallest, this.sizeOf(index).height), 0);
    const scaleForRow = (row: number[]) =>
      scaleFor(
        { width: paperWidth(row), height: rowHeight(row) },
        Math.max(availableWidth - gapsIn(row), 120),
      );
    const rowSpan = (row: number[], scale: number) =>
      row.reduce((sum, index) => sum + Math.round(this.sizeOf(index).width * scale), 0) +
      gapsIn(row);

    let width = 0;
    for (const row of rows) {
      width = Math.max(width, rowSpan(row, scaleForRow(row)));
    }
    this.contentWidth = Math.max(width, availableWidth) + padX * 2;

    let top = PAD_Y;
    let above = PAD_Y;
    for (const row of rows) {
      const scale = scaleForRow(row);
      const across = rowSpan(row, scale);
      let left = Math.round((this.contentWidth - across) / 2);
      let tallest = 0;
      for (const index of row) {
        const size = this.sizeOf(index);
        const pageWidth = Math.round(size.width * scale);
        const pageHeight = Math.round(size.height * scale);
        boxes[index] = { top, left, width: pageWidth, height: pageHeight, scale, above };
        left += pageWidth + this.gap;
        tallest = Math.max(tallest, pageHeight);
      }
      top += tallest + this.gap;
      above = this.gap;
    }
    this.boxes = boxes;

    this.pagesEl.style.width = `${this.contentWidth}px`;
    this.pagesEl.style.height = `${Math.max(top - this.gap + PAD_Y, 0)}px`;

    // Every mounted page moved; re-place and re-render them where they landed.
    for (const slot of this.slots.values()) this.place(slot);
    if (held) this.restorePoint(held);
    else this.scrollTo(anchor.page, anchor.offset);
    this.update();
    this.refreshSelection();
  }

  /** The spot in the document under a point on the screen, described so that
      it survives a change of scale: which page, and how far across and down
      it — plus where on the screen it was, so it can be put back there. */
  private pointAt(focus: { x: number; y: number }): Point | null {
    const view = this.container.getBoundingClientRect();
    const docY = this.container.scrollTop + (focus.y - view.top);
    const docX = this.container.scrollLeft + (focus.x - view.left);
    let index = this.mode === "paged" ? this.current - 1 : this.lastBoxStartingAbove(docY);
    // Which of a spread's two pages the pointer is actually over. The search
    // above is by row, and a zoom that keeps the wrong page under the fingers
    // is worse than one that keeps none.
    const beside = this.boxes[index - 1];
    if (beside && this.boxes[index] && beside.top === this.boxes[index].top && docX < this.boxes[index].left) {
      index -= 1;
    }
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
    if (slot.textEl) this.placeOverlay(slot.textEl, slot.index, box.scale);
    if (slot.linkEl) this.placeOverlay(slot.linkEl, slot.index, box.scale);
    if (slot.noteEl) this.placeOverlay(slot.noteEl, slot.index, box.scale);
    if (slot.markupEl) this.placeOverlay(slot.markupEl, slot.index, box.scale);
  }

  /**
   * Put a layer measured in whole pages over a box that is not.
   *
   * The canvas is the cropped part of the page and fills its box; the text
   * layer and the links are not, pdf.js laying its spans out as percentages of
   * the page. So both are the size the whole page would be and hang out past
   * the box, which `.page` clips. With no crop this is the box exactly.
   */
  private placeOverlay(el: HTMLElement, index: number, scale: number): void {
    const crop = this.crop;
    if (!crop) {
      // Back to the stylesheet's `inset: 0`, and to pdf.js's own sizing of the
      // text layer, which rounds against the device pixel grid rather than
      // simply multiplying.
      el.style.inset = "";
      el.style.left = "";
      el.style.top = "";
      el.style.width = "";
      el.style.height = "";
      return;
    }
    const whole = this.wholeSizeOf(index);
    el.style.inset = "auto";
    el.style.left = `${-crop.x * whole.width * scale}px`;
    el.style.top = `${-crop.y * whole.height * scale}px`;
    el.style.width = `${whole.width * scale}px`;
    el.style.height = `${whole.height * scale}px`;
  }

  /* ------------------------------------------------------------ scrolling */

  private onScroll = (): void => {
    this.callbacks.onScroll();
    this.update();
  };

  /** Turning the page with the wheel, one page at a time.
   *
   * In paged mode the container holds exactly one page, so a page that fits
   * cannot be scrolled and a taller one stops dead at its bottom edge — either
   * way the reader pushes and nothing happens, which is the gesture everybody
   * tries first. Past the edge, the scroll turns the page.
   *
   * One page per gesture: a trackpad flick sends events for about a second
   * after the fingers have gone, and a page each would be a flick through the
   * chapter. A gap in the events counts as letting go. */
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

    let mounted = false;
    for (const index of wanted) {
      if (this.slots.has(index)) continue;
      mounted = true;
      const slot = this.createSlot(index);
      this.slots.set(index, slot);
      // Links do not wait for paint. They are placed in fractions of the page,
      // so they are right the moment the page has a size — and asking for them
      // here rather than at the end of a render means a link answers a click
      // as soon as it is on screen, instead of a second later when the queue
      // has worked its way round to it.
      void this.attachLinks(slot);
    }

    // A page that has just come into view under a live selection has its own
    // share of it to draw.
    if (mounted) this.refreshSelection();

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
    // The row, and then the left-hand page of it: two pages standing side by
    // side share a top, so the search finds the right-hand one, and a reader
    // looking at a spread is on the page it opens at.
    const page = this.rowOf(this.lastBoxStartingAbove(probe))[0] + 1;
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
    // window and the page follows: the gap between two rows, or the margin
    // above the first. That distance is recorded on the box at layout time —
    // it used to be read back off the page before, which is right until two
    // pages stand side by side and the box before this one is its neighbour
    // rather than the row above.
    const target = box.top + offset * box.height - (offset === 0 ? box.above : 0);
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    this.container.scrollTo({
      top: Math.max(0, target),
      behavior: smooth && !reduced ? "smooth" : "auto",
    });
    this.current = index + 1;
    this.callbacks.onPageChange(this.current, this.pageCount);
    this.update();
  }

  goToPage(page: number): void {
    this.jumpTo(page, 0);
  }

  /* --------------------------------------------------------------- crop */

  /**
   * Take the margins off, or put them back.
   *
   * A scanned book and a LaTeX paper both spend a quarter of the window on
   * white paper, and fit width fits the paper rather than the words. Sumatra
   * calls it fit content; Zathura and Sioyek call it cropping.
   */
  setTrimMargins(on: boolean): void {
    if (on === this.trimming) return;
    this.trimming = on;
    if (!on) {
      this.cropping++;
      this.crop = null;
      this.relayout();
      return;
    }
    void this.measureCrop();
  }

  get trimsMargins(): boolean {
    return this.trimming;
  }

  /** Whether anything was actually found to trim. A document with no margins
      to speak of leaves the switch on and the page alone, and this is how the
      interface can say so. */
  get trimmed(): boolean {
    return this.crop !== null;
  }

  /**
   * Find the ink.
   *
   * A sample rather than the whole document, because measuring a page means
   * drawing it. First, last and evenly spaced between: the shapes that vary are
   * the front matter, the plates and the index.
   *
   * The union of what the sample finds, padded, and refused outright if what is
   * left is nearly the whole page (nothing to trim) or a sliver of it (a blank
   * page, or one that failed to draw).
   */
  private async measureCrop(): Promise<void> {
    const doc = this.doc;
    if (!doc) return;
    const run = ++this.cropping;

    const count = doc.numPages;
    const pages: number[] = [];
    const step = Math.max(1, Math.floor((count - 1) / Math.max(1, CROP_SAMPLE - 1)));
    for (let index = 0; index < count && pages.length < CROP_SAMPLE; index += step) {
      pages.push(index);
    }
    if (!pages.includes(count - 1)) pages.push(count - 1);

    let left = 1;
    let top = 1;
    let right = 0;
    let bottom = 0;
    let found = false;
    for (const index of pages) {
      const ink = await this.inkBox(index);
      if (this.doc !== doc || run !== this.cropping) return;
      if (!ink) continue;
      found = true;
      left = Math.min(left, ink.x);
      top = Math.min(top, ink.y);
      right = Math.max(right, ink.x + ink.width);
      bottom = Math.max(bottom, ink.y + ink.height);
    }
    if (!found) return;

    const crop = {
      x: Math.max(0, left - CROP_PAD),
      y: Math.max(0, top - CROP_PAD),
      width: 0,
      height: 0,
    };
    crop.width = Math.min(1, right + CROP_PAD) - crop.x;
    crop.height = Math.min(1, bottom + CROP_PAD) - crop.y;

    // Never take more than a share of any side: a page whose margins measure
    // wider than that is more likely to be a page this has misread, and the
    // cost of being wrong is a reader who cannot see the top line.
    crop.x = Math.min(crop.x, CROP_MAX);
    crop.y = Math.min(crop.y, CROP_MAX);
    crop.width = Math.max(crop.width, 1 - CROP_MAX - crop.x);
    crop.height = Math.max(crop.height, 1 - CROP_MAX - crop.y);
    crop.width = Math.min(crop.width, 1 - crop.x);
    crop.height = Math.min(crop.height, 1 - crop.y);

    // Nothing worth doing, either because the page has no margins or because
    // what came back is too small to be a page of anything.
    const trimmed = 1 - crop.width * crop.height;
    if (trimmed < CROP_MIN || crop.width < 0.3 || crop.height < 0.3) return;

    this.crop = crop;
    this.relayout();
  }

  /**
   * Where the ink is on one page, as fractions of it.
   *
   * Drawn small — a hundred and sixty pixels wide, a millisecond or two, and
   * enough to find a margin to within a character — then read row by row for
   * anything that is not paper. `CROP_INK` is `WHITE_POINT`'s threshold, so a
   * hairline printed at 90% white is paper here as it is when recolouring.
   */
  private async inkBox(index: number): Promise<Crop | null> {
    let page: PDFPageProxy | null = null;
    try {
      page = await this.page(index);
      const full = this.viewportFor(page, 1);
      const scale = CROP_PROBE_WIDTH / full.width;
      const viewport = this.viewportFor(page, scale);
      const width = Math.max(1, Math.floor(viewport.width));
      const height = Math.max(1, Math.floor(viewport.height));
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const ctx = canvas.getContext("2d", { alpha: false, willReadFrequently: true });
      if (!ctx) return null;
      // White, not the theme's paper: this is a question about the document,
      // and the answer must not move when the reader changes theme.
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(0, 0, width, height);
      await page.render({ canvas, canvasContext: ctx, viewport, background: "#ffffff" }).promise;

      const pixels = ctx.getImageData(0, 0, width, height).data;
      let left = width;
      let top = height;
      let right = -1;
      let bottom = -1;
      for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
          const at = (y * width + x) * 4;
          // The green channel stands in for lightness. It is most of luma,
          // and this is a threshold rather than a measurement.
          if (pixels[at + 1] > CROP_INK && pixels[at] > CROP_INK && pixels[at + 2] > CROP_INK) {
            continue;
          }
          if (x < left) left = x;
          if (x > right) right = x;
          if (y < top) top = y;
          if (y > bottom) bottom = y;
        }
      }
      release(canvas);
      if (right < left || bottom < top) return null; // A blank page says nothing.
      return {
        x: left / width,
        y: top / height,
        width: (right + 1 - left) / width,
        height: (bottom + 1 - top) / height,
      };
    } catch {
      return null;
    } finally {
      if (page && !this.isMounted(index + 1)) page.cleanup();
    }
  }

  /* ------------------------------------------------------------ rotation */

  /**
   * Turn the document a quarter at a time.
   *
   * A way of looking rather than a property of the file, so it is not written
   * down and does not survive the document being closed — which is what
   * Preview, Acrobat and Sumatra do.
   *
   * Added to the page's own rotation, because a page that says it is printed
   * sideways has already been turned once.
   */
  rotate(quarterTurns: number): void {
    const before = this.rotation;
    this.rotation = (((before + quarterTurns * 90) % 360) + 360) % 360;
    if (this.rotation === before) return;
    // The crop is a rectangle on the page as the reader sees it, so it turns
    // with the page: a quarter clockwise takes (x, y, w, h) to
    // (1 − y − h, x, h, w). Turning it is exact and free; measuring it again
    // would be eight renders for an answer already in hand.
    let turns = ((quarterTurns % 4) + 4) % 4;
    while (this.crop && turns-- > 0) {
      const { x, y, width, height } = this.crop;
      this.crop = { x: 1 - y - height, y: x, width: height, height: width };
    }
    // Where a link, a note or a highlight is on the page is a fraction of a
    // page that has just changed shape.
    this.linkCache.clear();
    this.noteCache.clear();
    this.markupCache.clear();
    this.relayout();
  }

  /** Quarter turns clockwise the reader has asked for, in degrees. */
  get turned(): number {
    return this.rotation;
  }

  /** How big a page is once it has been turned: the same numbers, and on a
      quarter turn the other way round. */
  private sizeOf(index: number): { width: number; height: number } {
    const size = this.sizes[index];
    if (!size) return { width: 1, height: 1 };
    const turned =
      this.rotation % 180 === 0 ? size : { width: size.height, height: size.width };
    const crop = this.crop;
    if (!crop) return turned;
    return { width: turned.width * crop.width, height: turned.height * crop.height };
  }

  /** The whole page, turned but not cropped — what the text layer and the
      links are still measured in, because their coordinates are fractions of
      a whole page whatever is being shown of it. */
  private wholeSizeOf(index: number): { width: number; height: number } {
    const size = this.sizes[index] ?? { width: 1, height: 1 };
    return this.rotation % 180 === 0 ? size : { width: size.height, height: size.width };
  }

  /** The viewport a page is drawn, measured and laid out through — the one
      place the reader's rotation is added to the page's own. */
  private viewportFor(
    page: PDFPageProxy,
    scale: number,
    offsets?: { offsetX: number; offsetY: number },
  ) {
    return page.getViewport({
      scale,
      rotation: page.rotate + this.rotation,
      ...offsets,
    });
  }

  /* ----------------------------------------------------------- the text */

  /** Whether any page on screen has text on it to select.
   *
   * The honest answer to "is this a scan" for a gesture that works off a
   * selection. The search knows for the whole document, but only once it has
   * scanned it — and a reader who has never searched should still be told there
   * is nothing here to mark rather than to select something first. */
  hasSelectableText(): boolean {
    for (const slot of this.slots.values()) {
      if (slot.textEl && slot.textEl.childElementCount > 0) return true;
    }
    return false;
  }

  /**
   * Select everything on one page.
   *
   * ⌘A has nothing good to select here: only the pages near the window are in
   * the DOM, so "everything" is a page and a half plus the contents panel and
   * the names in a menu, and ⌘C after it gets a fragment nobody asked for.
   *
   * A page is a unit somebody means, and the largest this app can honestly
   * offer — the page below is not there to be selected until it is mounted.
   */
  selectPage(page: number): boolean {
    const slot = this.slots.get(page - 1);
    if (!slot?.textEl || slot.textEl.childElementCount === 0) return false;
    const range = document.createRange();
    range.selectNodeContents(slot.textEl);
    const selection = window.getSelection();
    if (!selection) return false;
    selection.removeAllRanges();
    selection.addRange(range);
    return true;
  }

  /* ------------------------------------------------------------ metadata */

  /**
   * What the document says about itself: the fields somebody typing "get info"
   * is asking for, plus the ones only the file knows.
   *
   * Read on demand rather than at open time. It is one trip into the worker
   * and nothing needs it until it is asked for.
   */
  async details(): Promise<{ info: Record<string, unknown>; pages: number; size: string }> {
    const doc = this.doc;
    if (!doc) return { info: {}, pages: 0, size: "" };
    let info: Record<string, unknown> = {};
    try {
      const meta = await doc.getMetadata();
      info = (meta?.info ?? {}) as Record<string, unknown>;
    } catch {
      // A document whose metadata will not parse still has a page count.
    }
    const page = await this.page(0);
    const view = page.getViewport({ scale: 1 });
    // In millimetres and in inches, because a reader knows their paper in one
    // or the other and never in points.
    const mm = (points: number) => Math.round((points * 25.4) / 72);
    const inches = (points: number) => (points / 72).toFixed(2);
    const size = `${mm(view.width)} × ${mm(view.height)} mm (${inches(view.width)} × ${inches(
      view.height,
    )} in)`;
    return { info, pages: doc.numPages, size };
  }

  /* -------------------------------------------------------------- labels */

  /**
   * What the document calls its own pages.
   *
   * A book's front matter is numbered i, ii, iii and its body starts again at
   * 1, so page 314 of the index is not the 314th thing in the file. A reader
   * typing a number off a citation means the printed one.
   *
   * Asked for without waiting: the answer is in the catalogue and usually
   * arrives before the first page is drawn. The toolbar is told again when it
   * lands.
   *
   * Labels that merely restate the position are dropped — a document numbering
   * its pages 1 to n has said nothing, and every lookup below would run for no
   * reason.
   */
  private readLabels(doc: PDFDocumentProxy): void {
    void doc
      .getPageLabels()
      .then((labels) => {
        if (this.doc !== doc || !labels) return;
        if (labels.every((label, index) => label === String(index + 1))) return;
        this.labels = labels;
        this.callbacks.onPageChange(this.current, this.pageCount);
      })
      .catch(() => {});
  }

  /** Whether this document numbers its pages its own way. */
  get hasLabels(): boolean {
    return this.labels !== null;
  }

  /** What to call a page (1-based) when showing it to a reader. */
  label(page: number): string {
    const label = this.labels?.[page - 1];
    return label && label.length > 0 ? label : String(page);
  }

  /**
   * The page a reader means by what they typed.
   *
   * A label first, because that is what is printed on the page and what an
   * index cites; the position in the file second, so that "page 7" still
   * finds something in a document whose seventh page is called "vii" — and
   * because there is otherwise no way at all to reach a page whose label is
   * blank.
   */
  pageForLabel(text: string): number | null {
    const wanted = text.trim();
    if (wanted.length === 0) return null;
    if (this.labels) {
      const folded = wanted.toLowerCase();
      const index = this.labels.findIndex((label) => label.toLowerCase() === folded);
      if (index >= 0) return index + 1;
    }
    const number = Number.parseInt(wanted, 10);
    if (Number.isFinite(number) && number >= 1 && number <= this.pageCount) return number;
    return null;
  }

  /* --------------------------------------------------------------- pan */

  /**
   * Drag the page around with the middle button.
   *
   * Zoomed in past the window there is no way sideways but the scrollbar, and
   * the left button belongs to selecting text. The middle button is free, is
   * what every map uses for this, and needs no mode entered first.
   */
  private panning: { id: number; x: number; y: number; left: number; top: number } | null = null;
  /** Set by a drag that actually moved, and read by the link layer: a middle
      click is how a link is opened in a browser, and the mouseup at the end of
      a pan lands on whatever the pointer finished over. */
  private panned = false;

  private onPanStart = (event: PointerEvent): void => {
    if (event.button !== 1) return;
    event.preventDefault();
    this.panning = {
      id: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      left: this.container.scrollLeft,
      top: this.container.scrollTop,
    };
    this.panned = false;
    this.container.setPointerCapture(event.pointerId);
    this.container.classList.add("panning");
    this.container.addEventListener("pointermove", this.onPanMove);
    this.container.addEventListener("pointerup", this.onPanEnd);
    this.container.addEventListener("pointercancel", this.onPanEnd);
  };

  private onPanMove = (event: PointerEvent): void => {
    const from = this.panning;
    if (!from || event.pointerId !== from.id) return;
    const dx = event.clientX - from.x;
    const dy = event.clientY - from.y;
    if (Math.abs(dx) > 3 || Math.abs(dy) > 3) this.panned = true;
    this.container.scrollTo({ left: from.left - dx, top: from.top - dy, behavior: "auto" });
  };

  private onPanEnd = (event: PointerEvent): void => {
    if (!this.panning || event.pointerId !== this.panning.id) return;
    this.container.releasePointerCapture(this.panning.id);
    this.panning = null;
    this.container.classList.remove("panning");
    this.container.removeEventListener("pointermove", this.onPanMove);
    this.container.removeEventListener("pointerup", this.onPanEnd);
    this.container.removeEventListener("pointercancel", this.onPanEnd);
  };

  /* ------------------------------------------------------------- history */

  /**
   * Go somewhere the reader asked to go, remembering where they were.
   *
   * Moving *through* a document — scrolling, turning a page, stepping through
   * matches — leaves no trace: a history of those would be a history of the
   * last twenty keystrokes. Jumping *across* it does: a cross-reference, a
   * chapter, a typed page number. The citation on page 12 that lands on page
   * 190 is the reason this exists.
   *
   * A jump made after stepping back throws away what was ahead, which is what
   * every back button does.
   */
  jumpTo(page: number, offset = 0): void {
    const from = this.position();
    const to = Math.max(1, Math.min(page, this.pageCount));
    // A jump that lands where we already are is not a jump. Without this,
    // pressing Home twice files the first page away as somewhere worth
    // returning to, and Escape from the page field — which re-runs the jump
    // with the number that was already there — fills the history with copies
    // of one place.
    if (to === from.page && Math.abs(offset - from.offset) < 0.01) {
      this.scrollTo(to, offset);
      return;
    }
    this.past.push(from);
    if (this.past.length > HISTORY_LIMIT) this.past.shift();
    this.future = [];
    this.scrollTo(to, offset);
  }

  get canGoBack(): boolean {
    return this.past.length > 0;
  }

  get canGoForward(): boolean {
    return this.future.length > 0;
  }

  /** Back to where the last jump started. Returns false when there is nowhere
      to go, so the caller can say so rather than doing nothing visible. */
  goBack(): boolean {
    const place = this.past.pop();
    if (!place) return false;
    this.future.push(this.position());
    this.scrollTo(place.page, place.offset);
    return true;
  }

  goForward(): boolean {
    const place = this.future.pop();
    if (!place) return false;
    this.past.push(this.position());
    this.scrollTo(place.page, place.offset);
    return true;
  }

  /** The next page, or the next pair of them: turning one page of a spread
      and leaving the other where it is turns nothing. */
  nextPage(): void {
    const row = this.rowOf(this.current - 1);
    this.scrollTo(Math.min(row[row.length - 1] + 2, this.pageCount), 0);
  }

  previousPage(): void {
    const row = this.rowOf(this.current - 1);
    const before = this.rowOf(Math.max(row[0] - 1, 0));
    this.scrollTo(before[0] + 1, 0);
  }

  /** A nudge: the same distance whichever way it goes, and a proportion of the
      window so it feels the same in a small one as in a large one. */
  scrollByStep(direction: 1 | -1): void {
    const step = Math.max(60, Math.round(this.container.clientHeight * 0.12));
    this.container.scrollBy({ top: direction * step, behavior: "auto" });
  }

  /** A screen, or a fraction of one. The 60px is the overlap that keeps a
      line of what was just read on screen after the jump, and it comes off
      the screen before the fraction does: half a screen means half of what a
      whole one would have moved, not half a screen plus the whole overlap. */
  scrollByViewport(direction: 1 | -1, fraction = 1): void {
    if (this.mode === "paged") {
      const room = this.container.scrollHeight - this.container.clientHeight;
      const at = this.container.scrollTop;
      if ((direction === 1 && at >= room - 2) || (direction === -1 && at <= 2)) {
        direction === 1 ? this.nextPage() : this.previousPage();
        return;
      }
    }
    this.container.scrollBy({
      top: direction * (this.container.clientHeight - 60) * fraction,
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
    const crop = this.crop;
    const cropKey = crop
      ? `${crop.x.toFixed(3)},${crop.y.toFixed(3)},${crop.width.toFixed(3)},${crop.height.toFixed(3)}`
      : "whole";
    return `${
      box ? box.scale.toFixed(3) : "0"
    }@${density}|${this.rotation}|${cropKey}|${themeKey}`;
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
      selectionEl: null,
      selectionRuns: new Map(),
      linkEl: null,
      noteEl: null,
      markupEl: null,
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
    this.clearSelection(slot);
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
    const scale = box.scale * density;
    // Only the part of the page that is being shown is drawn. `offsetX` and
    // `offsetY` slide the page under the canvas, and the canvas is the size of
    // the crop — so a trimmed document costs *less* to draw than an untrimmed
    // one rather than the same, and nothing is rendered that will be clipped.
    const whole = this.wholeSizeOf(slot.index);
    const crop = this.crop;
    const viewport = this.viewportFor(page, scale, {
      offsetX: -(crop ? crop.x * whole.width * scale : 0),
      offsetY: -(crop ? crop.y * whole.height * scale : 0),
    });

    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(whole.width * (crop?.width ?? 1) * scale));
    canvas.height = Math.max(1, Math.round(whole.height * (crop?.height ?? 1) * scale));
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) {
      slot.renderedKey = key;
      release(canvas);
      return;
    }

    // Everything from here to the moment the canvas is on screen can be
    // abandoned — a discarded slot, a theme change, a closed document, a
    // cancelled render — and dropping the last reference to a canvas is not the
    // same as freeing it, which is what `release` is for. A fast scroll abandons
    // renders steadily, so this is the same leak by a different door.
    let adopted = false;
    try {
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
        if (!isRenderCancelled(error)) {
          // Give up on this page rather than retry it forever.
          slot.renderedKey = key;
          this.callbacks.onError(`Page ${slot.index + 1} could not be drawn.`);
        }
        return;
      } finally {
        if (slot.task === task) slot.task = null;
      }
      if (this.doc !== doc || this.slots.get(slot.index) !== slot) return;

      // What this page turned out to cost. A page is only known to carry
      // pictures once it has been drawn, which is also the moment its pictures
      // start taking up room.
      if (holdsPictures(page)) {
        this.pictorial.add(slot.index);
        this.trimPages();
      }

      if (theme) {
        // Links are coloured under every theme, including the ones that leave the
        // document alone otherwise. A link that reads exactly like the sentence
        // around it is a link nobody can see, and whether the page has been
        // recoloured is beside that point.
        const links = await this.linksFor(slot.index, page);
        if (this.doc !== doc || this.slots.get(slot.index) !== slot) return;
        // Populated by the same call, above — see `markupRegionsFrom`. Only
        // `/Highlight` runs need the redraw `tintMarkup` does; an underline or
        // a squiggly survives `recolor`'s own colour-keeping pass unaided.
        const markup = (this.markupCache.get(slot.index) ?? []).filter(
          (region) => region.style === "highlight",
        );
        // `page.imageCoordinates`, not `task.imageCoordinates`: the RenderTask
        // getter reads a field pdf.js never sets — the `complete` callback
        // writes the page's own instead. Reading the task's copy meant this was
        // always null, so every picture was recoloured whatever the setting
        // said. That is now the default; this is what makes turning it off do
        // anything.
        const coordinates: ArrayLike<number> | null = wantsImages
          ? page.imageCoordinates
          : null;
        const hasImages = Boolean(coordinates && coordinates.length > 0);
        // A copy of the page as it was drawn, to paint back over the recolouring.
        // Only a recoloured page has anything to undo, and the copy is the whole
        // canvas — at a high zoom that is tens of megabytes, so a theme that
        // leaves the document alone must not pay for it. Every zoom step
        // repaints every visible page, which is where that cost would land.
        const pristine = theme.recolor && (hasImages || links.length > 0 || markup.length > 0)
          ? copyCanvas(canvas)
          : null;
        try {
          if (theme.recolor) recolor(ctx, canvas.width, canvas.height, theme);
          if (markup.length > 0) {
            // Before the links: a highlighted cross-reference is rare, and
            // where the two overlap the link's own colour is the one that
            // should win, the same way it already wins over a page's own ink.
            tintMarkup(ctx, pristine, canvas.width, canvas.height, inCrop(markup, crop), theme);
          }
          if (links.length > 0) {
            // A link's rectangle is a fraction of the whole page and the
            // canvas is a fraction of it, so the two have to be put in the
            // same terms before one is multiplied by the other. The layer of
            // real links over the top keeps the page's own fractions: it is
            // the size of the whole page. See `placeOverlay`.
            tintLinks(ctx, pristine, canvas.width, canvas.height, inCrop(links, crop), theme);
          }
          if (pristine && coordinates && hasImages) {
            restoreImages(ctx, pristine, canvas.width, canvas.height, coordinates);
          }
        } finally {
          // As big as the page it copied, and finished with either way.
          release(pristine);
        }
      }

      const replaced = slot.canvas;
      slot.canvas = canvas;
      slot.el.prepend(canvas);
      adopted = true;
      replaced?.remove();
      release(replaced);
      slot.renderedKey = key;
    } finally {
      // Adopted means the slot owns it now and `discard` will do this instead.
      if (!adopted) release(canvas);
    }

    // The pixels the selection was copied from have just been replaced.
    this.refreshSelection();

    await this.renderText(slot, page, box.scale);
    // Links do not hold up the queue: the page is already readable without
    // them, and they are placed in fractions of the page, so they are correct
    // whenever they arrive and stay correct at every zoom afterwards.
    void this.renderLinks(slot, page);
  }

  /** The selectable text over a page.
   *
   * Built once per mounted page and thereafter only rescaled: pdf.js lays its
   * spans out in percentages and sizes them from `--total-scale-factor`, which
   * `place()` sets on every layout, so `update` only has to agree about the
   * number. Rebuilding meant re-streaming the page's text out of the worker on
   * every zoom step — and throwing away whatever the reader had selected. */
  private async renderText(slot: Slot, page: PDFPageProxy, scale: number): Promise<void> {
    const viewport = this.viewportFor(page, scale);

    if (slot.textLayer && slot.textEl) {
      this.placeOverlay(slot.textEl, slot.index, scale);
      slot.textLayer.update({ viewport });
      this.paintHighlights(slot);
      this.finishReveal(slot.index + 1);
      return;
    }

    slot.textEl?.remove();
    const container = document.createElement("div");
    container.className = "textLayer";
    // Sized and offset as a whole page, because that is what its percentages
    // are fractions of. See `placeOverlay`.
    this.placeOverlay(container, slot.index, scale);
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

    type Annotation = {
      subtype?: string;
      rect?: number[];
      dest?: unknown;
      url?: string;
      contentsObj?: { str?: string };
      titleObj?: { str?: string };
      quadPoints?: ArrayLike<number> | null;
      color?: ArrayLike<number> | null;
      opacity?: number;
      id?: string;
    };
    let annotations: Annotation[];
    try {
      annotations = await page.getAnnotations({ intent: "display" });
    } catch {
      annotations = []; // A document with unreadable annotations still reads fine.
    }

    const view = this.viewportFor(page, 1);
    // Notes and markup come out of the same fetch, because they are the same
    // annotations: asking twice would be two trips into the worker for one
    // answer. See `markupRegionsFrom` for why this is the drawing path's own
    // shape rather than the journal's `Highlight`.
    this.noteCache.set(index, notesIn(annotations, view));
    this.markupCache.set(index, markupRegionsFrom(annotations, view));
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

  /**
   * The notes somebody else left on this page, made readable.
   *
   * pdf.js paints an annotation's appearance into the page, so a sticky note
   * arrives as the icon it was drawn as. What does not arrive is the text
   * behind it, which lives in a popup annotation this app does not build — so
   * the icon sat there looking like a button and was not one.
   *
   * An icon-sized note gets a hit area over the whole of it; a note that is a
   * passage of text gets a narrow strip at its right edge, because covering the
   * passage would take the words underneath out of the pointer's reach.
   */
  private renderNotes(slot: Slot): void {
    if (slot.noteEl) return;
    const notes = this.noteCache.get(slot.index) ?? [];
    if (notes.length === 0) return;

    const layer = document.createElement("div");
    layer.className = "note-layer";
    for (const note of notes) {
      const spot = document.createElement("button");
      spot.className = note.icon ? "note-spot" : "note-edge";
      spot.style.left = `${(note.icon ? note.x : note.x + note.width) * 100}%`;
      spot.style.top = `${note.y * 100}%`;
      if (note.icon) {
        spot.style.width = `${note.width * 100}%`;
        spot.style.height = `${note.height * 100}%`;
      } else {
        spot.style.height = `${note.height * 100}%`;
      }
      const by = note.by ? `${note.by}: ` : "";
      spot.title = `${by}${note.text}`;
      spot.setAttribute("aria-label", `Note. ${by}${note.text}`);
      spot.addEventListener("click", (event) => {
        event.preventDefault();
        this.callbacks.onNote({ by: note.by, text: note.text, page: slot.index + 1 });
      });
      layer.append(spot);
    }

    const box = this.boxes[slot.index];
    if (box) this.placeOverlay(layer, slot.index, box.scale);
    slot.el.append(layer);
    slot.noteEl = layer;
  }

  /**
   * A click target over every run of this app's own coloured markup, so
   * clicking marked text offers to take the mark out — see `App.removeMarkup`
   * for what happens next.
   *
   * Covers the run exactly rather than an edge strip the way `renderNotes`
   * does for a passage of text: a note's text lives elsewhere and only its
   * icon or margin needs a hit area, but a highlight *is* the words underneath
   * it, and "click the marked text" is the gesture being offered here.
   * Markup with no `annotationId` — this app's own quads but not yet, or not
   * ever, written into the file — is not drawn from `getAnnotationsByType` at
   * all, so there is nothing on the page to attach a hit target to; that case
   * only ever shows up in the Contents panel.
   */
  private renderMarkupHits(slot: Slot): void {
    if (slot.markupEl) return;
    const regions = (this.markupCache.get(slot.index) ?? []).filter(
      (region) => region.annotationId !== null,
    );
    if (regions.length === 0) return;

    const layer = document.createElement("div");
    layer.className = "markup-layer";
    for (const region of regions) {
      const id = region.annotationId;
      if (!id) continue;
      const hit = document.createElement("button");
      hit.className = "markup-hit";
      hit.style.left = `${region.x * 100}%`;
      hit.style.top = `${region.y * 100}%`;
      hit.style.width = `${region.width * 100}%`;
      hit.style.height = `${region.height * 100}%`;
      hit.setAttribute("aria-label", "Marked passage. Open to remove the mark.");
      hit.addEventListener("click", (event) => {
        event.preventDefault();
        this.callbacks.onMarkupClick(slot.index + 1, id, hit);
      });
      layer.append(hit);
    }

    const box = this.boxes[slot.index];
    if (box) this.placeOverlay(layer, slot.index, box.scale);
    slot.el.append(layer);
    slot.markupEl = layer;
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
    // Before the return below: a page can carry notes or markup and no links,
    // and that page still has them.
    this.renderNotes(slot);
    this.renderMarkupHits(slot);
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

      // Deliberately not an `href`: an anchor carrying the address navigates on
      // a middle click, which never reaches the click handler — so the webview
      // left the app, taking the open document with it. Every destination goes
      // out through `onExternalLink`, which is the only thing allowed to decide
      // what opening a link means.
      link.setAttribute("role", "link");
      link.tabIndex = 0;
      // A name, because the element has no text of its own — a bare rectangle
      // over printed words, whose words are in the text layer where this cannot
      // reach them. Without one, a page of cross-references reads as "link,
      // link, link".
      //
      // An external link says where it goes; an internal one does not name its
      // page, because resolving a destination is a trip into the worker each and
      // a page of mathematics has hundreds.
      link.setAttribute("aria-label", url ?? "Elsewhere in this document");
      const follow = (event: Event) => {
        event.preventDefault();
        if (url) this.callbacks.onExternalLink(url);
        else void this.goToDestination(dest);
      };
      if (url) link.title = url;
      link.addEventListener("click", follow);
      // The middle button, which never fires `click`. Only the middle one:
      // `auxclick` is also how the right button and a mouse's back and forward
      // buttons arrive, so following on any of them meant a right-click on a
      // cross-reference navigated, and so did pressing "back" while the
      // pointer happened to be over a link.
      link.addEventListener("auxclick", (event) => {
        // Not the mouseup at the end of a drag across the page: the pointer
        // has to finish somewhere, and a link is as likely a place as any.
        if (event.button === 1 && !this.panned) follow(event);
      });
      link.addEventListener("keydown", (event) => {
        if (event.key === "Enter" || event.key === " ") follow(event);
      });
      layer.append(link);
    }

    if (layer.childElementCount === 0) return;
    // A link's rectangle is a fraction of the whole page, so the layer is the
    // whole page even when only part of it is on screen.
    const box = this.boxes[slot.index];
    if (box) this.placeOverlay(layer, slot.index, box.scale);
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
      this.jumpTo(index + 1, await this.offsetWithin(index, explicit));
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
      const view = this.viewportFor(page, 1);
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
    this.matchPages = new Map();
    for (let at = 0; at < matches.length; at++) {
      const match = matches[at];
      const onPage = this.matchPages.get(match.page);
      if (onPage) onPage.push({ at, match });
      else this.matchPages.set(match.page, [{ at, match }]);
    }
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
   * A match is a rectangle in the text layer, and the text layer of a page that
   * was not already on screen does not exist yet — it is built after the canvas,
   * a render away. Scrolling to the top of the page and stopping there is what
   * "it went to the page but I cannot see the word" looks like. So the reveal is
   * remembered, and whichever comes second finishes the job. */
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

    for (const { at: i, match } of this.matchPages.get(slot.index + 1) ?? []) {
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
      // Guarded like every other read of `boxes` in this file. Paged mode
      // lays out one page and leaves the rest of the array empty, so a slot
      // that has not been discarded yet has no box — and this is the one read
      // that was assuming otherwise without saying so.
      const box = this.boxes[slot.index];
      if (!box) return;
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
 * The rectangles a selection covers on one page, in that page's own
 * coordinates, tidied into the runs a reader would draw with a highlighter.
 *
 * Rounded outwards, because two rectangles that meet do so at a fraction and a
 * copy stopping short leaves a hairline of unselected page. And rectangles on
 * one line with a word's worth of space between them are joined, because
 * pdf.js's spans do not abut and the gaps otherwise show as white rules through
 * a highlighted sentence. A gap wider than half the line is a column of its
 * own, not a space.
 */
function joinRuns(rects: DOMRect[], page: DOMRect): Rect[] {
  const runs = rects
    .map((rect) => {
      const left = Math.floor(rect.left - page.left);
      const top = Math.floor(rect.top - page.top);
      return {
        x: left,
        y: top,
        w: Math.ceil(rect.right - page.left) - left,
        h: Math.ceil(rect.bottom - page.top) - top,
      };
    })
    .sort((a, b) => a.y - b.y || a.x - b.x);

  const joined: Rect[] = [];
  for (const run of runs) {
    const last = joined[joined.length - 1];
    const sameLine = last && Math.abs(last.y - run.y) <= 1 && Math.abs(last.h - run.h) <= 1;
    if (sameLine && run.x - (last.x + last.w) <= run.h / 2) {
      last.w = Math.max(last.x + last.w, run.x + run.w) - last.x;
    } else {
      joined.push(run);
    }
  }
  return joined;
}

/**
 * The annotations on a page that carry something to read.
 *
 * By whether there *is* text rather than by subtype, because a comment on a
 * highlight and a sticky note are the same thing to a reader. Links are the
 * exception: their text is where they go, which is already on the link.
 */
function notesIn(
  annotations: {
    subtype?: string;
    rect?: number[];
    contentsObj?: { str?: string };
    titleObj?: { str?: string };
  }[],
  view: { width: number; height: number; convertToViewportRectangle(rect: number[]): number[] },
): Note[] {
  const notes: Note[] = [];
  for (const annotation of annotations) {
    if (annotation.subtype === "Link" || annotation.subtype === "Popup") continue;
    const text = annotation.contentsObj?.str?.trim() ?? "";
    if (!text || !annotation.rect) continue;

    const [x1, y1, x2, y2] = view.convertToViewportRectangle(annotation.rect);
    const width = Math.abs(x2 - x1) / view.width;
    const height = Math.abs(y2 - y1) / view.height;
    if (width <= 0 || height <= 0) continue;
    notes.push({
      x: Math.min(x1, x2) / view.width,
      y: Math.min(y1, y2) / view.height,
      width,
      height,
      // An icon is small in both directions. A comment on a highlighted
      // sentence is not, and covering the sentence would put it out of reach.
      icon: width < 0.06 && height < 0.06,
      by: annotation.titleObj?.str?.trim() ?? "",
      text,
    });
  }
  return notes;
}

/** pdf.js's own prefix for an `annotationStorage` key that describes a *new*
    annotation to create, as opposed to an edit to one already in the file —
    `getNewAnnotationsMap` in the shipped worker keys off exactly this string.
    Undocumented, and pinned to the pdfjs-dist version this was read against;
    see `Viewer.markSelection`. */
const ANNOTATION_EDITOR_PREFIX = "pdfjs_internal_editor_";

/** The PDF spec's own four markup subtypes, as pdf.js names them, mapped onto
    the journal's own spelling. Nothing this app invents. */
const MARKUP_STYLES: Record<string, HighlightStyle> = {
  Highlight: "highlight",
  Underline: "underline",
  StrikeOut: "strikeout",
  Squiggly: "squiggly",
};

/**
 * Coloured markup on a page, ready to paint — one `MarkupRegion` per run of
 * `/QuadPoints`, in fractions of the page like a link's rectangle.
 *
 * `linksFor`'s conversion, not `toHighlight`'s below: the drawing path wants a
 * run on screen and the journal wants it in the file's own space, because that
 * is what a later save writes back. The two need only agree with the same
 * annotation.
 *
 * All four quads are converted and the axis-aligned box around them kept — the
 * same simplification a link's `/Rect` already is.
 */
function markupRegionsFrom(
  annotations: {
    subtype?: string;
    quadPoints?: ArrayLike<number> | null;
    color?: ArrayLike<number> | null;
    opacity?: number;
    id?: string;
  }[],
  view: PageViewport,
): MarkupRegion[] {
  const regions: MarkupRegion[] = [];
  for (const annotation of annotations) {
    const style = MARKUP_STYLES[annotation.subtype ?? ""];
    const quads = annotation.quadPoints;
    if (!style || !quads || quads.length === 0 || quads.length % 8 !== 0) continue;

    const c = annotation.color;
    const color = toHex(c && c.length >= 3 ? [c[0], c[1], c[2]] : [0, 0, 0]);
    const opacity = annotation.opacity ?? 1;

    for (let i = 0; i + 7 < quads.length; i += 8) {
      let left = Infinity;
      let top = Infinity;
      let right = -Infinity;
      let bottom = -Infinity;
      for (let corner = 0; corner < 4; corner++) {
        const [vx, vy] = view.convertToViewportPoint(quads[i + corner * 2], quads[i + corner * 2 + 1]);
        left = Math.min(left, vx);
        right = Math.max(right, vx);
        top = Math.min(top, vy);
        bottom = Math.max(bottom, vy);
      }
      const width = right - left;
      const height = bottom - top;
      if (width < 1 || height < 1) continue;
      regions.push({
        x: left / view.width,
        y: top / view.height,
        width: width / view.width,
        height: height / view.height,
        color,
        opacity,
        style,
        annotationId: annotation.id ?? null,
      });
    }
  }
  return regions;
}

/** The shape `getAnnotationsByType` hands back for one of the four subtypes
    above. The rest of what an annotation carries — `rect`, `contentsObj` and
    so on — belongs to links and notes, not this. */
type MarkupAnnotation = {
  subtype?: string;
  quadPoints?: ArrayLike<number> | null;
  color?: ArrayLike<number> | null;
  opacity?: number;
  id?: string;
  pageIndex?: number;
};

/**
 * One annotation, read as coloured markup — or `null` where it is not one of
 * the four subtypes above, or carries no quads to anchor it.
 *
 * The quads are kept exactly as pdf.js reports them, in the page's own PDF
 * space rather than as a fraction of a viewport: that is what the file agrees
 * on and what a later save writes straight back into.
 *
 * `quote` is read from under the quad, and `at` is when this was *read* rather
 * than drawn — this is markup the reader did not just make, so the moment is
 * not known.
 */
function toHighlight(annotation: MarkupAnnotation, page: number, items: TextItem[]): Highlight | null {
  const style = MARKUP_STYLES[annotation.subtype ?? ""];
  const quads = annotation.quadPoints;
  if (!style || !quads || quads.length === 0 || quads.length % 8 !== 0) return null;

  const rgb: [number, number, number] =
    annotation.color && annotation.color.length >= 3
      ? [annotation.color[0], annotation.color[1], annotation.color[2]]
      : [0, 0, 0];
  return {
    id: crypto.randomUUID(),
    page,
    quads: Array.from(quads),
    color: toHex(rgb),
    opacity: annotation.opacity ?? 1,
    style,
    quote: quoteFor(quads, items),
    at: Date.now(),
    annotation_id: annotation.id ?? null,
  };
}

/** Every subtype `toHighlight` reads, as the numbers `AnnotationType` gives
    them — what `getAnnotationsByType` is asked for below. */
const MARKUP_TYPES = new Set<number>([
  AnnotationType.HIGHLIGHT,
  AnnotationType.UNDERLINE,
  AnnotationType.STRIKEOUT,
  AnnotationType.SQUIGGLY,
]);

/**
 * A page's text items, read for their position rather than for a transcript:
 * `quoteFor` needs each item's own box, not the joined string `readTextRuns`
 * builds from the same stream.
 *
 * Deliberately not `getTextContent()`, which iterates the stream with `for
 * await` — WebKit's `ReadableStream` has no async iterator.
 */
async function readTextItems(page: PDFPageProxy): Promise<TextItem[]> {
  const reader = page.streamTextContent().getReader();
  const items: TextItem[] = [];
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    for (const item of value.items) {
      if ("str" in item) items.push(item as TextItem);
    }
  }
  return items;
}

/** One line of markup, as `/QuadPoints` spells a rectangle: upper-left,
    upper-right, lower-left, lower-right. Normalised again by whoever reads it
    back — `getQuadPoints` in the worker does the same min/max — so the order
    costs nothing to get wrong and is written properly anyway. */
function quadOf(minX: number, minY: number, maxX: number, maxY: number): number[] {
  return [minX, maxY, maxX, maxY, minX, minY, maxX, minY];
}

/** Page numbers from `near` outwards: the page itself, then the one after,
    then the one before, and so on to both ends. A rebuilt document has moved
    a passage by a page or two far more often than it has moved it to the
    other end of the book, and this is the difference between one page read
    and four hundred. */
function outwards(near: number, count: number): number[] {
  const start = Math.min(Math.max(Math.round(near) || 1, 1), Math.max(count, 1));
  const order = [start];
  for (let step = 1; order.length < count; step++) {
    if (start + step <= count) order.push(start + step);
    if (start - step >= 1) order.push(start - step);
  }
  return order;
}

/**
 * The quads a highlight over `wanted` would need on the page these text items
 * came from — or `null` where the page does not carry those words.
 *
 * The page's text is folded the same way and matched as one string, because a
 * quote routinely runs across the boundary between text items: a producer
 * splits a line wherever the font or kerning changes, so "the quick brown fox"
 * is four items and matching them one at a time finds none of it.
 *
 * `origin` maps each folded character back into the page's real text and
 * `owner` maps that back to its item, so a hit is a set of items. One quad per
 * line, grouped by baseline, each padded below it to cover the descenders —
 * which is what makes `quoteFor` read the same words back out afterwards.
 */
function quadsAround(items: TextItem[], wanted: string): number[] | null {
  if (items.length === 0) return null;

  let raw = "";
  const owner: number[] = [];
  items.forEach((item, index) => {
    for (let i = 0; i < item.str.length; i++) owner.push(index);
    raw += item.str;
    if (item.hasEOL) {
      owner.push(index);
      raw += " ";
    }
  });

  const folded = fold(raw);
  const at = folded.text.indexOf(wanted);
  if (at === -1) return null;
  const from = folded.origin[at];
  const to = folded.origin[Math.min(at + wanted.length, folded.origin.length - 1)];

  const marked = new Set<number>();
  for (let i = from; i < to && i < owner.length; i++) marked.add(owner[i]);
  if (marked.size === 0) return null;

  // By baseline, to the nearest point: two items on one line agree on it
  // exactly far more often than not, and where they are a fraction apart the
  // rounding puts them together rather than drawing two overlapping washes.
  const lines = new Map<number, { minX: number; maxX: number; minY: number; maxY: number }>();
  for (const index of marked) {
    const item = items[index];
    const height = item.height || Math.abs(item.transform[3]) || 1;
    const baseline = item.transform[5];
    const key = Math.round(baseline);
    const box = lines.get(key);
    const left = item.transform[4];
    const right = left + item.width;
    // Down a fifth of the line to clear the descenders, up the full height:
    // `quoteFor` only credits a highlight with an item that sits *wholly*
    // inside it, so a box drawn tight to the baseline would read its own
    // words back as nothing.
    const bottom = baseline - height * 0.2;
    const top = baseline + height;
    if (box) {
      box.minX = Math.min(box.minX, left);
      box.maxX = Math.max(box.maxX, right);
      box.minY = Math.min(box.minY, bottom);
      box.maxY = Math.max(box.maxY, top);
    } else {
      lines.set(key, { minX: left, maxX: right, minY: bottom, maxY: top });
    }
  }

  const quads: number[] = [];
  // Down the page, which is the order the words were read in.
  for (const box of [...lines.values()].sort((a, b) => b.maxY - a.maxY)) {
    quads.push(...quadOf(box.minX, box.minY, box.maxX, box.maxY));
  }
  return quads.length > 0 ? quads : null;
}

/**
 * The words under a highlight's quads, read back out of the page rather than
 * carried from the gesture that drew them — which is what makes this correct
 * for a highlight this app did not draw, and why there is one path rather than
 * two.
 *
 * An item counts as under a run of quad points when it sits *wholly* inside
 * that run's bounding box, not merely centred in it: a producer writing a whole
 * line as one `Tj` hands back an item far wider than a highlight over part of
 * it, and a centre-point test would credit the highlight with the rest of the
 * line. The cost is that partly-marked wide items attribute nothing, which is
 * the safer failure. Rotated glyphs and vertical writing are not accounted for,
 * the same limit `joinRuns` accepts for the screen.
 */
function quoteFor(quads: ArrayLike<number>, items: TextItem[]): string {
  const runs: { xMin: number; xMax: number; yMin: number; yMax: number }[] = [];
  for (let i = 0; i + 7 < quads.length; i += 8) {
    const xs = [quads[i], quads[i + 2], quads[i + 4], quads[i + 6]];
    const ys = [quads[i + 1], quads[i + 3], quads[i + 5], quads[i + 7]];
    runs.push({
      xMin: Math.min(...xs),
      xMax: Math.max(...xs),
      yMin: Math.min(...ys),
      yMax: Math.max(...ys),
    });
  }
  if (runs.length === 0) return "";

  let quote = "";
  let last = -2;
  items.forEach((item, index) => {
    const left = item.transform[4];
    const right = left + item.width;
    const height = item.height || Math.abs(item.transform[3]) || 1;
    const cy = item.transform[5] + height / 2;
    const under = runs.some(
      (r) => left >= r.xMin - 1 && right <= r.xMax + 1 && cy >= r.yMin - 1 && cy <= r.yMax + 1,
    );
    if (!under) return;
    if (quote && index !== last + 1) quote += " ";
    quote += item.str;
    if (item.hasEOL) quote += " ";
    last = index;
  });
  return quote.trim().replace(/\s+/g, " ");
}

/**
 * Every highlight, underline, strike-out and squiggly the document already
 * carries, read in one trip rather than one page at a time.
 *
 * What the journal in `library.toml` is rebuilt from on open — `App.syncMarkup`
 * calls this once and replaces the journal outright. It must be the whole
 * document: a replace built from a partial scroll would discard the entries for
 * every page nobody has looked at, which the file still carries.
 *
 * pdf.js reads every page's `/Annots` dictionary — a structural read, no
 * appearance stream and no canvas — and each annotation comes back carrying its
 * `pageIndex`.
 *
 * The quoted text costs one more trip per *marked* page, which is why
 * annotations are grouped by page first: a page with three highlights pays for
 * its text once, and a document with none pays nothing.
 */
export async function markupOf(doc: PDFDocumentProxy): Promise<Highlight[]> {
  let annotations: MarkupAnnotation[] | null;
  try {
    annotations = (await doc.getAnnotationsByType(
      MARKUP_TYPES,
      new Set(),
    )) as MarkupAnnotation[] | null;
  } catch {
    return []; // A document with unreadable annotations still reads fine.
  }
  if (!annotations) return [];

  const byPage = new Map<number, MarkupAnnotation[]>();
  for (const annotation of annotations) {
    const page = (annotation.pageIndex ?? 0) + 1;
    const onPage = byPage.get(page);
    if (onPage) onPage.push(annotation);
    else byPage.set(page, [annotation]);
  }

  const markup: Highlight[] = [];
  for (const [page, onPage] of byPage) {
    let items: TextItem[] = [];
    try {
      const proxy = await doc.getPage(page);
      items = await readTextItems(proxy);
      proxy.cleanup();
    } catch {
      // No text on this page, or none pdf.js could read — the quads still
      // anchor the highlight, so it is kept with an empty quote rather than
      // dropped.
    }
    for (const annotation of onPage) {
      const highlight = toHighlight(annotation, page, items);
      if (highlight) markup.push(highlight);
    }
  }
  return markup;
}

/** A link's or a markup region's rectangle, restated as fractions of the part
    of the page on screen — the same trim, whichever it is being asked of. */
function inCrop<T extends { x: number; y: number; width: number; height: number }>(
  items: T[],
  crop: Crop | null,
): T[] {
  if (!crop) return items;
  return items.map((item) => ({
    ...item,
    x: (item.x - crop.x) / crop.width,
    y: (item.y - crop.y) / crop.height,
    width: item.width / crop.width,
    height: item.height / crop.height,
  }));
}

/**
 * Colour the links on a page that has just been recoloured.
 *
 * A tinted box blended into the ink below is at the mercy of the compositor,
 * and a dropped blend is a solid band across the line. So the tint is painted
 * into the bitmap: the untouched page is put back inside the link's rectangle
 * and recoloured towards the link colour instead of the text colour. The paper
 * maps to the same background either way, so only the letters change and the
 * rectangle's edges leave no seam.
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
  // cannot blend, `duotone` works on pixels, and pixels do not honour a clip.
  duotone(
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

/**
 * Redraw a page's saved highlights over the recolouring `recolor` just did.
 *
 * A highlighter wash is translucent paint a shade or two off white,
 * `WHITE_POINT` calls anything that pale paper, and the ramp has already
 * flattened it into the theme's background. So the affected quads are put back
 * from the pristine copy and recoloured towards `markupWashColor` — the same
 * restore-then-reflatten `tintLinks` does, generalised to more than one
 * destination because two highlights can disagree.
 *
 * Only `/Highlight` runs need it: an underline, strike-out or squiggly draws a
 * solid stroke well below `WHITE_POINT`, which `recolor`'s colour-keeping pass
 * already carries across a theme.
 */
function tintMarkup(
  ctx: CanvasRenderingContext2D,
  pristine: CanvasImageSource | null,
  width: number,
  height: number,
  regions: MarkupRegion[],
  theme: Theme,
): void {
  const washes = new Map<string, Rect[]>();
  for (const region of regions) {
    if (region.style !== "highlight") continue;
    const wash = markupWashColor(theme, region.color, region.opacity);
    const rect: Rect = {
      x: region.x * width,
      y: region.y * height,
      w: region.width * width,
      h: region.height * height,
    };
    const rects = washes.get(wash);
    if (rects) rects.push(rect);
    else washes.set(wash, [rect]);
  }
  if (washes.size === 0) return;

  // Every wash shares one clip and one restore of the untouched page, the
  // same reason `restoreImages` puts every picture back in a single pass
  // rather than one `drawImage` per rectangle.
  ctx.save();
  ctx.beginPath();
  for (const rects of washes.values()) {
    for (const rect of rects) ctx.rect(rect.x, rect.y, rect.w, rect.h);
  }
  ctx.clip();
  if (pristine) ctx.drawImage(pristine, 0, 0, width, height);
  ctx.restore();

  // Then each colour is flattened towards its own wash, in its own clip —
  // the ink inside stays the theme's own text colour, unaffected, which is
  // what keeps the words under a highlight reading exactly like the rest of
  // the page.
  for (const [wash, rects] of washes) {
    ctx.save();
    ctx.beginPath();
    for (const rect of rects) ctx.rect(rect.x, rect.y, rect.w, rect.h);
    ctx.clip();
    duotone(ctx, width, height, { ...theme, background: wash, recolor: true }, rects);
    ctx.restore();
  }
}

function copyCanvas(source: HTMLCanvasElement): HTMLCanvasElement {
  const copy = document.createElement("canvas");
  copy.width = source.width;
  copy.height = source.height;
  const ctx = copy.getContext("2d");
  ctx?.drawImage(source, 0, 0);
  // Read back one pixel — load-bearing, not a nicety. At least one WebKit
  // build leaves this copy lazily backed: a *later* clipped `drawImage` of
  // it (which is exactly what `tintLinks`, `restoreImages` and `tintMarkup`
  // all do, to put an untouched rectangle back over a recoloured page) can
  // silently draw nothing at all, with no error anywhere, unless something
  // has already forced the copy to materialise. A one-pixel read is the
  // cheapest such force.
  ctx?.getImageData(0, 0, 1, 1);
  return copy;
}

/** Whether a drawn page is holding decoded pictures.
 *
 * `recordImages` cannot answer this: it reports where pdf.js painted image
 * XObjects, and a bitonal scan — the expensive case, and the reason any of this
 * exists — arrives as an image *mask* and is not among them. The page's own
 * object store is the honest answer, because holding those objects is the cost
 * being counted. Only the ids are wanted; the data behind one may be null by
 * the time anyone looks. */
function holdsPictures(page: PDFPageProxy): boolean {
  try {
    for (const [id] of page.objs) {
      if (typeof id === "string" && id.startsWith("img")) return true;
    }
  } catch {
    // An object store that will not be walked is not a reason to stop drawing.
  }
  return false;
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
