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
  /** The runs joined, exactly as the document has them. Kept because getting
      it back out of the worker is the expensive half of this, and folding it
      again is not: changing "Match case" refolds rather than re-extracts. */
  raw: string;
  /** Whether `text` below was folded with the case left alone. */
  cased: boolean;
  /** The runs joined and folded, which is what a query is matched against. */
  text: string;
  /** For each character of `text`, its offset in the unfolded page text.
      Folding changes lengths — "ﬁ" becomes two characters, a soft hyphen
      becomes none — so a hit has to be translated back before it can be
      pointed at a run. */
  origin: number[];
  /** Where each run starts inside the unfolded page text. */
  starts: number[];
};

/** How a query is matched. "Highlight all" is not here: it changes nothing
    about what is found, only how much of it is painted, and that belongs to
    the viewer. */
export type SearchOptions = {
  matchCase: boolean;
  wholeWords: boolean;
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
  /** Whether a scan is still running. Stepping through matches has to say so:
      it is a different thing from the scan, it can happen while one is in
      flight, and reporting `false` from there told the find bar the count was
      final when it was still climbing. */
  private scanning = false;
  private options: SearchOptions = { matchCase: false, wholeWords: false };

  constructor(
    private viewer: Viewer,
    private onUpdate: (state: SearchState) => void,
  ) {}

  reset(): void {
    this.pages.clear();
    this.clear();
  }

  /** Change how a query is matched. The extracted text stays: only the fold
      and the boundary test depend on these, and both are cheap. */
  setOptions(options: SearchOptions): void {
    this.options = options;
  }

  /** Put the index down.
   *
   * Every page ever scanned is kept — the joined text and the individual runs
   * — which is what makes stepping through matches instant, and what makes a
   * long book cost tens of megabytes for as long as it is open. That is a fair
   * trade while the find bar is up and no trade at all once it is closed, so
   * the index goes when the bar does. Reopening it rescans, which streams and
   * is over in well under a second. */
  forget(): void {
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
    this.scanning = false;
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
    this.scanning = false;

    if (query.trim().length === 0) {
      this.viewer.setMatches([], -1);
      this.onUpdate({ query, total: 0, index: -1, scanning: false, capped: false });
      return;
    }

    const needle = fold(query, this.options.matchCase).text;
    if (needle.length === 0) {
      // A query of nothing but soft hyphens or combining marks folds away to
      // nothing, and an empty needle matches at every position.
      this.viewer.setMatches([], -1);
      this.onUpdate({ query, total: 0, index: -1, scanning: false, capped: false });
      return;
    }
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
    this.scanning = true;
    this.onUpdate({ query, total: 0, index: -1, scanning: true, capped: false });

    for (const page of order) {
      if (token !== this.run) return;
      const text = await this.textFor(page, doc);
      if (token !== this.run) return;

      const hits = locate(text, needle, page, this.options.wholeWords);
      if (hits.length > 0) {
        // Strictly greater: a document with exactly `MATCH_LIMIT` matches has
        // had none of them dropped, and "2000+" would be a floor on a count
        // that is exact.
        if (total + hits.length > MATCH_LIMIT) {
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
    this.scanning = scanning;
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
      scanning: this.scanning,
      capped: this.capped,
    });
  }

  private async textFor(page: number, doc: PDFDocumentProxy): Promise<PageText> {
    const cached = this.pages.get(page);
    if (cached) return this.refold(cached);
    const proxy = await doc.getPage(page);
    const runs = await readTextRuns(proxy);
    const items: string[] = [];
    const starts: number[] = [];
    let raw = "";
    for (const run of runs) {
      starts.push(raw.length);
      items.push(run.str);
      raw += run.str;
      if (run.hasEOL) raw += "\n";
    }
    const folded = fold(raw, this.options.matchCase);
    const built: PageText = {
      items,
      raw,
      cased: this.options.matchCase,
      text: folded.text,
      origin: folded.origin,
      starts,
    };
    this.pages.set(page, built);
    // The proxy was fetched for its text and has no other work to do here;
    // holding its parsed contents would be a second copy of the document.
    proxy.cleanup();
    return built;
  }

  /** Bring a page indexed under the other case setting up to date. */
  private refold(page: PageText): PageText {
    if (page.cased === this.options.matchCase) return page;
    const folded = fold(page.raw, this.options.matchCase);
    page.cased = this.options.matchCase;
    page.text = folded.text;
    page.origin = folded.origin;
    return page;
  }
}

/**
 * Fold text into the form a search is actually done against, and record where
 * every character of the result came from.
 *
 * Three things stand between a typed word and the same word in a PDF, and all
 * three are invisible to the person typing:
 *
 * * **Ligatures.** A professionally typeset document does not contain "fi" —
 *   it contains "ﬁ", one character. Searching for "find" in a book set in
 *   anything but Courier found nothing at all, which reads as the search being
 *   broken rather than as a fact about typography.
 * * **Accents.** Someone typing "resume" means to find "résumé". Decomposing
 *   and dropping the combining marks makes both sides the same word.
 * * **Soft hyphens.** A word broken across a line keeps a U+00AD in the
 *   extracted text, so "typography" split at the margin is two words to an
 *   exact match and one word to a reader.
 *
 * `origin` is what keeps the answer usable: folding changes lengths, so a hit
 * at index *i* in the folded text has to be translated back to the offset in
 * the page's real text before it can be turned into a run and a DOM range.
 *
 * Case is the one part of this the reader can turn off. The other three are
 * not offered as choices because nobody types a soft hyphen on purpose.
 */
function fold(input: string, caseSensitive = false): { text: string; origin: number[] } {
  let text = "";
  const origin: number[] = [];

  for (let i = 0; i < input.length; i++) {
    const source = i;
    // NFKD splits the ligatures into their letters and the accented letters
    // into a letter plus its marks; the marks are then dropped. Done a
    // character at a time so that every piece of the result knows which
    // character of the original it came from.
    const decomposed = input[i].normalize("NFKD");
    const pieces = caseSensitive ? decomposed : decomposed.toLowerCase();
    for (const piece of pieces) {
      if (COMBINING.test(piece) || IGNORED.test(piece)) continue;
      text += piece;
      origin.push(source);
    }
  }
  // One past the end, so a match that runs to the last character has somewhere
  // to point its end at.
  origin.push(input.length);
  return { text, origin };
}

/** Combining marks, which are what is left of an accent after NFKD. */
const COMBINING = /[\u0300-\u036f\u1ab0-\u1aff\u1dc0-\u1dff\u20d0-\u20f0\ufe20-\ufe2f]/;
/** Characters that are in the text but not in the word: the soft hyphen, and
    the zero-width joiners that some producers scatter through it. */
const IGNORED = /[\u00ad\u200b-\u200d\ufeff]/;

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

function locate(
  page: PageText,
  needle: string,
  number: number,
  wholeWords = false,
): Match[] {
  const found: Match[] = [];
  let at = page.text.indexOf(needle);
  while (at !== -1) {
    if (wholeWords && !standsAlone(page.text, at, at + needle.length)) {
      // A rejected hit only moves the search on by one: "and" inside "understand"
      // is not a word, but the "and" that ends it is, and it starts one
      // character later.
      at = page.text.indexOf(needle, at + 1);
      continue;
    }
    // Back from the folded text to the page's own, which is what the runs and
    // the text layer are indexed by. The end has to clear the last character
    // it matched: one source character can fold to several — "ﬁ" is two — so
    // a match ending inside a ligature would otherwise start and end on the
    // same character and highlight nothing.
    const last = at + needle.length - 1;
    const start = position(page, page.origin[at]);
    const end = position(page, Math.max(page.origin[at + needle.length], page.origin[last] + 1));
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

/** Letters, digits and the underscore: what "whole words" counts as being part
    of a word, in every alphabet rather than only the Latin one. */
const WORD = /[\p{L}\p{N}_]/u;

/** Whether a match has something other than a word character on each side.
 *
 * The test is done against the folded text, which is the point of doing it
 * here rather than against the page: a word hyphenated across a line break
 * has already had the soft hyphen taken out of it, so "typo-graphy" at the
 * margin is one whole word by the time this sees it, as it is to a reader. */
function standsAlone(text: string, start: number, end: number): boolean {
  if (start > 0 && WORD.test(text[start - 1])) return false;
  if (end < text.length && WORD.test(text[end])) return false;
  return true;
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
