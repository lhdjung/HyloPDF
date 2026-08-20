/* Full-document search.
 *
 * The index is built once per document, lazily, page by page: text extraction
 * of a long book takes a moment, so results stream in and the count keeps
 * climbing while the reader is already looking at the first hit.
 *
 * Streaming has a cost, though, and it is paid on screen. Every time results
 * are handed to the viewer it measures each one against the text layer, which
 * makes the browser lay the page out again. A single letter in a long article
 * finds thousands of matches, and doing that after every page turned typing
 * into a slideshow. So the scan works in short slices, hands its results over
 * a few times a second rather than a hundred, and stops counting long before
 * a count could mean anything to anybody. */

import type {
  PDFDocumentProxy,
  PDFPageProxy,
} from "pdfjs-dist/types/src/display/api";
import type { Match, Viewer } from "./viewer";

type PageText = {
  /** Every text run on the page, in the order the text layer draws them. */
  items: string[];
  /** The runs joined, which is what a query is matched against. */
  text: string;
  /** Where each run starts inside `text`. */
  starts: number[];
};

/** Past this many, another match is not news. Stopping keeps the highlight
    work bounded no matter how common a letter is. */
const MATCH_LIMIT = 2000;
/** How long to scan before giving the rest of the app a turn. */
const SLICE_MS = 12;
/** How often results reach the screen while the scan is still running. */
const FLUSH_MS = 120;

export type SearchState = {
  query: string;
  total: number;
  index: number;
  scanning: boolean;
  /** True when the scan stopped at the limit, so the total is a floor. */
  capped: boolean;
};

export class Search {
  private pages = new Map<number, PageText>();
  /** Matches by page, so the ordered list can be rebuilt without sorting it
      again every time another page comes in. */
  private found = new Map<number, Match[]>();
  private matches: Match[] = [];
  private index = -1;
  private query = "";
  private run = 0;
  private capped = false;

  constructor(
    private viewer: Viewer,
    private onUpdate: (state: SearchState) => void,
  ) {}

  reset(): void {
    this.pages.clear();
    this.clear();
  }

  clear(): void {
    this.run++;
    this.query = "";
    this.found.clear();
    this.matches = [];
    this.index = -1;
    this.capped = false;
    this.viewer.setMatches([], -1);
    this.onUpdate({ query: "", total: 0, index: -1, scanning: false, capped: false });
  }

  get total(): number {
    return this.matches.length;
  }

  async find(query: string, doc: PDFDocumentProxy): Promise<void> {
    const token = ++this.run;
    this.query = query;
    this.found.clear();
    this.matches = [];
    this.index = -1;
    this.capped = false;

    if (query.trim().length === 0) {
      this.viewer.setMatches([], -1);
      this.onUpdate({ query, total: 0, index: -1, scanning: false, capped: false });
      return;
    }

    const needle = query.toLowerCase();
    // Start at the page being read, then outwards, so the first result is
    // usually the one just below the reader's eyes.
    const order = pagesFromHere(this.viewer.pageNumber, doc.numPages);

    // The match to settle on: the first one found, which is the nearest one
    // below the reader rather than the first in the document.
    let preferred: Match | null = null;
    let total = 0;
    let pending = false;
    let shown = false;
    let sliceStarted = performance.now();
    let flushed = 0;

    // A long document with no hits in it would otherwise leave the last
    // search's count sitting there while this one works.
    this.viewer.setMatches([], -1);
    this.onUpdate({ query, total: 0, index: -1, scanning: true, capped: false });

    for (const page of order) {
      if (token !== this.run) return;
      const text = await this.textFor(page, doc);
      if (token !== this.run) return;

      const hits = locate(text, needle, page);
      if (hits.length > 0) {
        if (total + hits.length >= MATCH_LIMIT) {
          hits.length = MATCH_LIMIT - total;
          this.capped = true;
        }
        if (hits.length > 0) {
          this.found.set(page, hits);
          preferred ??= hits[0];
          total += hits.length;
          pending = true;
        }
      }

      const now = performance.now();
      // The first results go up at once; after that, a few times a second.
      if (pending && (!shown || now - flushed > FLUSH_MS)) {
        this.publish(preferred, !shown, true);
        pending = false;
        shown = true;
        flushed = now;
      }
      if (this.capped) break;

      if (now - sliceStarted > SLICE_MS) {
        await breathe();
        if (token !== this.run) return;
        sliceStarted = performance.now();
      }
    }

    if (token !== this.run) return;
    this.publish(preferred, !shown && total > 0, false);
  }

  /** Hand the results over: rebuild the list in page order, keep the reader on
      the match they were on, and tell the viewer once. */
  private publish(preferred: Match | null, reveal: boolean, scanning: boolean): void {
    const standing = this.matches[this.index] ?? preferred;
    this.matches = [];
    for (const page of [...this.found.keys()].sort((a, b) => a - b)) {
      for (const match of this.found.get(page)!) this.matches.push(match);
    }
    this.index = standing ? this.matches.indexOf(standing) : -1;
    if (this.index < 0 && this.matches.length > 0) this.index = 0;

    this.viewer.setMatches(this.matches, this.index);
    if (reveal && this.index >= 0) this.viewer.revealMatch(this.index);
    this.onUpdate({
      query: this.query,
      total: this.matches.length,
      index: this.index,
      scanning,
      capped: this.capped,
    });
  }

  step(direction: 1 | -1): void {
    if (this.matches.length === 0) return;
    this.index = (this.index + direction + this.matches.length) % this.matches.length;
    this.viewer.setMatches(this.matches, this.index);
    this.viewer.revealMatch(this.index);
    this.onUpdate({
      query: this.query,
      total: this.matches.length,
      index: this.index,
      scanning: false,
      capped: this.capped,
    });
  }

  private async textFor(page: number, doc: PDFDocumentProxy): Promise<PageText> {
    const cached = this.pages.get(page);
    if (cached) return cached;
    const runs = await readTextRuns(await doc.getPage(page));
    const items: string[] = [];
    const starts: number[] = [];
    let text = "";
    for (const run of runs) {
      starts.push(text.length);
      items.push(run.str);
      text += run.str;
      if (run.hasEOL) text += "\n";
    }
    const built: PageText = { items, text: text.toLowerCase(), starts };
    this.pages.set(page, built);
    return built;
  }
}

/**
 * Read a page's text runs.
 *
 * Deliberately not `getTextContent()`: that iterates the stream with
 * `for await`, and WebKit — which is the engine on macOS — has no async
 * iterator on ReadableStream, so it throws. Pulling the reader by hand works
 * everywhere and is what pdf.js does one layer down.
 */
async function readTextRuns(
  page: PDFPageProxy,
): Promise<{ str: string; hasEOL: boolean }[]> {
  const reader = page.streamTextContent().getReader();
  const runs: { str: string; hasEOL: boolean }[] = [];
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    for (const item of value.items) {
      if ("str" in item) runs.push({ str: item.str, hasEOL: item.hasEOL });
    }
  }
  return runs;
}

/** Yield the thread long enough for the window to paint and for a keystroke to
    be heard. A macrotask, not a microtask: awaiting a promise alone would keep
    the browser out of the loop. */
function breathe(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function pagesFromHere(current: number, count: number): number[] {
  const order: number[] = [];
  for (let page = current; page <= count; page++) order.push(page);
  for (let page = 1; page < current; page++) order.push(page);
  return order;
}

function locate(page: PageText, needle: string, number: number): Match[] {
  const found: Match[] = [];
  let at = page.text.indexOf(needle);
  while (at !== -1) {
    const start = position(page, at);
    const end = position(page, at + needle.length);
    found.push({
      page: number,
      itemStart: start.item,
      offsetStart: start.offset,
      itemEnd: end.item,
      offsetEnd: end.offset,
    });
    at = page.text.indexOf(needle, at + Math.max(needle.length, 1));
  }
  return found;
}

/** Turn an offset into the joined page text back into a run and an offset
    inside it, which is what a DOM range needs. */
function position(page: PageText, offset: number): { item: number; offset: number } {
  let low = 0;
  let high = page.starts.length - 1;
  let item = 0;
  while (low <= high) {
    const middle = (low + high) >> 1;
    if (page.starts[middle] <= offset) {
      item = middle;
      low = middle + 1;
    } else {
      high = middle - 1;
    }
  }
  const within = offset - page.starts[item];
  const length = page.items[item].length;
  if (within > length) {
    // Landed on a line break, which belongs to no run: clamp to the run's end.
    return { item, offset: length };
  }
  return { item, offset: within };
}
