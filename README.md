# HyloPDF

A PDF reader that gets out of the way. One thin toolbar, a document that fills
the rest of the window, and dark mode that actually recolours the page instead
of dimming the screen.

It is small, quick to open a book of any size, and it puts you back on the page
you left.

## Installation

Every [release](../../releases) has an installer for each platform:

| | |
|---|---|
| macOS | `HyloPDF_<version>_aarch64.dmg` (Apple silicon) or `_x64.dmg` (Intel) |
| Linux | a `.deb`, an `.rpm`, or an AppImage |
| Windows | an `.msi`, or `-setup.exe` |

The builds are not signed yet, so the first launch on macOS needs a right-click → Open, or a nudge in System Settings → Privacy & Security.

Windows SmartScreen will want "More info" → "Run anyway".

## Dev install

```sh
npm install
npm run tauri dev      # the app, with the interface hot-reloading
npm test               # the whole interface, headlessly
npm run tauri build    # installers, in src-tauri/target/release
```

## AI usage
The code was written by Claude Opus 5, but I had a strong vision for the UI and kept complaining to Claude until I liked the implementation.

## The name

*Strix hylophila*, the [rusty-barred owl](https://en.wikipedia.org/wiki/Rusty-barred_owl). Night owls might appreciate dark themes. Also, Rust.
