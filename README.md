# HyloPDF

A PDF reader that gets out of the way. One thin toolbar, a document that fills
the rest of the window, and dark mode that actually recolours the page instead
of dimming the screen.

It is small, quick to open a book of any size, and it puts you back on the page
you left.

## Installation

Download and run — no store, no package manager, nothing to build:

| | |
|---|---|
| **macOS** | [Apple silicon](../../releases/latest/download/HyloPDF-macos-arm64.dmg) · [Intel](../../releases/latest/download/HyloPDF-macos-x64.dmg) |
| **Linux** | [AppImage](../../releases/latest/download/HyloPDF-linux-x86_64.AppImage) · [.deb](../../releases/latest/download/HyloPDF-linux-amd64.deb) · [.rpm](../../releases/latest/download/HyloPDF-linux-x86_64.rpm) |
| **Windows** | [Installer](../../releases/latest/download/HyloPDF-windows-setup.exe) · [.msi](../../releases/latest/download/HyloPDF-windows.msi) |

Those links always point at the newest build; [every release](../../releases)
is listed if you want a particular one.

> **macOS first launch:** macOS blocks the app because it is not signed. After
> the warning, open *System Settings → Privacy & Security*, scroll to the
> *Security* section, and click *Open Anyway*.

> **Windows first launch:** SmartScreen blocks it. Click *More info* on the
> warning, then *Run anyway*.

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
