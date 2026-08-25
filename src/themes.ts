/* A theme needs two colours and may name three more.
 *
 * Ink and paper are the required pair. Accent, link and the selection area can
 * each be given outright, and each has a derivation here for when it is not —
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
 * that was printed at 90% white is nearly invisible on paper, because two
 * bright greys are hard to tell apart; carried across to a dark theme by the
 * same fraction, it arrives as a light rule on a dark ground, which is the
 * easiest thing in the world to see. The hyperref boxes around cross-references
 * are the usual sighting — grey enough to ignore on the printed page, a cage
 * around every citation once the page turns dark.
 *
 * So the top of the ramp is compressed: anything this light is paper, and the
 * greys just below it are pulled most of the way there. It costs the faintest
 * eighth of the range, which is the part a reader was never meant to notice,
 * and it also flattens the off-white of a scan into the theme's own background
 * instead of leaving every scanned page a shade paler than the app around it.
 *
 * Kept as a level rather than a fraction because the blend path can only reach
 * it as a fill colour, and both paths have to walk the same curve to the same
 * rounding — `recolor.test.mjs` holds them to a level of each other.
 */
const WHITE_POINT = 235;

/**
 * Read a colour out of a theme file.
 *
 * Hex, in the four lengths anyone writes: `#abc`, `#abcd`, `#aabbcc`,
 * `#aabbccdd`. The two with an alpha channel are accepted and the alpha
 * dropped — a theme's colours are opaque, and a file that names one with an
 * alpha means the colour rather than the transparency. `readColor` below is
 * the same function for anyone who needs to know whether it worked.
 *
 * Every length is checked against the alphabet before it is read.
 * `parseInt("12345g", 16)` stops at the character it cannot read and returns
 * what it had, so the six-digit path used to answer `#12345g` with a
 * plausible-looking colour rather than with the fallback — which is the worst
 * of the three possible behaviours, because it is the one nobody notices.
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
 * A theme file is meant to be written by hand, or by something asked to write
 * one, and the thing it will get wrong is the notation: `steelblue`,
 * `rgb(30, 42, 59)`, a stray character in a hex string. Every one of those
 * fell through to a fallback — black for the ink, white for the paper — with
 * nothing said anywhere, so the file was wrong and the screen looked like a
 * bug in the app.
 *
 * Returns the names of the fields at fault, in the order they appear in a
 * file, so the reader can be told which line to look at.
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
 * A theme may name it outright; most do not, and this derivation is why they
 * do not have to. It has one job — to be visible behind the theme's own ink
 * without becoming the loudest thing on the page — so it is the accent pulled
 * most of the way back towards the paper, which keeps it in the palette and
 * out of the way. A theme with a very saturated accent will want to say so
 * itself.
 */
export function selectionArea(theme: Theme): Rgb {
  const pulled = mix(accentOf(theme), parseColor(theme.background, WHITE), isDarkTheme(theme) ? 0.62 : 0.72);
  return theme.selection_area ? parseColor(theme.selection_area, pulled) : pulled;
}

/**
 * The colour selected text itself is drawn in.
 *
 * The default is the inverse of the area behind it, channel by channel, which
 * is the one colour guaranteed to belong to the same choice the reader already
 * made: change the area and the ink on it follows. It is also the reason a
 * selection is legible at all now — before this, the wash went over the page
 * and the words under it kept whatever the printer gave them, which on a dark
 * theme meant reading grey through slate.
 *
 * The one thing an inverse cannot do is separate itself from a middle grey,
 * which inverts to another middle grey. So a derived colour that does not
 * clear 3:1 against its own background gives up and takes black or white,
 * whichever reads. A theme that wants something else says so.
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
  // to be read.
  set("--text-note", toHex(mix(text, bg, 0.38)));
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

  // The chips on the toolbar.
  //
  // The bar takes the paper's colour rather than the surface's, because it
  // belongs to the document instead of floating over it — and until now the
  // things inside it did not follow: a hover, a held-down button, the zoom
  // group and the page field were all derived from `--surface`, which comes
  // off the backdrop. On a warm theme that is a cold chip on warm paper, and
  // on a theme whose paper is not its background it is a chip from another
  // theme entirely. So the bar gets a family of its own, mixed from the paper
  // it sits on towards the ink the reader is already looking at.
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
 * Map a rendered page onto the theme's two colours.
 *
 * The page comes out of pdf.js as ink on paper. Flatten it to luminance, then
 * stretch that single channel between the theme's text and background colour:
 * black ink lands on the text colour, white paper on the background, and
 * everything in between keeps its relative weight, so a grey rule stays a grey
 * rule instead of vanishing. `WHITE_POINT` is the one departure from that —
 * the palest greys are paper, not ink.
 *
 * Doing this with canvas blend modes keeps the whole thing on the GPU and,
 * more importantly, bakes the result into the bitmap — scrolling afterwards
 * costs nothing, which a CSS filter over every page could not promise.
 */
export function recolor(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  theme: Theme,
  regions?: Rect[],
): void {
  if (!theme.recolor) return;
  const bg = parseColor(theme.background, WHITE);
  const text = parseColor(theme.text, BLACK);
  const inverted = luminance(text) > luminance(bg);

  if (!canBlend()) {
    recolorByPixel(ctx, width, height, text, bg, regions);
    return;
  }

  ctx.save();
  ctx.globalCompositeOperation = "saturation";
  ctx.fillStyle = "#808080";
  ctx.fillRect(0, 0, width, height);

  // The white point, and the only bend in an otherwise straight ramp.
  // `color-dodge` against a constant is a division: the grey is scaled up by
  // 255 / WHITE_POINT and everything that reaches white stays there. Black is
  // dodge's fixed point, so full-strength ink is untouched by it.
  ctx.globalCompositeOperation = "color-dodge";
  ctx.fillStyle = toHex(grey(255 - WHITE_POINT));
  ctx.fillRect(0, 0, width, height);

  if (inverted) {
    // Light text on a dark page: flip the greyscale first, so the multiply
    // below has a positive range to work with.
    ctx.globalCompositeOperation = "difference";
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, width, height);
  }

  const [from, to] = inverted ? [bg, text] : [text, bg];
  ctx.globalCompositeOperation = "multiply";
  ctx.fillStyle = toHex([to[0] - from[0], to[1] - from[1], to[2] - from[2]]);
  ctx.fillRect(0, 0, width, height);

  ctx.globalCompositeOperation = "lighter";
  ctx.fillStyle = toHex(from);
  ctx.fillRect(0, 0, width, height);
  ctx.restore();
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
 * The same mapping, done a pixel at a time.
 *
 * Slower than the blend modes by a good margin — a full page is tens of
 * milliseconds rather than one or two — but it is only reached where the fast
 * path would have quietly produced the wrong picture, and a page that takes a
 * moment longer beats a dark theme that does not work.
 */
function recolorByPixel(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  text: Rgb,
  bg: Rgb,
  regions?: Rect[],
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

  const mask = regions?.length ? maskFor(regions, originX, originY, spanX, spanY) : null;

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
  for (let row = 0; row < spanY; row++) {
    const rowStart = row * spanX;
    for (let column = 0; column < spanX; column++) {
      // Overlapping rectangles would otherwise be run through the ramp twice,
      // which is a different colour rather than the same one.
      if (mask && !mask[rowStart + column]) continue;
      const i = (rowStart + column) * 4;
      // Rec. 601 luma, which is what the `saturation` + greyscale path amounts
      // to and is cheap in integers. The half added before the shift is what
      // rounds it rather than flooring it: the white point multiplies whatever
      // the two paths disagree about by 255 / WHITE_POINT, and half a level of
      // truncation here came out the other end as two.
      const level =
        (pixels[i] * 77 + pixels[i + 1] * 151 + pixels[i + 2] * 28 + 128) >> 8;
      const at = level * 3;
      pixels[i] = ramp[at];
      pixels[i + 1] = ramp[at + 1];
      pixels[i + 2] = ramp[at + 2];
    }
  }
  ctx.putImageData(image, originX, originY);
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
