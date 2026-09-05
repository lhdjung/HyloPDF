# Third-party components

HyloPDF's own source is MIT or Apache-2.0, at your option (see `LICENSE`). A
built app also carries the components below, each under its own licence. All of them are permissive:
nothing here places a condition on what you may do with HyloPDF, and nothing
here has to be shared back. What they do ask for is attribution, which is why
this file exists and why the licence texts travel with the files they cover.

## Bundled in the app

| Component | Licence | Where its licence text lives |
| --- | --- | --- |
| [PDFium](https://pdfium.googlesource.com/pdfium/) — renders every page, and writes the markup | BSD-3-Clause | with the library, from [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) |
| [Dioxus](https://dioxuslabs.com) and [Blitz](https://github.com/DioxusLabs/blitz) — the interface, laid out and painted | MIT or Apache-2.0 | in each crate |
| Stylo, Parley, Taffy, Vello and the rest of the Rust crates | MIT or Apache-2.0, a few MPL-2.0 or BSD | in each crate |

The pdfium build shipped beside the binary is Google's own source, compiled by
`bblanchon/pdfium-binaries`; the `LICENSE` in that archive is the one to keep
with it. `cargo tree` is the current answer for the rest, and `cargo metadata`
prints every licence field at once.

## Not bundled: a webview

There isn't one any more. The interface used to be HTML in the system's web
engine — WebKit, WebView2, WebKitGTK — and is now HTML laid out by Blitz inside
the binary. Nothing links against a browser.

## The typeface you are reading

None. Every label in the app is set in whatever the machine resolves
`ui-sans-serif` to, so nothing has to be shipped and nothing looks foreign.
