## Items from previous
(Note: Strictly speaking, these first three items refer to the old Tauri version, but I think the issues they describe still persist in the current Dioxus Native rewrite.)

1. **Very high zoom goes soft.** `MAX_CANVAS_PIXELS` is 12 million
   (`src/viewer.ts`), so past roughly 300-400% on a large page the render is
   downsampled and the type blurs — at precisely the moment somebody is zooming
   in to read a footnote or inspect a figure. The cap is right; the answer is to
   render only the visible tile at full density rather than the whole page.
2. **The two platforms disagree about what a menu bar is.** Tauri installs its
   default macOS menu, so a Mac gets Copy, Select All, Close Window, Hide and
   Quit for free. Windows and Linux get none of it and the app supplies no menu
   bar of its own, so there is no discoverable Copy, Open Recent, Print or File
   menu at all. Decide it once rather than inherit it differently per platform.
3. **⌘P's notice lands after focus has left** for the program that prints, so it
   is easy to miss.

Short note: Dark Forest text color used to be #f7e0a2


## New issues

*(4–6 done. What each came to is in the commit and in the code.)*

4. ~~"Show toolbar" and the "Toolbar hidden […]" message should swap places.~~
   The notice takes the top of the window and the handle sits below it, on the
   line the toolbar itself occupies — clear of the strip macOS slides its title
   bar over. The reach that brings the handle down covers the handle's own box
   now, not just the top eight pixels.
5. ~~Split "Toolbar hidden […]" into two lines, and a full stop for the comma.~~
   Done, and "Show toolbar" is two lines to match.
6. ~~The theme editor's colour picker is forty swatches and nothing else.~~ A
   saturation/value square and a hue strip above them, after QRnew's: markers
   drawn as background layers, press and drag on both, the swatches kept as a
   shortcut.

And three things that fell out of those:

- Escape is a ladder in the theme editor — the field, then the picker, then the
  window — rather than closing the window from under somebody correcting six
  digits.
- Every notice goes to the top right corner now, with the toolbar and without
  it, so ⌘+ answers in one place rather than two.
- `ColorField` writes the draft itself, which took six duplicated handlers out
  of the editor.
