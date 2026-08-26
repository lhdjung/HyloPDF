# HyloPDF

A calm, spacious PDF reader: simple UI, easy dark mode, and ergonomic controls.

This is the working implementation of the description in [AGENTS.md](AGENTS.md).

## Installing

Each release on the [releases page](../../releases) carries an installer for
every platform:

| | |
|---|---|
| macOS | `HyloPDF_<version>_aarch64.dmg` for Apple silicon, `_x64.dmg` for Intel |
| Linux | a `.deb`, an `.rpm`, or an AppImage |
| Windows | an `.msi`, or `-setup.exe` for the NSIS installer |

Until the builds are signed, macOS will say the app cannot be checked for
malicious software: open it once from the right-click menu, or allow it in
System Settings → Privacy & Security.

## Running it

```sh
npm install
npm run tauri dev      # development, with hot reload for the interface
npm test               # the interface, headlessly — takes nobody's screen
npm run tauri build    # a release build and an installer in src-tauri/target/release
```

`HyloPDF path/to/file.pdf` opens a document straight away, and the app
registers itself for `.pdf` files, so "Open with" works too. Opening a second
document hands it to the window that is already open rather than starting
another copy.

Documents are read a piece at a time rather than loaded whole, so a very large
PDF opens as quickly as a small one and costs about as much to keep open.

## Keyboard

The shortcuts below use ⌘ on macOS and Ctrl elsewhere.

| | |
|---|---|
| ⌘O | Open a document |
| ⌘, | Settings |
| F1, or ⌘/ | Every shortcut, in one list |
| ⌘P | Hand the document to a program that prints |
| ⌘A | Select the text of the page you are on |
| ⌘F | Search this document |
| Enter / ⇧Enter | Next / previous match, from the search field |
| ⌘G / ⌘⇧G | Next / previous match, from anywhere |
| The count in the find bar | Every match, listed with the line it is on |
| ⌘D | Dark mode on or off |
| ⌘B | Show or hide the contents panel |
| ⌘T | Show or hide the toolbar |
| ⌘+ / ⌘− / ⌘0 | Zoom in, zoom out, back to fit width |
| ⌘1 / ⌘2 | Actual size, fit the whole page |
| ⌘R / ⌘L | Turn the page right, turn it left |
| ⌘⇧F, or ⌃⌘F on macOS, or F11 | Full screen |
| ⌘⇧P | Presenting: full screen with nothing else on it |
| Escape | Close the search bar, or leave full screen |

Moving around a document:

| | |
|---|---|
| → / ← | Next / previous page |
| ↓ / ↑, or j / k | A little down, a little up |
| Space / ⇧Space | Down a screen, up a screen |
| Page Down / Page Up | Down a screen, up a screen |
| Home / End | First / last page |
| g | Jump to a page number — or to a page label, like `xii` |
| ⌘[ / ⌘], or ⌥← / ⌥→ | Back to where you jumped from, and forward again |

Two pages can stand side by side — Settings → Reading, or the Settings menu in
the bar. "Two, cover alone" leaves page one on its own, which is how a book
falls open: page one is a right-hand page, so pairing it with page two puts
every spread after it out by one.

There is deliberately no shortcut for the page layout. Continuous scrolling is
the default and switching away from it should take a decision, not a slip of
the fingers.

Scanned books and anything typeset with an inch of white down each side spend
a quarter of the window on paper. **Trim the margins** measures where the ink
starts — over a sample of the pages, so every page keeps the same scale — and
gives that room back to the words. It is off until you ask for it.

## What it does not do

HyloPDF reads. It does not annotate, fill in forms, or print — and of those,
printing is the one that answers back rather than doing nothing: ⌘P hands the
document to Preview, or to whatever this system opens PDFs with, and says so.
Printing well needs a print dialog — a page range, a paper size, a printer, a
preview — and every shortcut around writing one ends with four hundred pages
coming out of a printer nobody chose.

## Themes

HyloPDF follows the machine: the light theme while the system is light, the
dark one when it turns dark, and the two are whichever you last chose. Picking
a theme that disagrees with the system — or pressing ⌘D — turns the following
off, since at that point you would rather decide yourself. The switch is in
the Theme menu and on the Appearance page.

A theme is a small TOML file. The fourteen that ship with HyloPDF are written into
your theme folder on every run, so they can be read and copied, and so a change
to a shipped theme reaches a machine that already has the old one. The embedded
copies are the authoritative ones: a built-in edited in place is overwritten,
while editing one through the app saves a copy under a name of its own, which
is never touched. Each shipped file says as much at the top of it, so nobody
finds that out by losing an afternoon's work.

```
~/Library/Application Support/app.hylopdf/themes/   (macOS)
~/.config/app.hylopdf/themes/                       (Linux)
%APPDATA%\app.hylopdf\themes\                       (Windows)
```

A whole theme is five lines:

```toml
name = "Hylo Dark"
text = "#e9eaee"
background = "#24272f"
accent = "#8fb0d4"
# link = "#8ec5e8"           # links in the document; the accent if left out
# selection_area = "#44475a" # behind selected text; from the accent if left out
# selection_text = "#f8eeec" # the ink on it; from selection_area if left out
# recolor = false            # leave the document as printed, theme only the app
```

Two colours are enough because everything else — the toolbar, the borders, the
muted text, the shadow under a page — is derived from them. Documents are
mapped onto those two colours by luminance, so black ink lands on the text
colour, white paper on the background, and a grey rule stays a grey rule.

The theme editor in the app writes the same files; anything you save there can
be edited by hand afterwards, and anything you write by hand shows up in the
app.

## Settings

HyloPDF starts on the document that was open when you last quit. Closing a
document yourself means you have finished with it, and it is not reopened;
`reopen_last_document = false` turns the whole thing off.

Settings live next to the themes in `settings.toml`, one flat table of plain
values, and the file is yours to edit. Every setting is written on its own:
changing one never rewrites another, and a key HyloPDF does not recognise is
carried through untouched rather than dropped.

Where you stopped reading is remembered per document in `library.toml`, which
is reading history rather than configuration and so keeps its own file.

## How it is put together

```
src-tauri/     Rust: settings, themes, reading history, window state
  themes/      the built-in theme files, embedded in the binary
src/           the interface: viewer, sidebar, search, themes, menus
public/pdfjs/  character maps, standard fonts, colour profiles, wasm decoders
```

The viewer measures every page up front, so the scrollbar tells the truth from
the first frame, then keeps only the pages near the viewport in the DOM. A page
is drawn once per zoom level and theme; the theme is baked into the bitmap with
canvas blend modes rather than a CSS filter, so scrolling afterwards costs
nothing.

## Name
HyloPDF is named after the [rusty-barred owl](https://en.wikipedia.org/wiki/Rusty-barred_owl), *Strix hylophila*. Night owls might appreciate dark themes. Also, Rust.