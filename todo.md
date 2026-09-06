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


## New issues

All of 4-12 are done. What is worth knowing about them:

- **11 (the Settings window slides sideways)** is fixed by the mechanism
  rather than by a reproduction: a wheel with nowhere to go chains outward in
  Blitz and the last parent is the viewport, which scrolls whatever sticks out
  of the root. `.root` and `.window-pane` no longer scroll sideways at all.
  Nothing in the harness overflows the root, so there is no test — worth
  checking by hand on the machine that showed it.
- **12 (a renamed theme keeps its file name)**: yes, and it does now — but
  only where the app named the file. `My Theme.toml` written by hand keeps its
  name whatever the theme inside it comes to be called, which is the same rule
  that stops this app reverting a built-in edited in place.
