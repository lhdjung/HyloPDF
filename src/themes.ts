/* A theme names two colours. Everything else — the toolbar, the borders, the
   muted text, the shadow under a page — is derived from those two, so that a
   five-line TOML file is genuinely enough to describe a whole look. */

import type { Theme } from "./api";

type Rgb = [number, number, number];

const BLACK: Rgb = [0, 0, 0];
const WHITE: Rgb = [255, 255, 255];
/** The two greens confirmation is drawn in: one for pale paper, one for dark. */
const GREEN_DARK: Rgb = [0x3d, 0x8f, 0x5b];
const GREEN_LIGHT: Rgb = [0x6c, 0xc0, 0x8b];

export function parseColor(value: string, fallback: Rgb = BLACK): Rgb {
  const hex = value.trim().replace(/^#/, "");
  if (hex.length === 3) {
    const [r, g, b] = [...hex].map((c) => parseInt(c + c, 16));
    return [r, g, b].some(Number.isNaN) ? fallback : ([r, g, b] as Rgb);
  }
  if (hex.length === 6) {
    const n = parseInt(hex, 16);
    if (Number.isNaN(n)) return fallback;
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }
  return fallback;
}

export function toHex([r, g, b]: Rgb): string {
  const part = (v: number) =>
    Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0");
  return `#${part(r)}${part(g)}${part(b)}`;
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

/** Apply a theme to the app chrome. The document itself is recoloured at
    render time — see `recolor` below. */
export function applyTheme(theme: Theme): void {
  const bg = parseColor(theme.background, WHITE);
  const text = parseColor(theme.text, BLACK);
  const dark = luminance(bg) < 0.35;
  const accent = theme.accent
    ? parseColor(theme.accent, mix(text, bg, 0.3))
    : mix(text, bg, 0.3);

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
  // What sits behind selected text.
  //
  // A theme may name it outright; most do not, and the derivation below is why
  // they do not have to. It has one job — to be visible behind the theme's own
  // ink without becoming the loudest thing on the page — so it is the accent
  // pulled most of the way back towards the paper, which keeps it in the
  // palette and out of the way. A theme with a very saturated accent will want
  // to say so itself.
  const selection = theme.selection
    ? parseColor(theme.selection, mix(accent, bg, dark ? 0.62 : 0.72))
    : mix(accent, bg, dark ? 0.62 : 0.72);
  set("--selection", toHex(selection));
  set(
    "--accent-contrast",
    toHex(contrastRatio(accent, WHITE) >= 3 ? WHITE : mix(accent, BLACK, 0.82)),
  );
  // "That worked": a green that reads on this theme's surface, pulled a little
  // towards the theme's own ink so it belongs to the palette rather than
  // arriving from somewhere else.
  set("--positive", toHex(mix(dark ? GREEN_LIGHT : GREEN_DARK, text, 0.14)));
  // The paper behind a page that has not finished rendering: the colour it is
  // about to become, so nothing flashes white on the way in.
  set("--page-paper", theme.recolor ? toHex(bg) : "#ffffff");
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
 * rule instead of vanishing.
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
 * property keeps whatever it had before. Asked once, kept thereafter. */
let blendable: boolean | null = null;

function canBlend(): boolean {
  if (blendable !== null) return blendable;
  try {
    const probe = document.createElement("canvas").getContext("2d");
    if (!probe) return (blendable = false);
    probe.globalCompositeOperation = "saturation";
    const saturation = probe.globalCompositeOperation === "saturation";
    probe.globalCompositeOperation = "multiply";
    const multiply = probe.globalCompositeOperation === "multiply";
    return (blendable = saturation && multiply);
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

  const image = ctx.getImageData(originX, originY, spanX, spanY);
  const pixels = image.data;
  // The same ramp the blend chain walks: flatten to a single channel, then
  // stretch it from the text colour at black to the background at white.
  const ramp = new Uint8ClampedArray(256 * 3);
  for (let level = 0; level < 256; level++) {
    const t = level / 255;
    ramp[level * 3] = text[0] + (bg[0] - text[0]) * t;
    ramp[level * 3 + 1] = text[1] + (bg[1] - text[1]) * t;
    ramp[level * 3 + 2] = text[2] + (bg[2] - text[2]) * t;
  }
  for (let row = 0; row < spanY; row++) {
    const y = originY + row;
    for (let column = 0; column < spanX; column++) {
      // Overlapping rectangles would otherwise be run through the ramp twice,
      // which is a different colour rather than the same one.
      if (regions?.length && !within(regions, originX + column, y)) continue;
      const i = (row * spanX + column) * 4;
      // Rec. 601 luma, which is what the `saturation` + greyscale path amounts
      // to and is cheap in integers.
      const level = (pixels[i] * 77 + pixels[i + 1] * 151 + pixels[i + 2] * 28) >> 8;
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

function within(regions: Rect[], x: number, y: number): boolean {
  for (const rect of regions) {
    if (x >= rect.x && x < rect.x + rect.w && y >= rect.y && y < rect.y + rect.h) return true;
  }
  return false;
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
