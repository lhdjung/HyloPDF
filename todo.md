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

*(1–4 done, 2026-09-11.)*

- **Swatches**: × closes the popover and keeps the selection; … opens the
  Highlight colours window (the theme editor's picker on each of the six,
  written straight to `markup_color_N`), with a reset that asks first.
- **Double click** selects the word and a third click the line; the sweep
  extends by that unit, so the pixel a mouse moves before the release no
  longer cuts the word back.
- **Numbers off the paper**: a document with no `/PageLabels` has a sample of
  pages read for a whole number in its top or bottom band; the offset most
  agree on names every page (Bem 2011 reads 407 of 425).
- **"of 425" is a menu** choosing between the printed numbers and the place
  in the file (`page_numbering`), also in the settings menu and Reading page.
