## Big-picture issues

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
D. **The logo is boring.** Reddish on black doesn't look great, and there is not
   much connection to the name: the app is named after an owl. I don't want an
   owl staring at the user, but maybe some abstract or geometric art would be
   nice, like a paper with lines on it that faintly resemble an owl in shape.
   Maybe if facial features are deemphasized, it works? It shouldn't be flashy
   or dramatic in any way; very much the opposite!
E. **Git traces of the olds logos remain.** I don't want tons of image files
   to hang around in the repo forever, so at some point after point D is done,
   something like git-filter-repo might be in order.

Short note: Dark Forest text color used to be #f7e0a2


## Concrete issues

*(9–11 done, 2026-09-12.)*

1. Selecting some text can sometimes crash the app! Not sure why and when it does it.
2. For some reason, Cmd+A closes the app. Not great. The "123 characters of this page selected" message is not needed; remove it.
3. Double-clicking a word right before a comma, period, double colon, or em-dash (or maybe other symbols?) selects that symbol, as well; but it shouldn't.
4. Searching opens the sidebar – ok. But then, when clicking on a page (not in the sidebar but on the "main screen"!) causes a jarring zoom onto that page and a stray selection of some area near where I clicked. Not sure how to resolve this.
5. In the "Highlight colors" window, when the color picker is opened, clicking elsewhere in the window should close the color picker but doesn't. It also says "Colour" but you should avoid Briticisms.
6. Not sure if there is anything wrong with this, but highlight colors themselves go through the theme filter. I genuinely don't know how to resolve this, but as it stands, the highlight colors on the page are not the same as those in the palette or the color picker.
7. Adding or removing a highlight causes a jarring flicker of the page: all contents of the page briefly disappear, then reappear.
8. Resizing the page – any kind of zooming – causes the same flicker.
9. The color palette's red X symbol has a black background which looks a bit too dramatic. Choose a different background color; I'm thinking dark grey, but you are free to try something else, too.
10. The scrollbar should disappear a few seconds after the end of page movement (e.g., scrolling, moving with left or right arrow keys, etc.).
11. Sidebar pages: the bottom of the last page and the page number below it are cut off (unless all pages fit into the sidebar without scrolling).