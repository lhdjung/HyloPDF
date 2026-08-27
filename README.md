# HyloPDF

A PDF reader that gets out of the way. One thin toolbar, a document that fills
the rest of the window, and dark mode that actually recolours the page instead
of dimming the screen.

It is small, quick to open a book of any size, and it puts you back on the page
you left.

## Getting it

Every [release](../../releases) carries an installer for each platform:

| | |
|---|---|
| macOS | `HyloPDF_<version>_aarch64.dmg` (Apple silicon) or `_x64.dmg` (Intel) |
| Linux | a `.deb`, an `.rpm`, or an AppImage |
| Windows | an `.msi`, or `-setup.exe` |

The builds are not signed yet, so the first launch on macOS needs a right-click
→ Open, or a nudge in System Settings → Privacy & Security. Windows
SmartScreen will want "More info" → "Run anyway".

## Reading

Drop a PDF on the window, press ⌘O, or double-click a file — HyloPDF registers
itself for `.pdf`, and a second document opens in a second window rather than
pushing the first one out. Quit with something open and it comes back next
time, on the page you left it.

Pages scroll continuously and fit the width of the window. Both are settings,
but they are the defaults for a reason.

**A few keys worth knowing.** ⌘ on macOS, Ctrl elsewhere. **F1** shows the
whole list, including anything you have rebound.

| | |
|---|---|
| ⌘F | Search — the match count opens a list of every hit |
| ⌘G / ⌘⇧G | Next / previous match |
| → / ← | Next / previous page |
| Space / ⇧Space | Down a screen, up a screen |
| p | Go to a page number, or a label like `xii` |
| ⌘[ / ⌘] | Back to where you jumped from, and forward again |
| ⌘D | Dark mode |
| ⌘B | Contents, marks and thumbnails |
| ⌘T | Hide the toolbar; ⌘⇧F for full screen |
| ⌘⇧B | Put a pin in this page |
| ⌘⇧C | Copy what you selected, with its page number |

Vim's `j k h l`, `g g` and `G` work too, and every key can be changed —
Settings → Keyboard has a button that opens `keys.toml` for you.

**Two worth finding.** Under Settings → Reading, *Trim the margins* measures
where the ink actually starts and gives the white edges back to the words,
which on a scan is a quarter of the window. *Pages side by side* uses a wide
window the way a book does; "Two, cover alone" keeps page one on its own, so
every spread after it falls the way it was printed.

**Marks** (⌘⇧B) are pins, listed in the sidebar above the document's own
contents and named for the section they fall in. Nothing is written into the
PDF. Notes somebody else left in a document show up and can be read; writing
one is not something HyloPDF does.

## Themes

Fourteen ship with it, from Sepia to Gruvbox to Tokyo Night. HyloPDF follows
your system between a light theme and a dark one; choosing a theme yourself, or
pressing ⌘D, takes that over.

A theme is a small file you can write by hand, in `themes/` beside the
settings:

```toml
name = "Hylo Dark"
text = "#e9eaee"
background = "#24272f"
accent = "#8fb0d4"
# link = "#e0a271"           # links in the document; the accent if left out
# selection_area = "#7a4247" # behind selected text; from the accent if left out
# selection_text = "#f8eeec" # the ink on it; from selection_area if left out
# recolor = false            # leave the document as printed, theme only the app
```

Two colours are enough: everything else is worked out from them, and the page
is mapped onto them by brightness, so black type lands on your text colour and
white paper on your background. A figure keeps its hues. Save the file and the
open document repaints. The theme editor in the app writes the same files, so
you can start in one and finish in the other.

## Where your things live

```
~/Library/Application Support/app.hylopdf/   macOS
~/.config/app.hylopdf/                       Linux
%APPDATA%\app.hylopdf\                       Windows
```

`settings.toml` is one flat table of plain values, `keys.toml` your keyboard,
`themes/` your themes, and `library.toml` the page each document was left on.
All of them are yours to edit; HyloPDF changes one setting at a time and leaves
anything it does not recognise alone.

## What it does not do

It reads. It does not annotate or fill in forms, and ⌘P hands the document to
whatever your system prints PDFs with rather than pretending to have a print
dialog of its own.

## Building it

```sh
npm install
npm run tauri dev      # the app, with the interface hot-reloading
npm test               # the whole interface, headlessly
npm run tauri build    # installers, in src-tauri/target/release
```

[AGENTS.md](AGENTS.md) is the long version: what the app is meant to be, and
how the one that exists is put together.

## Licence

MIT — see [LICENSE](LICENSE). [THIRD-PARTY.md](THIRD-PARTY.md) lists what a
built app carries with it, all of it permissive.

## The name

*Strix hylophila*, the [rusty-barred owl](https://en.wikipedia.org/wiki/Rusty-barred_owl).
Night owls appreciate a dark theme. Also, Rust.
