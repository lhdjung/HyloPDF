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
| Linux | a `.deb` or an AppImage |
| Windows | an `.msi`, or `-setup.exe` |

The builds are not signed yet, so the first launch on macOS needs a right-click → Open, or a nudge in System Settings → Privacy & Security.

Windows SmartScreen will want "More info" → "Run anyway".

## Dev build

The whole app is Rust: [Dioxus] Native, with [Blitz] laying out real HTML and
CSS instead of a webview. Building it needs the Rust toolchain, Node for one
fixture generator, and `libfontconfig1-dev` on Linux or the Xcode command line
tools on macOS. pdfium is a shared library and is not in this repository —
point `HYLO_PDFIUM` at a directory holding one from
[pdfium-binaries](https://github.com/bblanchon/pdfium-binaries).

```sh
cargo run                            # the app
cargo test                           # the whole interface, headlessly
cargo install cargo-packager --locked
cargo packager --release             # installers, in target/release
```

[Dioxus]: https://dioxuslabs.com
[Blitz]: https://github.com/DioxusLabs/blitz

## AI usage
The code was written by Claude Opus 5, but I had a strong vision for the UI and kept complaining to Claude until I liked the implementation.

## The name

*Strix hylophila*, the [rusty-barred owl](https://en.wikipedia.org/wiki/Rusty-barred_owl). Night owls might appreciate dark themes. Also, Rust.
