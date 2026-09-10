## Items from previous
(Note: Strictly speaking, these first three items refer to the old Tauri version, but I think the issues they describe still persist in the current Dioxus Native rewrite.)

A. **Very high zoom goes soft.** `MAX_CANVAS_PIXELS` is 12 million
   (`src/viewer.ts`), so past roughly 300-400% on a large page the render is
   downsampled and the type blurs — at precisely the moment somebody is zooming
   in to read a footnote or inspect a figure. The cap is right; the answer is to
   render only the visible tile at full density rather than the whole page.
B. **The two platforms disagree about what a menu bar is.** Tauri installs its
   default macOS menu, so a Mac gets Copy, Select All, Close Window, Hide and
   Quit for free. Windows and Linux get none of it and the app supplies no menu
   bar of its own, so there is no discoverable Copy, Open Recent, Print or File
   menu at all. Decide it once rather than inherit it differently per platform.
C. **⌘P's notice lands after focus has left** for the program that prints, so it
   is easy to miss.

Short note: Dark Forest text color used to be #f7e0a2


## New issues

*(1–23 done, 2026-09-09. What each came to is in the code; the shape of the
larger ones:)*

- **⌘Q** is answered by the app's delegate (`applicationShouldTerminate:`
  → the app's own quit), so the writes after the event loop run.
- **Textures** are keyed by the palette's colours, not the theme's name, and
  the pipelines are rebuilt only when the renderer goes.
- **Every write** tells the watch it was ours (`reopen` does it once, for all
  of them), reopens with the document's own password, and takes the old
  handle back if the reopen fails. Removing a mark asks `standing` first.
- **Rendering is off the UI thread**: one render thread, FIFO, and a frame
  asked for when a page lands. Nothing prefetches yet, and a page arrives on
  the frame after it was drawn rather than in the one that mounted it.
- **Theme change is a re-render**, and the docs now say so; the source is
  not kept on the GPU (25MB a page), so there is nothing to re-run a pass
  over. The key stays.
- **Paths are made absolute at the door** (`config::absolute`); the config
  directory is the bundle id's, with `HyloPDF-dioxus` moved into place once.
- Marked documents stay in the library past 24; the shelf still shows 24.
