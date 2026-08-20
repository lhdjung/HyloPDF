# HyloPDF, a smooth reading experience

Note: everything down to the horizontal rule describes what the project SHOULD be like. Below it, "Architecture of the built app" describes what it currently is.

## General
HyloPDF is a PDF reader written in Rust with Tauri. Cross-plattform, ergonomic, with a calm UI, and efficient: fast with no lags, little memory and CPU consumption, and a small binary.

Importantly, all settings are preserved throughout sessions, and all of them are independent of each other: changing one setting does not change any other setting.

## UI
The UI is clean and sleek. It is close to full-screen by default. In particular, the app should reserve much or most or all of the vertical axis for the document, as there is likely more room to the sides. No clunky or overbearing UI covers the document. However, true full-screen – no UI elements at all – is easily toggleable, and leaving it should be at least as easy and obvious.

Page progression is continuous scrolling by default, and it's a strong default: it can only ever be changed to anything else if the user explicitly opts into it. Not sure changing this should even be possible using shortcuts because continuous scrolling is so much better than the alternatives, and hitting such a keybind by accident would be frustrating.

There is no clutter in the UI. All elements are nice, modern, polished, and look straight out of professional web design. However, they should not have the typical vibe coded look, i.e., small caps (or caps in general), italics, exotic fonts, and a kind of dead, technical, sterile look. On the contrary, the look should be friendly and open; fresh and lively but in a subtle way.

UI elements might include symbols but they are definitely not just symbols, and not just tiny symbols. For each element, a combination of one symbol and one succinct text label would probably be good.

No animations unless the user takes an action. No pop-up windows that get into people's way.

## Theme settings
The app has dark mode that is easy to toggle (via UI or shortcuts) and that has a customizable definition: text, background, accent, and link colors can be any color chosen by the user, but with sensible defaults. It isn't black by default because the contrast would be too high. The text selection color should be customizable in the same way, and harmonize with each individual theme.

The app supports multiple themes, where each theme is a text-background color combination. Some themes are preinstalled, but users can define and name their own themes. Each theme has a name.

I guess, but I'm not certain, that themes are stored in some kind of config files (one per theme). Possible advantage: easily LLM-able if people want to create a theme but don't want to get in the technical weeds. If we do go this way, choose a good config file format, like TOML or whatever Ghostty uses.

## Preinstalled themes
Ignoring some settings, we have:
- Hylo Light: the default light theme, and the overall default theme. Doesn't change colors at all.
- Hylo Dark: the default dark theme. Text is white. Background is a dark grey, with maybe a tint of slate blue.
- Pzazz: cool and glamorous dark theme inspired by the Charm / Bubble Tea aesthetic.
- Dracula: text is pink, background is dark blue-ish. Some light blue and/or green is sprinkled in. Maybe that's not accurate – check the Dracula themes other apps use, and how that would translate into PDF theming.
- Gruvbox, for the oldies.

---

# Architecture of the built app

Everything above is the brief. What follows describes the app as it actually
stands, so that a change can be made without reading every file first.

## Shape

A Tauri 2 desktop app. Rust owns the disk and the window; a TypeScript frontend
owns everything the reader sees. Pages are drawn by **pdf.js** (`pdfjs-dist`)
onto plain canvases. There is no framework and no state library — the interface
is built with `document.createElement`, and one `App` object holds the state.

```
src-tauri/          Rust: settings, themes, reading history, the window
  src/lib.rs        every #[tauri::command], window restore, file associations
  src/settings.rs   settings.toml — one flat table, one key written at a time
  src/theme.rs      one TOML file per theme, built-ins installed on first run
  src/library.rs    library.toml — where you were in each document
  themes/*.toml     the five packaged themes, embedded with include_str!

src/                TypeScript: the interface
  main.ts           the App object: state, menus, keyboard, wiring
  viewer.ts         layout, rendering, scrolling, links   ← the heart of it
  themes.ts         theme → CSS variables, and the page recolouring itself
  search.ts         the full-document index and match stepping
  sidebar.ts        contents and thumbnails
  settings.ts       the settings window
  ui.ts             menus, switches, the modal window, the notice line
  api.ts            the only file that talks to Rust
  icons.ts          the hand-drawn icon set
  styles.css        all of it; textlayer.css is pdf.js's own selection layer
```

## What lives where

**Rust never renders anything.** It hands over bytes (`read_document` returns a
raw response rather than base64 through the JSON bridge), remembers things, and
asks the system to open a link or show a file. It also decides when the window
appears: the frontend calls `ready` once it can paint, so a dark theme never
flashes white on the way in.

**`api.ts` is the only door.** Nothing else imports `@tauri-apps/api`. It also
carries a browser fallback — settings in `localStorage`, a file input instead of
the native picker — so `npm run dev` can be opened in an ordinary browser while
working on the interface.

**Settings are written one key at a time.** `set_setting` reads the file,
changes one entry, writes it back, and leaves unknown keys alone. The defaults
table in `settings.rs` doubles as a whitelist. `App.set` keeps the in-memory
copy in step; `App.setSoon` debounces the ones that move continuously, like
zoom during a pinch.

**Themes are files.** Five built-ins are written into the user's themes
directory on every run so they can be read and copied, and so a change to a
shipped theme reaches a machine that already has the old one; the embedded
copies are authoritative, and a built-in file edited in place is overwritten.
Editing a built-in through the app saves a copy under an id of its own, which
is never touched. A theme names colours and a `recolor` flag, and nothing
else. `applyTheme` derives every shade the chrome uses — surface, line, three
grades of muted text, the positive green — from those colours, which is why a
five-line file is enough.

## The viewer

`viewer.ts` earns its size. Four things are worth knowing before changing it.

*Layout is computed once, for every page, in advance.* Page dimensions are read
up front (cheap — a page proxy is not a render), so the scroll container gets
its true height on the first frame and the scrollbar never lies. `boxes[]` holds
the position and scale of every page.

*Only the pages near the viewport exist in the DOM.* `mount()` keeps a window of
slots around the viewport (`OVERSCAN`), discards the rest, and queues the rest
for rendering nearest-to-the-middle first. A nine hundred page book costs about
what a two page letter costs.

*A rendered page is identified by `keyFor()` — its scale and its theme.* If the
key still matches, the canvas is reused; change scale, text colour, background,
link colour, or the picture setting, and the page repaints. This is the whole
invalidation story.

*Recolouring is baked into the bitmap, not applied by CSS.* `recolor()` in
`themes.ts` flattens the canvas to luminance with composite operations and
stretches it between the theme's two colours, so scrolling afterwards costs
nothing. Two things are painted back on top of that result: pictures, if
"Recolour pictures too" is off (pdf.js reports where images landed via
`recordImages`), and links, which are redrawn from the untouched copy and
recoloured towards the link colour. Both need a pristine copy of the canvas,
taken before recolouring.

## Things that will bite

**pdf.js runtime data must be given absolute URLs.** `cMapUrl`,
`standardFontDataUrl`, `iccUrl` and `wasmUrl` are handed to the pdf.js *worker*,
where a relative address resolves against the worker script rather than the
page. When they are wrong the worker silently drops what it cannot fetch, and
the failure is oblique: scanned documents lose all their text, because that text
lives in image masks. `asset()` in `viewer.ts` exists for this.

**Do not tint the document with `mix-blend-mode`.** WebKit drops the blend
against a composited canvas, and a dropped blend renders as a solid band across
the line. Anything that has to change the colour of ink goes onto the canvas.

**The top of the window is the app's, not the system's.** `titleBarStyle:
Overlay` runs the document up under the title bar, so on macOS there is no
native strip to drag the window by: `#toolbar` carries
`data-tauri-drag-region="deep"`, and that needs
`core:window:allow-start-dragging` in the capability, which `core:window:default`
does *not* include. With the toolbar hidden and the window not in full screen,
`.title-drag` stands in for it — inert until the pointer reaches the top eight
pixels, the same reach that brings the peek handle down — and
`set_titlebar_buttons` takes the three traffic lights away to match. All three
hang off `applyChrome()`, which is why `syncFullscreen` calls it.

**`core:window:default` grants almost nothing but getters.** Every window verb
the frontend uses has to be named in the capability one at a time, and the
failure is a line in the console rather than anything visible: without
`allow-destroy` the window will not close, because `onCloseRequested` in the JS
API destroys the window itself unless the handler prevents the default.

**A full-screen change costs the page its keyboard.** The webview stops being
the window's first responder, so every shortcut dies until something is
clicked, and `el.viewer.focus()` is not enough to get it back — only
`setFocus()` on the window is. `reclaimKeyboard()` does both, and it runs from
`syncFullscreen`, once the window has stopped moving, rather than the moment
the switch is thrown: AppKit passes focus around until the animation ends.

**The webview's own context menu is suppressed** everywhere except editable
fields and live text selections, because it offers to reload the app (which
closes the document) and to open the inspector.

**Escape and menus.** A popover registers its own capturing key handler, and so
does the modal window; the app-level shortcut handler bows out while either is
open. Clicking the button that opened a menu closes it — `showPopover` tracks
its anchor for exactly that.

## Testing the interface without taking the screen

**Drive the frontend headlessly. Do not synthesise input into the real app
unless the change is genuinely native.** `scripts/ui-harness.mjs` opens the
interface in Playwright's WebKit and gives you keys, wheel gestures, clicks and
screenshots against it:

```js
import { openApp } from "./scripts/ui-harness.mjs";
const app = await openApp({ pdf: "x.pdf", settings: { scroll_mode: "paged" } });
await app.press("ArrowRight");
console.log(await app.state());     // page, zoom, scroll, find bar, menus
await app.close();
```

Needs `npm run dev` running first. **WebKit, not Chromium**, and the default for
a reason: the app lives in a WKWebView, and the engines disagree about exactly
the things this app leans on — blend modes on a composited canvas, pinch zoom,
text layout. `{ engine: "chromium" }` is there for comparing the two.

Settings are seeded through the `localStorage` fallback in `api.ts`, so the
whole browser path is exercised: no Rust, no window, no traffic lights.
Anything that is really about the *window* — dragging it, full screen, the
title-bar buttons, the peek handle clearing the system bars — has to be checked
in the real app, and there is no way to do that quietly.

**The real app can only be driven from the foreground.** Synthetic keys and
clicks go to whichever process is frontmost, so testing takes the machine away
from whoever is using it. `CGEventPostToPid` looks like a way out and is not:
posting to the app's pid works for keystrokes only while its window is still
key — a few seconds after it was last in front — and never for clicks or scroll
(tested: two clicks and a scroll burst, nothing moved). Window-targeted
screenshots *do* work in the background, `screencapture -l <windowid>`, and are
cleaner than a full-screen grab. Take the window id from
`CGWindowListCopyWindowInfo` filtered by pid; a window on another Space or in
full screen cannot be captured at all.

So: say plainly when you are about to drive the real app, and say when you have
stopped.

## Running it

```
npm run tauri dev              # the app, with vite behind it
npm run tauri dev -- -- FILE   # …opened on a document
npm run dev                    # the interface alone, in a browser
npm run check                  # tsc --noEmit
npm run tauri build            # .app and .dmg
```

`scripts/sync-pdfjs.mjs` copies pdf.js's cmaps, standard fonts, ICC profiles and
wasm decoders into `public/pdfjs` before every dev run and build. Nothing is
fetched at runtime, and the app works offline.
