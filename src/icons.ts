/* A small hand-drawn icon set. Line icons on a 24px grid, rounded joins, so
   they sit next to text without shouting. Every icon here is paired with a
   label in the UI; none of them has to carry a meaning on its own. */

const paths: Record<string, string> = {
  folder:
    '<path d="M3 7.5A2.5 2.5 0 0 1 5.5 5h3.2l2 2.2h7.8A2.5 2.5 0 0 1 21 9.7v7.8A2.5 2.5 0 0 1 18.5 20h-13A2.5 2.5 0 0 1 3 17.5z"/>',
  contents:
    '<path d="M4 6.5h.01M4 12h.01M4 17.5h.01M9 6.5h11M9 12h11M9 17.5h7"/>',
  pages:
    '<rect x="4" y="4" width="7" height="7" rx="1.4"/><rect x="13" y="4" width="7" height="7" rx="1.4"/><rect x="4" y="13" width="7" height="7" rx="1.4"/><rect x="13" y="13" width="7" height="7" rx="1.4"/>',
  search: '<circle cx="11" cy="11" r="6.5"/><path d="M16 16l4.5 4.5"/>',
  up: '<path d="M6 14.5L12 8.5l6 6"/>',
  down: '<path d="M6 9.5l6 6 6-6"/>',
  left: '<path d="M14.5 6l-6 6 6 6"/>',
  right: '<path d="M9.5 6l6 6-6 6"/>',
  minus: '<path d="M5.5 12h13"/>',
  plus: '<path d="M12 5.5v13M5.5 12h13"/>',
  theme:
    '<circle cx="12" cy="12" r="8.2"/><path d="M12 3.8a8.2 8.2 0 0 1 0 16.4z" fill="currentColor" stroke="none"/>',
  close: '<path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/>',
  check: '<path d="M5 12.5l4.6 4.5L19 7.5"/>',
  document:
    '<path d="M13.5 3.5H7.5A2 2 0 0 0 5.5 5.5v13a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2V8.5z"/><path d="M13.5 3.5v5h5"/>',
  copy:
    '<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M15 5.5A1.5 1.5 0 0 0 13.5 4H5.5A1.5 1.5 0 0 0 4 5.5v8A1.5 1.5 0 0 0 5.5 15"/>',
  fitWidth: '<path d="M4 6.5v11M20 6.5v11M8 12h8M8 12l2.5-2.5M8 12l2.5 2.5M16 12l-2.5-2.5M16 12l2.5 2.5"/>',
  fitPage: '<rect x="6" y="4" width="12" height="16" rx="1.6"/><path d="M12 8.5v7M12 8.5l-2 2M12 8.5l2 2M12 15.5l-2-2M12 15.5l2-2"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M18.4 5.6L17 7M7 17l-1.4 1.4"/>',
  moon: '<path d="M20 13.4A8.2 8.2 0 0 1 10.6 4a8.2 8.2 0 1 0 9.4 9.4z"/>',
  // A cog, not a second sun: the old one was a circle with eight rays around
  // it, which is what `sun` is, and the two sat one button apart in the bar.
  settings:
    '<path d="M9.4 3.4h5.2l.2 2.3 1.3.7 2-1 2.7 4.6-1.9 1.3v1.4l1.9 1.3-2.7 4.6-2-1-1.3.7-.2 2.3H9.4l-.2-2.3-1.3-.7-2 1L3.2 14l1.9-1.3v-1.4L3.2 10l2.7-4.6 2 1 1.3-.7zM15.1 12a3.1 3.1 0 1 0-6.2 0 3.1 3.1 0 0 0 6.2 0z" fill="currentColor" fill-rule="evenodd"/><circle cx="12" cy="12" r="3.1"/>',
  edit: '<path d="M4.5 19.5h4l10-10a2.1 2.1 0 0 0-3-3l-10 10z"/><path d="M14.5 6.5l3 3"/>',
  trash: '<path d="M5.5 7h13M9.5 7V5.5h5V7M7 7l.8 12a1.6 1.6 0 0 0 1.6 1.5h5.2a1.6 1.6 0 0 0 1.6-1.5L17 7"/>',
  sidebar: '<rect x="3.5" y="4.5" width="17" height="15" rx="2.2"/><path d="M10 4.5v15"/>',
  plusCircle: '<circle cx="12" cy="12" r="8.2"/><path d="M12 8.5v7M8.5 12h7"/>',
  reset: '<path d="M4.5 12a7.5 7.5 0 1 0 2.4-5.5L4 9"/><path d="M4 4.5V9h4.5"/>',
  book: '<path d="M4 5.5A1.5 1.5 0 0 1 5.5 4H10a2 2 0 0 1 2 2v13a1.8 1.8 0 0 0-1.8-1.5H4z"/><path d="M20 5.5A1.5 1.5 0 0 0 18.5 4H14a2 2 0 0 0-2 2v13a1.8 1.8 0 0 1 1.8-1.5H20z"/>',
  keyboard:
    '<rect x="2.8" y="6.5" width="18.4" height="11" rx="2.2"/><path d="M6.5 10h.01M9.8 10h.01M13.1 10h.01M16.4 10h.01M6.5 13.2h.01M9.8 13.2h.01M13.1 13.2h.01M16.4 13.2h.01M8.5 16h7"/>',
  info: '<circle cx="12" cy="12" r="8.2"/><path d="M12 11v5.2"/><path d="M12 7.9h.01"/>',
  power: '<path d="M12 3.5v8"/><path d="M7.5 6.6a7.2 7.2 0 1 0 9 0"/>',
};

export type IconName = keyof typeof paths;

export function iconMarkup(name: string): string {
  const body = paths[name];
  if (!body) return "";
  return `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
}

/** Replace every `data-icon` attribute in a tree with the icon itself. */
export function hydrateIcons(root: ParentNode = document): void {
  for (const element of root.querySelectorAll<HTMLElement>("[data-icon]")) {
    const name = element.dataset.icon;
    if (!name) continue;
    element.insertAdjacentHTML("afterbegin", iconMarkup(name));
    delete element.dataset.icon;
  }
}
