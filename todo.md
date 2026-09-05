1. The toolbar's "Open" button should say "Open…" instead.
2. The current-document dropdown's "Information" window looks a bit shabby. Make it look polished like the Cmd+, "Settings" window.
3. The app uses a so-called "window" icon that is actually the classical copy icon. Only use it for copy actions, and use better fitting icons for the other actions.
4. "Settings" item "Pages side by side" is a default but it doesn't say so on its right.
5. "Changing keybinds" section basically tells the user: "good luck finding that file, nerd." Add an "Open keys file" button instead of showing the path. Style the button like "Reload" below it.
6. In "About", do the same for the two paths there.
## From the outside review (the rest of it is done; see git for the full text)

7. **Very high zoom goes soft.** `MAX_CANVAS_PIXELS` is 12 million
   (`src/viewer.ts`), so past roughly 300-400% on a large page the render is
   downsampled and the type blurs — at precisely the moment somebody is zooming
   in to read a footnote or inspect a figure. The cap is right; the answer is to
   render only the visible tile at full density rather than the whole page.
8. **The two platforms disagree about what a menu bar is.** Tauri installs its
   default macOS menu, so a Mac gets Copy, Select All, Close Window, Hide and
   Quit for free. Windows and Linux get none of it and the app supplies no menu
   bar of its own, so there is no discoverable Copy, Open Recent, Print or File
   menu at all. Decide it once rather than inherit it differently per platform.
9. **⌘P's notice lands after focus has left** for the program that prints, so it
   is easy to miss.
