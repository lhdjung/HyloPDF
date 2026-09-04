/* A theme needs two colours and may name three more.
 *
 * Ink and paper are the required pair. The accent, the link colour and the two
 * selection colours can each be given outright, and each has a derivation here
 * for when it is not —
 * as does every shade the chrome uses and never asks about: the toolbar, the
 * borders, the muted text, the shadow under a page. So a five-line TOML file
 * is genuinely enough to describe a whole look, and a longer one is only ever
 * a theme disagreeing with a default. */

import type { Theme } from "./api";

type Rgb = [number, number, number];

const BLACK: Rgb = [0, 0, 0];
const WHITE: Rgb = [255, 255, 255];
/** The two greens confirmation is drawn in: one for pale paper, one for dark. */
const GREEN_DARK: Rgb = [0x3d, 0x8f, 0x5b];
const GREEN_LIGHT: Rgb = [0x6c, 0xc0, 0x8b];
const RED_DARK: Rgb = [0xb0, 0x2a, 0x37];
const RED_LIGHT: Rgb = [0xd9, 0x63, 0x6b];

/**
 * Where paper begins, on the 0–255 grey the recolouring reads the page as.
 *
 * A straight ramp is fair to ink and unfair to the absence of it. A hairline
 * printed at 90% white is nearly invisible on paper; carried across by the same
 * fraction it arrives as a light rule on a dark ground, which is the easiest
 * thing in the world to see. The hyperref boxes around cross-references are the
 * usual sighting — ignorable in print, a cage around every citation once the
 * page turns dark.
 *
 * So the top of the ramp is compressed: anything this light is paper. It costs
 * the faintest eighth of the range, which a reader was never meant to notice,
 * and it flattens the off-white of a scan into the theme's own background
 * instead of leaving every scanned page a shade paler than the app.
 *
 * A level rather than a fraction, because the blend path can only reach it as a
 * fill colour and both paths have to walk the same curve to the same rounding.
 */
const WHITE_POINT = 235;

/**
 * How much colour a pixel needs before the theme lets it keep any, and how
 * much before it keeps all of it — chroma, on the same 0–255 scale.
 *
 * The floor tells a colour apart from a cast: scanned paper is never quite
 * grey and a JPEG's flat areas carry a couple of levels of chroma noise, and
 * neither is a colour anybody chose. Below it a pixel is neutral and takes the
 * theme's ink and paper, which keeps a scanned page exactly the background of
 * the app. Above the ceiling it is plainly coloured and keeps its hue outright.
 * Between them it fades, so an antialiased edge is not a rim of colour around a
 * themed letter.
 */
const COLOUR_FLOOR = 12;
const COLOUR_FULL = 32;

/**
 * Where the colour is: the question the two paths hang off.
 *
 * A strip of page with nothing but type on it comes out the same whichever path
 * draws it and the blend chain is twenty times quicker, so the pixels are only
 * walked where there is a colour to keep. The answer comes off a downscale — 
 * cells of about `PROBE_CELL` pixels, averaged by the engine — so asking costs
 * one `drawImage` rather than a read of ten million pixels. Asked by the row,
 * because a row is a rectangle and a rectangle is what both paths take.
 *
 * Averaging dilutes: a blue curve a few pixels wide crossing a cell of white
 * arrives as a few levels of chroma. That is why the floor here is far below
 * `COLOUR_FLOOR` and nothing is decided by it — a row over it is only a row
 * worth reading properly. Guessing wrong the other way costs a figure its
 * colours, so the rows either side of a hit are taken too and near rows joined.
 */
const PROBE_CELL = 12;
const PROBE_CELLS_MAX = 256;
const PROBE_FLOOR = 3;
const PROBE_JOIN = 2;

/**
 * Read a colour out of a theme file.
 *
 * Hex, in the four lengths anyone writes: `#abc`, `#abcd`, `#aabbcc`,
 * `#aabbccdd`. The two with an alpha are accepted and the alpha dropped.
 * `readColor` below is the same function saying `null` instead of guessing.
 *
 * Every length is checked against the alphabet before it is read:
 * `parseInt("12345g", 16)` stops at the character it cannot read and returns
 * what it had, so `#12345g` came back as a plausible colour from a string that
 * is not one — the worst of the three possible behaviours, because nobody
 * notices it.
 */
export function parseColor(value: string, fallback: Rgb = BLACK): Rgb {
  return readColor(value) ?? fallback;
}

/** The same, but saying so when it cannot: `null` rather than a guess, which
    is what lets a theme with an unreadable colour be reported instead of
    quietly rendering as black on white. */
export function readColor(value: string): Rgb | null {
  const hex = value.trim().replace(/^#/, "");
  if (!/^(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(hex)) return null;
  // The short forms double each digit; the alpha, where there is one, is read
  // and thrown away.
  const full = hex.length <= 4 ? [...hex].map((c) => c + c).join("") : hex;
  return [
    parseInt(full.slice(0, 2), 16),
    parseInt(full.slice(2, 4), 16),
    parseInt(full.slice(4, 6), 16),
  ];
}

/**
 * Which of a theme's colours could not be read.
 *
 * A theme file is meant to be written by hand, and the notation is what will be
 * got wrong: `steelblue`, `rgb(30, 42, 59)`, a stray character in a hex string.
 * Each fell through to a fallback — black ink, white paper — with nothing said,
 * so the file was wrong and the screen looked like a bug in the app.
 *
 * Returns the fields at fault, in the order they appear in a file.
 */
export function unreadableColors(theme: Theme): string[] {
  const named: [string, string | null][] = [
    ["text", theme.text],
    ["background", theme.background],
    ["accent", theme.accent],
    ["link", theme.link],
    ["selection_area", theme.selection_area],
    ["selection_text", theme.selection_text],
  ];
  return named
    .filter(([, value]) => value !== null && value !== "" && readColor(value) === null)
    .map(([name]) => name);
}

export function toHex([r, g, b]: Rgb): string {
  const part = (v: number) =>
    Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0");
  return `#${part(r)}${part(g)}${part(b)}`;
}

/** A colour with nothing in it but a level, for the fills that carry one. */
function grey(level: number): Rgb {
  return [level, level, level];
}

function mix(a: Rgb, b: Rgb, t: number): Rgb {
  return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
}

/** Relative luminance, the WCAG definition. */
export function luminance([r, g, b]: Rgb): number {
  const channel = (v: number) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

export function isDarkTheme(theme: Theme): boolean {
  return luminance(parseColor(theme.background, WHITE)) < 0.35;
}

export function contrastRatio(a: Rgb, b: Rgb): number {
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (high + 0.05) / (low + 0.05);
}

/** The accent a theme works with, named or derived. */
export function accentOf(theme: Theme): Rgb {
  const fallback = mix(parseColor(theme.text, BLACK), parseColor(theme.background, WHITE), 0.3);
  return theme.accent ? parseColor(theme.accent, fallback) : fallback;
}

/**
 * What sits behind selected text.
 *
 * A theme may name it outright; most do not. It has one job — to be visible
 * behind the theme's own ink without becoming the loudest thing on the page —
 * so it is the accent pulled most of the way back towards the paper. A theme
 * with a very saturated accent will want to say so itself.
 */
export function selectionArea(theme: Theme): Rgb {
  const pulled = mix(accentOf(theme), parseColor(theme.background, WHITE), isDarkTheme(theme) ? 0.62 : 0.72);
  return theme.selection_area ? parseColor(theme.selection_area, pulled) : pulled;
}

/**
 * The colour selected text itself is drawn in.
 *
 * The default is the inverse of the area behind it, channel by channel, which
 * is the one colour guaranteed to follow the choice the reader already made.
 *
 * What an inverse cannot do is separate itself from a middle grey, which
 * inverts to another middle grey — so a derived colour that does not clear 3:1
 * against its own background takes black or white, whichever reads.
 */
export function selectionInk(theme: Theme): Rgb {
  const area = selectionArea(theme);
  const inverse: Rgb = [255 - area[0], 255 - area[1], 255 - area[2]];
  const derived = contrastRatio(inverse, area) >= 3
    ? inverse
    : luminance(area) > 0.18
      ? BLACK
      : WHITE;
  return theme.selection_text ? parseColor(theme.selection_text, derived) : derived;
}

/**
 * What a saved highlight's wash looks like on a recolouring theme.
 *
 * The colour written to `/C` is never changed — that is what makes a highlight
 * "the red one" on every theme and red again in Preview. This is narrower: a
 * wash is translucent paint and `/CA` is the alpha it was laid down at, so
 * mixing the raw colour with the theme's paper by that fraction is the same
 * paint on this theme's page rather than a guess. It cannot be left to the
 * luminance ramp — a conventional wash lands past `WHITE_POINT` and the ramp
 * calls it paper. See `markup-assessment.md`, "the trap".
 */
export function markupWashColor(theme: Theme, color: string, opacity: number): string {
  const bg = parseColor(theme.background, WHITE);
  const raw = parseColor(color, BLACK);
  const alpha = Math.min(1, Math.max(0, opacity));
  return toHex(mix(raw, bg, 1 - alpha));
}

/** Apply a theme to the app chrome. The document itself is recoloured at
    render time — see `recolor` below. */
export function applyTheme(theme: Theme): void {
  const bg = parseColor(theme.background, WHITE);
  const text = parseColor(theme.text, BLACK);
  const dark = luminance(bg) < 0.35;
  const accent = accentOf(theme);

  const backdrop = mix(bg, BLACK, dark ? 0.34 : 0.07);
  const surface = mix(bg, WHITE, dark ? 0.06 : 0.55);
  const set = (name: string, value: string) =>
    document.documentElement.style.setProperty(name, value);

  set("--bg", toHex(backdrop));
  set("--surface", toHex(surface));
  set("--surface-hover", toHex(mix(surface, text, 0.09)));
  set("--surface-sunk", toHex(mix(surface, text, 0.055)));
  set("--line", toHex(mix(bg, text, dark ? 0.14 : 0.17)));
  set("--text", toHex(text));
  set("--text-soft", toHex(mix(text, bg, 0.26)));
  // The small print beside a setting: quieter than the label, but still meant
  // to be read, which is why it is only a little quieter. At 0.38 it fell
  // under 4.5:1 against the paper on a light theme, and the sentence that
  // explains a switch was harder to read than the switch.
  set("--text-note", toHex(mix(text, bg, 0.28)));
  set("--text-faint", toHex(mix(text, bg, 0.52)));
  set("--accent", toHex(accent));
  // The colour links take. The document itself is tinted at render time; this
  // is for the app's own chrome — the ring under the pointer, the swatch in a
  // theme card.
  set("--link", toHex(theme.link ? parseColor(theme.link, accent) : accent));
  set("--accent-soft", toHex(mix(accent, surface, dark ? 0.8 : 0.86)));
  // Selection is two colours, and both of them are derived unless the theme
  // says otherwise — see `selectionArea` and `selectionInk`.
  set("--selection-area", toHex(selectionArea(theme)));
  set("--selection-text", toHex(selectionInk(theme)));
  set(
    "--accent-contrast",
    toHex(contrastRatio(accent, WHITE) >= 3 ? WHITE : mix(accent, BLACK, 0.82)),
  );
  // "That worked": a green that reads on this theme's surface, pulled a little
  // towards the theme's own ink so it belongs to the palette rather than
  // arriving from somewhere else.
  set("--positive", toHex(mix(dark ? GREEN_LIGHT : GREEN_DARK, text, 0.14)));
  // The pair a filled "danger zone" button needs — same shape as
  // `--accent`/`--accent-contrast`, but a red that reads as destructive
  // regardless of the theme's own accent.
  const negative = dark ? RED_LIGHT : RED_DARK;
  set("--negative", toHex(negative));
  set(
    "--negative-contrast",
    toHex(contrastRatio(negative, WHITE) >= 3 ? WHITE : mix(negative, BLACK, 0.82)),
  );
  // The paper behind a page that has not finished rendering: the colour it is
  // about to become, so nothing flashes white on the way in.
  const paper = theme.recolor ? bg : WHITE;
  set("--page-paper", toHex(paper));

  // The chips on the toolbar, which are a family of their own because the bar
  // takes the *paper's* colour rather than the surface's — it belongs to the
  // document instead of floating over it. Derived from `--surface` they were a
  // cold chip on warm paper, and on a theme whose paper is not its background,
  // a chip from another theme entirely.
  const paperDark = luminance(paper) < 0.35;
  // Which ink that is: the theme's own, almost always, because that is what
  // carries the tint. A theme may name a text colour its paper cannot support
  // — a dark theme that leaves the document alone shows its chrome on white —
  // and a field nobody can see is worse than one that is merely grey.
  const chipInk = contrastRatio(text, paper) >= 3 ? text : paperDark ? WHITE : BLACK;
  set("--bar-hover", toHex(mix(paper, chipInk, paperDark ? 0.13 : 0.09)));
  set("--bar-sunk", toHex(mix(paper, chipInk, paperDark ? 0.075 : 0.055)));
  set("--bar-line", toHex(mix(paper, chipInk, paperDark ? 0.2 : 0.17)));
  // What a button on the bar wears while it is holding something open. The
  // same idea as `--accent-soft` and the same accent, over the bar's paper
  // instead of the surface.
  set("--bar-accent", toHex(mix(accent, paper, paperDark ? 0.8 : 0.86)));
  set(
    "--page-shadow",
    dark
      ? "0 1px 3px rgba(0, 0, 0, 0.5), 0 10px 30px rgba(0, 0, 0, 0.42)"
      : "0 1px 3px rgba(0, 0, 0, 0.09), 0 8px 24px rgba(0, 0, 0, 0.07)",
  );
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
}

/**
 * Map a rendered page onto the theme, keeping the colours the page has.
 *
 * The page comes out of pdf.js as ink on paper and a theme is two colours to
 * put in their place. For everything printed in grey that is the whole story:
 * flatten to luminance and stretch that channel between the theme's text and
 * background, so a grey rule stays a grey rule instead of vanishing.
 * `WHITE_POINT` is the one departure — the palest greys are paper, not ink.
 *
 * A figure is not printed in grey, and flattening one throws away what it is
 * for: four curves that were blue, orange, green and red arrive told apart by
 * nothing. So a pixel with a colour of its own keeps it, and what the theme
 * moves is its *lightness* — on a dark theme a dark blue curve becomes a light
 * blue one, the same way black type becomes white.
 *
 * The two are one mapping, not two: colour is kept in proportion to how much
 * of it there is, so ink with none of it lands exactly where it always did and
 * nothing about a page of type changes. `COLOUR_FLOOR` is where that begins.
 *
 * Which path draws which part of it is a performance question and nothing else
 * — see `colouredRows`. The blend chain keeps the whole thing on the GPU and,
 * more importantly, bakes the result into the bitmap: scrolling afterwards
 * costs nothing, which a CSS filter over every page could not promise.
 */
export function recolor(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  theme: Theme,
): void {
  if (!theme.recolor) return;
  const bg = parseColor(theme.background, WHITE);
  const text = parseColor(theme.text, BLACK);
  if (!canBlend()) {
    recolorByPixel(ctx, width, height, text, bg, undefined, true);
    return;
  }
  const coloured = colouredRows(ctx, width, height);
  // The rows that have a colour on them are walked pixel by pixel, and the
  // rest of the page — most of most pages — goes down the blend chain, which
  // does the same thing to a grey and does it on the GPU.
  flattenByBlend(ctx, width, height, text, bg, gapsBetween(coloured, width, height));
  for (const band of coloured) {
    recolorByPixel(ctx, width, height, text, bg, [band], true);
  }
}

/**
 * The same ramp with the colour taken out: everything onto two colours.
 *
 * This is what a link and a selected word want. Both are saying *this part of
 * the page is different*, in a colour the theme chose for the purpose, and a
 * link that was printed blue keeping its blue is a link that says nothing. So
 * they flatten, where a page keeps what it has.
 *
 * `regions` is not an optimisation: where the blend path is bounded by the
 * caller's clipping path, the pixel path has to be told, because `putImageData`
 * is the one drawing operation that ignores a clip.
 */
export function duotone(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  theme: Theme,
  regions?: Rect[],
): void {
  const bg = parseColor(theme.background, WHITE);
  const text = parseColor(theme.text, BLACK);
  if (!canBlend()) {
    recolorByPixel(ctx, width, height, text, bg, regions, false);
    return;
  }
  flattenByBlend(ctx, width, height, text, bg, null);
}

/** The luminance ramp, drawn with composite operations: a chain of five fills
    over the whole canvas — or over `parts` of it, where something else is
    drawing the rest — bounded by whatever clip the caller has set. */
function flattenByBlend(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  text: Rgb,
  bg: Rgb,
  parts: Rect[] | null,
): void {
  const inverted = luminance(text) > luminance(bg);
  const fill = (style: string) => {
    ctx.fillStyle = style;
    if (!parts) ctx.fillRect(0, 0, width, height);
    else for (const part of parts) ctx.fillRect(part.x, part.y, part.w, part.h);
  };

  ctx.save();
  ctx.globalCompositeOperation = "saturation";
  fill("#808080");

  // The white point, and the only bend in an otherwise straight ramp.
  // `color-dodge` against a constant is a division: the grey is scaled up by
  // 255 / WHITE_POINT and everything that reaches white stays there. Black is
  // dodge's fixed point, so full-strength ink is untouched by it.
  ctx.globalCompositeOperation = "color-dodge";
  fill(toHex(grey(255 - WHITE_POINT)));

  if (inverted) {
    // Light text on a dark page: flip the greyscale first, so the multiply
    // below has a positive range to work with.
    ctx.globalCompositeOperation = "difference";
    fill("#ffffff");
  }

  const [from, to] = inverted ? [bg, text] : [text, bg];
  ctx.globalCompositeOperation = "multiply";
  fill(toHex([to[0] - from[0], to[1] - from[1], to[2] - from[2]]));

  ctx.globalCompositeOperation = "lighter";
  fill(toHex(from));
  ctx.restore();
}

/**
 * Which rows of this page have a colour on them.
 *
 * Full-width bands in canvas pixels, top to bottom, none of them touching. An
 * empty list means a page of type, which the blend chain draws identically and
 * far faster; a page that cannot be read at all is answered with the whole of
 * it, because losing a figure's colours is the worse of the two mistakes.
 *
 * See `PROBE_CELL` for why this is a downscale and what the floor is worth.
 */
let probeCanvas: HTMLCanvasElement | null = null;

function colouredRows(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
): Rect[] {
  const whole = [{ x: 0, y: 0, w: width, h: height }];
  const across = Math.max(1, Math.min(PROBE_CELLS_MAX, Math.round(width / PROBE_CELL)));
  const down = Math.max(1, Math.min(PROBE_CELLS_MAX, Math.round(height / PROBE_CELL)));
  try {
    const canvas = (probeCanvas ??= document.createElement("canvas"));
    canvas.width = across;
    canvas.height = down;
    const small = canvas.getContext("2d", { alpha: false, willReadFrequently: true });
    if (!small) return whole;
    // The quality is load-bearing, not a nicety. Averaging is the whole premise
    // of reading a page this small — it is what makes a line one pixel thick
    // show up as a few levels of chroma across its cell — and `low`, which is
    // the default, samples instead: measured on a page carrying a 1px rule, a
    // 2px rule and a plotted curve, it found the curve and neither rule.
    // `medium` found all three and costs a couple of milliseconds over `low`;
    // `high` costs six more than that and turns up a tenth more rows, all of
    // them fainter than anything the eye was going to miss.
    small.imageSmoothingQuality = "medium";
    small.drawImage(ctx.canvas, 0, 0, width, height, 0, 0, across, down);
    const cells = small.getImageData(0, 0, across, down).data;

    const bands: Rect[] = [];
    const cell = height / down;
    for (let row = 0; row < down; row++) {
      let coloured = false;
      for (let column = 0; column < across && !coloured; column++) {
        const i = (row * across + column) * 4;
        const r = cells[i];
        const g = cells[i + 1];
        const b = cells[i + 2];
        const high = r > g ? (r > b ? r : b) : g > b ? g : b;
        const low = r < g ? (r < b ? r : b) : g < b ? g : b;
        coloured = high - low >= PROBE_FLOOR;
      }
      if (!coloured) continue;
      // A cell either side, and joined to the band above if the gap between
      // them is not worth the two extra reads of the canvas.
      const top = Math.max(0, Math.floor((row - 1) * cell));
      const bottom = Math.min(height, Math.ceil((row + 2) * cell));
      const last = bands[bands.length - 1];
      if (last && top - (last.y + last.h) <= PROBE_JOIN * cell) {
        last.h = bottom - last.y;
      } else {
        bands.push({ x: 0, y: top, w: width, h: bottom - top });
      }
    }
    return bands;
  } catch {
    return whole;
  }
}

/** The rest of the page: the full-width bands the given ones leave over. */
function gapsBetween(bands: Rect[], width: number, height: number): Rect[] {
  const gaps: Rect[] = [];
  let y = 0;
  for (const band of bands) {
    if (band.y > y) gaps.push({ x: 0, y, w: width, h: band.y - y });
    y = band.y + band.h;
  }
  if (y < height) gaps.push({ x: 0, y, w: width, h: height - y });
  return gaps;
}

/** Whether this engine really does the blend modes `recolor` is built on.
 *
 * `saturation` is a non-separable blend mode, and support for it on a canvas
 * has been uneven — WebKitGTK, which is the engine on Linux, is the one we
 * have the least visibility into. A dropped blend mode does not throw; it
 * silently does nothing, and the page comes out as printed under a theme that
 * was meant to recolour it, which reads as the theme being broken.
 *
 * The check is the standard one: an unsupported value is refused, and the
 * property keeps whatever it had before. Asked once, kept thereafter.
 *
 * All five of them, not three. `difference` and `lighter` are only on the
 * inverted path — light ink on dark paper — so they were left out, and an
 * engine that dropped either would have gone down the fast path and produced
 * a page inverted rather than recoloured. That is the silent-wrong-picture
 * failure this function exists to prevent, so the list has to be the whole
 * list. Two of them are separable and one is not even a blend mode, so this
 * costs nothing and closes the gap. */
const BLEND_MODES = [
  "saturation",
  "color-dodge",
  "difference",
  "multiply",
  "lighter",
] as const;

let blendable: boolean | null = null;

function canBlend(): boolean {
  if (blendable !== null) return blendable;
  try {
    const probe = document.createElement("canvas").getContext("2d");
    if (!probe) return (blendable = false);
    return (blendable = BLEND_MODES.every((mode) => {
      probe.globalCompositeOperation = mode;
      return probe.globalCompositeOperation === mode;
    }));
  } catch {
    return (blendable = false);
  }
}

/**
 * The whole mapping, done a pixel at a time.
 *
 * This is the only path that can keep a colour — the blend chain flattens by
 * construction — and it is also the fallback for an engine that will not do
 * the blend modes at all. `keepColour` is the difference: with it off the ramp
 * is all there is, which is the answer for a link, for a selected word, and
 * for a page that has no colour on it in the first place.
 *
 * Slower than the blend modes by a good margin — a full page is tens of
 * milliseconds rather than one or two — which is what `hasColour` exists to
 * spare a page of type.
 */
function recolorByPixel(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  text: Rgb,
  bg: Rgb,
  regions: Rect[] | undefined,
  keepColour: boolean,
): void {
  // `regions` is not an optimisation. `putImageData` is the one drawing
  // operation that ignores the clipping path, so where the blend path relies
  // on a clip to reach only part of the page — colouring the links — this path
  // has to be told which part, or it would recolour the whole page a second
  // time and leave nothing of it.
  const area = regions?.length ? bounds(regions, width, height) : null;
  if (area && (area.w <= 0 || area.h <= 0)) return;
  const originX = area ? area.x : 0;
  const originY = area ? area.y : 0;
  const spanX = area ? area.w : width;
  const spanY = area ? area.h : height;

  // One rectangle is its own bounding box, so every pixel read is inside it
  // and there is nothing to ask. Which is the common case at both ends: a band
  // of a page that has a colour on it, and a single link on a line.
  const mask =
    regions && regions.length > 1 ? maskFor(regions, originX, originY, spanX, spanY) : null;

  const image = ctx.getImageData(originX, originY, spanX, spanY);
  const pixels = image.data;
  // The same ramp the blend chain walks: flatten to a single channel, then
  // stretch it from the text colour at black to the background at white.
  const ramp = new Uint8ClampedArray(256 * 3);
  for (let level = 0; level < 256; level++) {
    // The white point, arrived at the way the dodge arrives at it: an 8-bit
    // canvas rounds after every composite, so rounding here too is what keeps
    // the two paths on the same level rather than a level apart.
    const t = Math.min(255, Math.round((level * 255) / WHITE_POINT)) / 255;
    ramp[level * 3] = text[0] + (bg[0] - text[0]) * t;
    ramp[level * 3 + 1] = text[1] + (bg[1] - text[1]) * t;
    ramp[level * 3 + 2] = text[2] + (bg[2] - text[2]) * t;
  }

  /* Four tables, and between them they are the whole of keeping a colour.
   *
   * A colour is its hue, its saturation and its lightness, and only the last
   * of those is the theme's business. Lightness here is HSL's — the midpoint
   * of the highest and lowest channel — because that is the one under which
   * hue and saturation can be put back without leaving the box: at any
   * lightness there is exactly `roomAt` of chroma available, and a colour
   * rescaled to the room at its new lightness lands inside it by construction.
   * No clipping, and so no hue quietly bending as a channel is clamped.
   *
   * Which lightness is new is the ramp's answer, and the ramp is read by luma
   * rather than by that midpoint, because luma is what says a yellow is light
   * and a blue is dark. So `mapped` is where a pixel of a given luma ends up,
   * and `room` is what fits there.
   */
  const mapped = new Uint8Array(256);
  const room = new Uint8Array(256);
  // The same question of the pixel that arrived, as a reciprocal: the chroma
  // it is being scaled out of. Zero at the two ends of the range, where a
  // pixel can have no chroma to scale.
  const inverseRoom = new Float32Array(256);
  // And, by chroma rather than by lightness, how much of its own colour a
  // pixel keeps.
  const share = new Float32Array(256);
  // Left at zero when there is no colour to keep, which is what makes the loop
  // below one loop: `keep` comes out zero for every pixel and the ramp is all
  // that happens.
  if (keepColour) {
    for (let level = 0; level < 256; level++) {
      const at = level * 3;
      const high = Math.max(ramp[at], ramp[at + 1], ramp[at + 2]);
      const low = Math.min(ramp[at], ramp[at + 1], ramp[at + 2]);
      mapped[level] = (high + low) >> 1;
      room[level] = roomAt(mapped[level]);
      inverseRoom[level] = roomAt(level) ? 1 / roomAt(level) : 0;
      share[level] =
        level <= COLOUR_FLOOR
          ? 0
          : level >= COLOUR_FULL
            ? 1
            : (level - COLOUR_FLOOR) / (COLOUR_FULL - COLOUR_FLOOR);
    }
  }

  for (let row = 0; row < spanY; row++) {
    const rowStart = row * spanX;
    for (let column = 0; column < spanX; column++) {
      // Overlapping rectangles would otherwise be run through the ramp twice,
      // which is a different colour rather than the same one.
      if (mask && !mask[rowStart + column]) continue;
      const i = (rowStart + column) * 4;
      const r = pixels[i];
      const g = pixels[i + 1];
      const b = pixels[i + 2];
      // Rec. 601 luma, which is what the `saturation` + greyscale path amounts
      // to and is cheap in integers. The half added before the shift is what
      // rounds it rather than flooring it: the white point multiplies whatever
      // the two paths disagree about by 255 / WHITE_POINT, and half a level of
      // truncation here came out the other end as two.
      const level = (r * 77 + g * 151 + b * 28 + 128) >> 8;
      const at = level * 3;
      const high = r > g ? (r > b ? r : b) : g > b ? g : b;
      const low = r < g ? (r < b ? r : b) : g < b ? g : b;
      // Anything this light is paper, whatever colour it is — the white point
      // again, and the reason a scan's warm cast does not survive as a tint on
      // a theme's own background. Below the floor there is no colour to keep,
      // which is every pixel of a page of type.
      const keep = level >= WHITE_POINT ? 0 : share[high - low];
      if (keep === 0) {
        pixels[i] = ramp[at];
        pixels[i + 1] = ramp[at + 1];
        pixels[i + 2] = ramp[at + 2];
        continue;
      }
      // Hue and saturation as they were, at the lightness the ramp asked for:
      // the channels keep their distances from the lowest of them, scaled by
      // the room at the new lightness against the room at the old.
      const scale = room[level] * inverseRoom[(high + low) >> 1];
      const foot = mapped[level] - ((high - low) * scale) / 2;
      pixels[i] = ramp[at] + (foot + (r - low) * scale - ramp[at]) * keep;
      pixels[i + 1] = ramp[at + 1] + (foot + (g - low) * scale - ramp[at + 1]) * keep;
      pixels[i + 2] = ramp[at + 2] + (foot + (b - low) * scale - ramp[at + 2]) * keep;
    }
  }
  ctx.putImageData(image, originX, originY);
}

/** How much chroma an HSL lightness has room for: all of it in the middle,
    none of it at either end, because black is black and white is white. */
function roomAt(lightness: number): number {
  return 255 - Math.abs(2 * lightness - 255);
}

/** A rectangle in canvas pixels. */
export type Rect = { x: number; y: number; w: number; h: number };

function bounds(regions: Rect[], width: number, height: number): Rect {
  let left = width;
  let top = height;
  let right = 0;
  let bottom = 0;
  for (const rect of regions) {
    left = Math.min(left, rect.x);
    top = Math.min(top, rect.y);
    right = Math.max(right, rect.x + rect.w);
    bottom = Math.max(bottom, rect.y + rect.h);
  }
  left = Math.max(0, Math.floor(left));
  top = Math.max(0, Math.floor(top));
  right = Math.min(width, Math.ceil(right));
  bottom = Math.min(height, Math.ceil(bottom));
  return { x: left, y: top, w: right - left, h: bottom - top };
}

/**
 * One byte a pixel, saying whether it falls inside any of the rectangles.
 *
 * Built once by filling each rectangle, rather than asked per pixel by
 * scanning the list. Asking cost rectangles × pixels, and the shape that makes
 * that bite is an ordinary one: a bibliography has links from the top of the
 * page to the bottom, so their bounding box is the whole page, and there are a
 * couple of hundred of them. Twelve million pixels against two hundred
 * rectangles is two and a half billion tests for one repaint — on the path an
 * engine without blend modes takes for every step of a zoom. Filling costs the
 * sum of the rectangles' own areas, which is the ink actually being coloured.
 *
 * Rounded outwards, the way `bounds` rounds, so a rectangle that lands on a
 * fraction colours the pixel it touches rather than stopping short of it.
 */
function maskFor(
  regions: Rect[],
  originX: number,
  originY: number,
  spanX: number,
  spanY: number,
): Uint8Array {
  const mask = new Uint8Array(spanX * spanY);
  for (const rect of regions) {
    const left = Math.max(0, Math.floor(rect.x) - originX);
    const right = Math.min(spanX, Math.ceil(rect.x + rect.w) - originX);
    const top = Math.max(0, Math.floor(rect.y) - originY);
    const bottom = Math.min(spanY, Math.ceil(rect.y + rect.h) - originY);
    if (right <= left) continue;
    for (let row = top; row < bottom; row++) {
      mask.fill(1, row * spanX + left, row * spanX + right);
    }
  }
  return mask;
}

/**
 * Put the pictures back. `coordinates` is pdf.js's record of where images
 * landed on the canvas: six numbers per image, three corners of a
 * parallelogram in fractions of the canvas. The fourth corner follows.
 */
export function restoreImages(
  ctx: CanvasRenderingContext2D,
  pristine: CanvasImageSource,
  width: number,
  height: number,
  coordinates: ArrayLike<number>,
): void {
  if (coordinates.length < 6) return;

  // Every picture goes into one path, and the page is put back once.
  //
  // Clipping and drawing per picture cost a full-canvas `drawImage` each time,
  // so the work was pictures × pixels rather than pixels. That is fine for a
  // photograph or two and ruinous for a page of typeset mathematics, where
  // every formula is its own small image: two hundred of them on a page meant
  // two hundred redraws of a canvas that can be twelve million pixels, and the
  // page simply stopped. The subpaths union under the default winding rule, so
  // one clip covers exactly what all of them covered.
  ctx.save();
  ctx.beginPath();
  for (let i = 0; i + 5 < coordinates.length; i += 6) {
    const ax = coordinates[i] * width;
    const ay = coordinates[i + 1] * height;
    const bx = coordinates[i + 2] * width;
    const by = coordinates[i + 3] * height;
    const cx = coordinates[i + 4] * width;
    const cy = coordinates[i + 5] * height;
    // Three corners of a parallelogram; the fourth follows.
    ctx.moveTo(ax, ay);
    ctx.lineTo(bx, by);
    ctx.lineTo(cx + (bx - ax), cy + (by - ay));
    ctx.lineTo(cx, cy);
    ctx.closePath();
  }
  ctx.clip();
  ctx.drawImage(pristine, 0, 0, width, height);
  ctx.restore();
}
