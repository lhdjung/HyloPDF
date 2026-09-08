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

*(4–11 done. What each came to is in the commit and in `AGENTS.md`.)*

4. ~~The app doesn't currently have a scrollbar.~~ Drawn in `app.rs` and
   `styles.rs`: 12px hard against the window's edge, the theme's `--faint` and
   `--muted` under the hand, a thumb with a 34px floor so a 400-page book still
   has something to catch. A press on the track jumps; a press on the thumb
   drags.
5. ~~"x of y" overlaid on the page.~~ It rides the scrollbar's thumb now, to
   its left.
6. ~~"Show page count while scrolling" should be off by default.~~ It is, and
   the count still appears for as long as the bar is being dragged.
7. ~~The "Toolbar hidden […]" message is at the foot of the page.~~ With the
   bar away every notice goes to the top right, under where the bar's own
   right-hand group was.
8. ~~"Show toolbar" to the same place.~~ Done.
9. ~~The recents carry a redundant icon and cut off hard.~~ The second icon is
   gone from the Open menu's shelf, and both shelves fade their last 20px the
   way the document's name in the toolbar does.
10. ~~"New window" opens a tab in full screen.~~ Automatic tabbing is off, so
    ⌘N is always a window; "New tab" is its own item under Open… and its own
    (unbound, rebindable) action.
11. ~~Can't jump between tabs.~~ ⌘1–⌘9 choose one, on macOS. Actual size and
    fit page move to ⌥⌘1 and ⌥⌘2 there and keep ⌘1/⌘2 elsewhere. ⌘W closes the
    current tab — it had been bound to nothing at all on a Mac.
