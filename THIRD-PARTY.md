# Third-party components

HyloPDF's own source is MIT (see `LICENSE`). A built app also carries the
components below, each under its own licence. All of them are permissive:
nothing here places a condition on what you may do with HyloPDF, and nothing
here has to be shared back. What they do ask for is attribution, which is why
this file exists and why the licence texts travel with the files they cover.

## Bundled in the app

| Component | Licence | Where its licence text lives |
| --- | --- | --- |
| [pdf.js](https://mozilla.github.io/pdf.js/) (`pdfjs-dist`) — renders every page | Apache-2.0 | `public/pdfjs/LICENSE` |
| Adobe character maps, for CJK documents | Apache-2.0, with pdf.js | `public/pdfjs/LICENSE` |
| Liberation Sans, one of the fourteen standard PDF fonts | SIL OFL 1.1 | `public/pdfjs/standard_fonts/LICENSE_LIBERATION` |
| Foxit fonts, the other thirteen | BSD-3-Clause | `public/pdfjs/standard_fonts/LICENSE_FOXIT` |
| OpenJPEG, the JPEG 2000 decoder | BSD-2-Clause | `public/pdfjs/wasm/LICENSE_OPENJPEG` |
| JBIG2 from PDFium, the scan decoder | BSD-3-Clause | `public/pdfjs/wasm/LICENSE_JBIG2` |
| qcms, the colour-profile converter | MIT | `public/pdfjs/wasm/LICENSE_QCMS` |
| The CGATS colour profile | CC0-1.0 | `public/pdfjs/iccs/LICENSE` |
| [Tauri](https://tauri.app) and its plugins — the window, the bridge | MIT or Apache-2.0 | in each crate |
| The Rust crates Tauri brings with it | MIT or Apache-2.0, a few MPL-2.0 or BSD | in each crate |

`cargo tree` and `npm ls` are the current answer for the last two rows; `cargo
metadata` prints every licence field at once.

## Not bundled: the webview

The window's contents are drawn by the system's own web engine — WebKit on
macOS, WebView2 on Windows, WebKitGTK on Linux — which the app links against
rather than ships. WebKitGTK is LGPL, and dynamic linking against it is what
the LGPL is written to allow.

## The typeface you are reading

None. Every label in the app is set in whatever the machine uses for its own
interface, so nothing has to be shipped and nothing looks foreign.
