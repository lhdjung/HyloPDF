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
): void {
  if (!theme.recolor) return;
  const bg = parseColor(theme.background, WHITE);
  const text = parseColor(theme.text, BLACK);
  const inverted = luminance(text) > luminance(bg);

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
  for (let i = 0; i + 5 < coordinates.length; i += 6) {
    const ax = coordinates[i] * width;
    const ay = coordinates[i + 1] * height;
    const bx = coordinates[i + 2] * width;
    const by = coordinates[i + 3] * height;
    const cx = coordinates[i + 4] * width;
    const cy = coordinates[i + 5] * height;
    const dx = cx + (bx - ax);
    const dy = cy + (by - ay);

    ctx.save();
    ctx.beginPath();
    ctx.moveTo(ax, ay);
    ctx.lineTo(bx, by);
    ctx.lineTo(dx, dy);
    ctx.lineTo(cx, cy);
    ctx.closePath();
    ctx.clip();
    ctx.drawImage(pristine, 0, 0, width, height);
    ctx.restore();
  }
}
