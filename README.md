# Moonowl

A PDF reader that gets out of the way. One thin toolbar, a document that fills
the rest of the window, and dark mode that actually recolours the page instead
of dimming the screen.

It is small, quick to open a book of any size, and it puts you back on the page
you left.

## Installation

Download and run — no store, no package manager, nothing to build:

| | |
|---|---|
| **macOS** | [Apple silicon](../../releases/latest/download/Moonowl-macos-arm64.dmg) · [Intel](../../releases/latest/download/Moonowl-macos-x64.dmg) |
| **Linux** | [AppImage](../../releases/latest/download/Moonowl-linux-x86_64.AppImage) · [.deb](../../releases/latest/download/Moonowl-linux-amd64.deb) · [.rpm](../../releases/latest/download/Moonowl-linux-x86_64.rpm) |
| **Windows** | [Installer](../../releases/latest/download/Moonowl-windows-setup.exe) · [.msi](../../releases/latest/download/Moonowl-windows.msi) |

Those links always point at the newest build; [every release](../../releases)
is listed if you want a particular one.

> **macOS first launch:** macOS blocks the app because it is not signed. After
> the warning, open *System Settings → Privacy & Security*, scroll to the
> *Security* section, and click *Open Anyway*.

> **Windows first launch:** SmartScreen blocks it. Click *More info* on the
> warning, then *Run anyway*.

## Build it yourself

One command. It fetches pdfium, builds the release, packages it and installs
it — on macOS into `/Applications`, on Linux through `apt`, `dnf` or an
AppImage in `~/.local/bin`, on Windows through the installer:

```sh
./scripts/install.sh                 # macOS and Linux
```
```powershell
.\scripts\install.ps1                # Windows
```

You need the Rust toolchain, plus the Xcode command line tools on macOS or
`libfontconfig1-dev` on Linux. Nothing else: pdfium comes from
[pdfium-binaries] and everything else is a crate. Because you built the app
rather than downloaded it, neither Gatekeeper nor SmartScreen has anything to
complain about — no *Open Anyway* step.

The app itself is all Rust: [Dioxus] Native, with [Blitz] laying out real HTML
and CSS instead of a webview. To work on it, run `./scripts/pdfium.sh` once and
then the usual `cargo run`, `cargo run -- FILE` and `cargo test`.

[Dioxus]: https://dioxuslabs.com
[Blitz]: https://github.com/DioxusLabs/blitz
[pdfium-binaries]: https://github.com/bblanchon/pdfium-binaries

## AI usage
The code was written by Claude (Opus 5 and Fable 5.1), but I had a strong vision for the UI and kept complaining to Claude until I liked the result.

## The name

An owl by moonlight. The icon's owl is *Strix hylophila*, the [rusty-barred owl](https://en.wikipedia.org/wiki/Rusty-barred_owl). Night owls might appreciate dark themes. Also, Rust.
