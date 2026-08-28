/** The keyboard, as a table rather than as a tower of branches.
 *
 * Everything the app listens for is an *action* with a name, and a chord is
 * only ever a way of asking for one. That is what makes the keys remappable —
 * a file on disk says `action = ["chord"]` and nothing else in the app has to
 * know — but it is worth having for its own sake: what used to decide which
 * shortcut had been pressed was twenty-five `if` branches whose *order* was
 * load-bearing. ⌘⇧F had to be tested before ⌘F or full screen dropped the
 * reader into the find bar; ⌘G had to say `!altKey` or it took ⌥⌘G's
 * keystroke first. None of that survives here: an event is turned into a
 * chord, and a chord is looked up. Two actions cannot both answer to ⌘F,
 * because that is one key in one map, and saying so is a collision the reader
 * is told about rather than a bug that depends on which branch came first.
 *
 * A chord is written `mod+shift+f`. The modifiers, in the order they are
 * always spelled in a canonical chord:
 *
 * * `mod`   — ⌘ on a Mac, Ctrl everywhere else. This is the modifier the app
 *             uses for its own shortcuts, and the only one that changes
 *             meaning between platforms.
 * * `ctrl`  — the Control key itself. On Windows and Linux that *is* `mod`,
 *             so it is normalised to it: `ctrl+d` and `mod+d` are one chord
 *             there and two on a Mac, and pretending otherwise would mean a
 *             file that quietly does nothing.
 * * `alt`   — ⌥ / Alt.
 * * `shift`
 *
 * Chords separated by a space are pressed one after the other, which is how
 * `g g` — Vim's "go to the top", and every Vim-shaped reader's — can be a
 * binding at all.
 */

import { isMac } from "./api";

/* --------------------------------------------------------------- actions */

export type Action =
  // Documents
  | "open"
  | "new-window"
  | "print"
  | "settings"
  | "help"
  | "close-window"
  | "quit"
  | "find"
  | "find-next"
  | "find-previous"
  | "select-page"
  | "copy-quote"
  | "mark"
  | "markup"
  | "dismiss"
  // Moving around
  | "next-page"
  | "previous-page"
  | "scroll-down"
  | "scroll-up"
  | "half-screen-down"
  | "half-screen-up"
  | "screen-down"
  | "screen-up"
  | "first-page"
  | "last-page"
  | "go-to-page"
  | "back"
  | "forward"
  // Looking at it
  | "zoom-in"
  | "zoom-out"
  | "fit-width"
  | "actual-size"
  | "fit-page"
  | "rotate-right"
  | "rotate-left"
  | "dark"
  | "sidebar"
  | "toolbar"
  | "fullscreen"
  | "present";

export type ActionGroup = "Documents" | "Moving around" | "Looking at it";

export type ActionSpec = {
  id: Action;
  /** What it does, in the words the Keyboard page uses. */
  label: string;
  group: ActionGroup;
  /** Needs a document open. Everything else answers on the start screen too. */
  needsDocument?: boolean;
  /** The keys it ships with, on every platform. */
  keys: string[];
  /** …and these as well, on a Mac. */
  macKeys?: string[];
  /** …or these, on Windows and Linux. */
  otherKeys?: string[];
};

/** Every action, its default keys, and the words the Keyboard page shows.
 *
 * The page is generated from this list, so what Settings says the app listens
 * for is what the app listens for — including a key the reader has rebound.
 * The hand-written list it replaced had already drifted: it named ⌘T twice
 * and omitted the pinch. */
export const ACTIONS: readonly ActionSpec[] = [
  /* ------------------------------------------------------------ documents */
  { id: "open", label: "Open a document", group: "Documents", keys: ["mod+o"] },
  // Two documents at once means two windows: the whole interface is one object
  // in one webview, so a window is a complete second reader and nothing about
  // the first one changes.
  { id: "new-window", label: "New window", group: "Documents", keys: ["mod+n"] },
  {
    id: "print",
    label: "Print — handed to a program that prints",
    group: "Documents",
    keys: ["mod+p"],
  },
  { id: "settings", label: "Settings", group: "Documents", keys: ["mod+,"] },
  { id: "help", label: "This list", group: "Documents", keys: ["f1", "mod+/"] },
  // Closing the window, and quitting, on the platforms that have no menu to
  // put them in. macOS gets a menu bar from Tauri whether this app asks for
  // one or not, and AppKit answers ⌘W and ⌘Q before the page ever sees them.
  //
  // Two of them rather than one, because there is more than one window now and
  // Ctrl+Q closing whichever of them happened to have the keyboard would be a
  // strange thing for Quit to do.
  {
    id: "close-window",
    label: "Close this window",
    group: "Documents",
    keys: [],
    otherKeys: ["mod+w"],
  },
  {
    id: "quit",
    label: "Close HyloPDF",
    group: "Documents",
    keys: [],
    otherKeys: ["mod+q"],
  },
  { id: "find", label: "Search this document", group: "Documents", keys: ["mod+f"] },
  { id: "find-next", label: "Next match", group: "Documents", keys: ["mod+g"] },
  { id: "find-previous", label: "Previous match", group: "Documents", keys: ["mod+shift+g"] },
  {
    id: "select-page",
    label: "Select the text of this page",
    group: "Documents",
    needsDocument: true,
    keys: ["mod+a"],
  },
  {
    id: "copy-quote",
    label: "Copy selection, with its page number",
    group: "Documents",
    keys: ["mod+shift+c"],
  },
  {
    id: "mark",
    label: "Mark this page, or take the mark off",
    group: "Documents",
    keys: ["mod+shift+b"],
  },
  // copy-quote's neighbour: the other thing a reader does with a selection.
  // Opens the colour popover rather than marking outright, because "which
  // colour" is the one thing this key cannot answer for itself.
  {
    id: "markup",
    label: "Mark the selection — opens the colour popover",
    group: "Documents",
    needsDocument: true,
    keys: ["mod+shift+h"],
  },
  {
    id: "dismiss",
    label: "Close the search bar, leave full screen, stop presenting",
    group: "Documents",
    keys: ["escape"],
  },

  /* --------------------------------------------------------- moving about */
  // Left and right turn pages, in every scroll mode: continuous scrolling
  // makes a page boundary easy to lose, and landing on the top of one is the
  // whole reason to reach for these rather than for the keys that move by a
  // screen. h and l are the same pair for a reader who came from Vim, and
  // they mean the same thing here as the arrows do rather than the panning
  // they mean in Zathura — this app has one pair of keys per direction and
  // they should not disagree.
  {
    id: "next-page",
    label: "Next page",
    group: "Moving around",
    needsDocument: true,
    keys: ["right", "l"],
  },
  {
    id: "previous-page",
    label: "Previous page",
    group: "Moving around",
    needsDocument: true,
    keys: ["left", "h"],
  },
  {
    id: "scroll-down",
    label: "A little down",
    group: "Moving around",
    needsDocument: true,
    keys: ["down", "j"],
  },
  {
    id: "scroll-up",
    label: "A little up",
    group: "Moving around",
    needsDocument: true,
    keys: ["up", "k"],
  },
  // Vim spells these ⌃D and ⌃U, and that spelling cannot ship: on Windows and
  // Linux Ctrl is `mod`, so ⌃D is already dark mode there and a default that
  // works on one platform and collides on the other two is worse than no
  // default at all. d and u are what Sioyek uses, they are free everywhere,
  // and ⌃D is one line in `keys.toml` for anyone on a Mac who wants it.
  {
    id: "half-screen-down",
    label: "Half a screen down",
    group: "Moving around",
    needsDocument: true,
    keys: ["d"],
  },
  {
    id: "half-screen-up",
    label: "Half a screen up",
    group: "Moving around",
    needsDocument: true,
    keys: ["u"],
  },
  {
    id: "screen-down",
    label: "Down a screen",
    group: "Moving around",
    needsDocument: true,
    keys: ["space", "pagedown"],
  },
  {
    id: "screen-up",
    label: "Up a screen",
    group: "Moving around",
    needsDocument: true,
    keys: ["shift+space", "pageup"],
  },
  {
    id: "first-page",
    label: "First page",
    group: "Moving around",
    needsDocument: true,
    keys: ["home", "g g"],
  },
  {
    id: "last-page",
    label: "Last page",
    group: "Moving around",
    needsDocument: true,
    keys: ["end", "shift+g"],
  },
  // On p rather than on g. g belongs to `g g` and `G`, which are what a
  // reader arriving from Vim, Zathura or Sioyek will try first, and a lone g
  // that opened the page field would leave `g g` unreachable — a key cannot
  // both act at once and wait to see what follows it.
  {
    id: "go-to-page",
    label: "Go to page: type the number, press Enter",
    group: "Moving around",
    needsDocument: true,
    keys: ["mod+alt+g", "p"],
  },
  // Two traditions, and neither camp thinks to try the other's: ⌘[ and ⌘] are
  // Preview's, ⌥← and ⌥→ are Acrobat's, Sumatra's and Okular's.
  {
    id: "back",
    label: "Back to where you jumped from",
    group: "Moving around",
    keys: ["mod+[", "alt+left"],
  },
  { id: "forward", label: "Forward again", group: "Moving around", keys: ["mod+]", "alt+right"] },

  /* -------------------------------------------------------- looking at it */
  { id: "zoom-in", label: "Zoom in", group: "Looking at it", keys: ["mod++", "mod+="] },
  { id: "zoom-out", label: "Zoom out", group: "Looking at it", keys: ["mod+-"] },
  // ⌘0 stays fit width: it is this app's default and its best mode, and it is
  // what the button says. ⌘1 is actual size, which is Acrobat's.
  { id: "fit-width", label: "Fit the width of the window", group: "Looking at it", keys: ["mod+0"] },
  { id: "actual-size", label: "Actual size", group: "Looking at it", keys: ["mod+1"] },
  { id: "fit-page", label: "Fit the whole page", group: "Looking at it", keys: ["mod+2"] },
  // Preview's own pair, and free everywhere else.
  { id: "rotate-right", label: "Turn the page right", group: "Looking at it", keys: ["mod+r"] },
  { id: "rotate-left", label: "Turn the page left", group: "Looking at it", keys: ["mod+l"] },
  { id: "dark", label: "Dark mode", group: "Looking at it", keys: ["mod+d"] },
  { id: "sidebar", label: "Contents sidebar", group: "Looking at it", keys: ["mod+b"] },
  { id: "toolbar", label: "Toolbar", group: "Looking at it", keys: ["mod+t"] },
  {
    id: "fullscreen",
    label: "Full screen",
    group: "Looking at it",
    keys: ["f11", "mod+shift+f"],
    // The system's own gesture for it, which reaches the page as a chord like
    // any other and must not be read as a plain ⌘F.
    macKeys: ["mod+ctrl+f"],
  },
  {
    id: "present",
    label: "Presenting — full screen, nothing else on it",
    group: "Looking at it",
    keys: ["mod+shift+p"],
  },
];

export const GROUPS: ActionGroup[] = ["Documents", "Moving around", "Looking at it"];

const BY_ID = new Map<string, ActionSpec>(ACTIONS.map((spec) => [spec.id, spec]));

/** What an action answers to out of the box, on this machine. */
export function defaultKeys(spec: ActionSpec): string[] {
  return [...spec.keys, ...((isMac ? spec.macKeys : spec.otherKeys) ?? [])];
}

/* ---------------------------------------------------------------- chords */

/** Named keys, as they are spelled in a chord. Anything else is a single
 * character: `f`, `7`, `[`, `+`. */
const NAMES: Record<string, string> = {
  Escape: "escape",
  " ": "space",
  Enter: "enter",
  Tab: "tab",
  Backspace: "backspace",
  Delete: "delete",
  ArrowLeft: "left",
  ArrowRight: "right",
  ArrowUp: "up",
  ArrowDown: "down",
  PageUp: "pageup",
  PageDown: "pagedown",
  Home: "home",
  End: "end",
};

/** Spellings a person might reasonably reach for, mapped onto the one this
 * module uses. A chord is written by hand, so the notation is exactly what
 * will be got wrong. */
const ALIASES: Record<string, string> = {
  esc: "escape",
  return: "enter",
  spacebar: "space",
  arrowleft: "left",
  arrowright: "right",
  arrowup: "up",
  arrowdown: "down",
  "page-up": "pageup",
  "page-down": "pagedown",
  pgup: "pageup",
  pgdn: "pagedown",
  del: "delete",
  plus: "+",
  minus: "-",
  equals: "=",
  equal: "=",
  slash: "/",
  comma: ",",
  period: ".",
  dot: ".",
  semicolon: ";",
  backslash: "\\",
  backquote: "`",
  grave: "`",
};

/** `event.code` for the keys whose `event.key` a modifier can take away.
 *
 * Option is not a letter on a Mac: ⌥G arrives as ©, and the G is simply not
 * there to compare any more. So every event offers a second spelling taken
 * from its physical key, and a chord matches if either spelling does. This is
 * what the one `event.code` special case in the old handler was, generalised
 * — and it is why ⌥⌘G needs no special case at all now. */
const CODES: Record<string, string> = {
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Space: "space",
};

const MODIFIERS = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock"]);

function isLetter(name: string): boolean {
  return name.length === 1 && name >= "a" && name <= "z";
}

/** A chord's canonical spelling: modifiers in one fixed order, key lowercased,
 * aliases resolved. `null` when it is not a chord this app can read. */
export function parseChord(text: string): string | null {
  let rest = text.trim().toLowerCase();
  if (!rest) return null;
  let mod = false;
  let ctrl = false;
  let alt = false;
  let shift = false;
  // Peeled from the front rather than split on `+`, because `+` is also a key:
  // `mod++` is the zoom, and splitting would read it as two empty names.
  for (;;) {
    const found = /^(mod|cmd|command|meta|super|win|ctrl|control|alt|opt|option|shift)\+/.exec(
      rest,
    );
    if (!found) break;
    switch (found[1]) {
      case "mod":
      case "cmd":
      case "command":
      case "meta":
      case "super":
      case "win":
        mod = true;
        break;
      case "ctrl":
      case "control":
        // On Windows and Linux, Control *is* the modifier this app uses, so
        // there is no third thing for a literal `ctrl` to mean.
        if (isMac) ctrl = true;
        else mod = true;
        break;
      case "alt":
      case "opt":
      case "option":
        alt = true;
        break;
      default:
        shift = true;
        break;
    }
    rest = rest.slice(found[0].length);
  }
  const key = ALIASES[rest] ?? rest;
  const known =
    key.length === 1 || /^f([1-9]|1[0-2])$/.test(key) || Object.values(NAMES).includes(key);
  if (!known) return null;
  return spell({ mod, ctrl, alt, shift }, key);
}

type Mods = { mod: boolean; ctrl: boolean; alt: boolean; shift: boolean };

function spell(mods: Mods, key: string): string {
  return (
    (mods.mod ? "mod+" : "") +
    (mods.ctrl ? "ctrl+" : "") +
    (mods.alt ? "alt+" : "") +
    (mods.shift ? "shift+" : "") +
    key
  );
}

/** A binding: one chord, or several pressed in turn. `null` when any part of
 * it is unreadable, because half a sequence is not a shorter one. */
export function parseBinding(text: string): string | null {
  const parts = text.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return null;
  const chords: string[] = [];
  for (const part of parts) {
    const chord = parseChord(part);
    if (chord === null) return null;
    chords.push(chord);
  }
  return chords.join(" ");
}

/** Every spelling of the key that was just pressed, best first.
 *
 * Best first matters: ⇧Space offers `shift+space` before `space`, so a binding
 * that wants the shifted one gets it and a binding that does not still
 * answers. Shift is only dropped for keys that are not letters — G and g are
 * two different keys to a reader, and `shift+g` must never fall through to
 * `g`. */
export function chordsOf(event: KeyboardEvent): string[] {
  if (MODIFIERS.has(event.key)) return [];
  // The Windows and Super keys are not bound to anything, and reading one as
  // no modifier at all would turn ⊞J into a scroll.
  if (!isMac && event.metaKey) return [];
  const mods: Mods = {
    mod: isMac ? event.metaKey : event.ctrlKey,
    ctrl: isMac && event.ctrlKey,
    alt: event.altKey,
    shift: event.shiftKey,
  };

  const names: string[] = [];
  const push = (name: string | undefined) => {
    if (name && !names.includes(name)) names.push(name);
  };
  push(NAMES[event.key] ?? (event.key.length === 1 ? event.key.toLowerCase() : undefined));
  if (/^F([1-9]|1[0-2])$/.test(event.key)) push(event.key.toLowerCase());
  const code = event.code;
  if (/^Key[A-Z]$/.test(code)) push(code.slice(3).toLowerCase());
  else if (/^Digit[0-9]$/.test(code)) push(code.slice(5));
  else push(CODES[code]);

  const out: string[] = [];
  for (const name of names) out.push(spell(mods, name));
  if (mods.shift) {
    for (const name of names) {
      if (isLetter(name)) continue;
      const without = spell({ ...mods, shift: false }, name);
      if (!out.includes(without)) out.push(without);
    }
  }
  return out;
}

/* --------------------------------------------------------------- keymaps */

export type Keymap = {
  /** Binding → the action it asks for. */
  byBinding: Map<string, Action>;
  /** Every binding of every action, in the order they are offered. */
  byAction: Map<Action, string[]>;
  /** Chords that begin a longer binding and mean nothing on their own. */
  prefixes: Set<string>;
  /** Lines of `keys.toml` this app could not use, in the words the reader
      needs to fix them. Empty is the normal case. */
  problems: string[];
};

/** The bindings in force: the defaults, with the reader's file over the top.
 *
 * Naming an action in the file *replaces* its keys rather than adding to
 * them, and naming it with an empty list unbinds it. Anything the file does
 * not mention keeps what it shipped with — so a file that rebinds one key
 * stays one line long, and a key added in a later version arrives rather than
 * being frozen out by a file written before it existed.
 *
 * Nothing here throws. A file somebody wrote by hand is a file this app does
 * not own, and the answer to a line it cannot read is to say so and carry on
 * with the rest — the same answer `unreadableColors` gives a theme. */
export function buildKeymap(overrides: Record<string, string[]> = {}): Keymap {
  const problems: string[] = [];
  const byAction = new Map<Action, string[]>();

  for (const name of Object.keys(overrides)) {
    if (!BY_ID.has(name)) problems.push(`${name} is not something HyloPDF can do.`);
  }

  for (const spec of ACTIONS) {
    const given = overrides[spec.id];
    if (given === undefined) {
      byAction.set(spec.id, defaultKeys(spec));
      continue;
    }
    const chords: string[] = [];
    for (const text of given) {
      const binding = parseBinding(text);
      if (binding === null) problems.push(`${spec.id}: "${text}" is not a key HyloPDF can read.`);
      else if (!chords.includes(binding)) chords.push(binding);
    }
    byAction.set(spec.id, chords);
  }

  // One chord, one action. Which one is not a matter of the order the tests
  // happen to be written in any more, so a chord claimed twice is a thing the
  // reader is told about — and the one they wrote themselves wins, because
  // the other one is ours.
  const byBinding = new Map<string, Action>();
  const fromFile = new Set<string>(Object.keys(overrides));
  for (const spec of ACTIONS) {
    for (const binding of byAction.get(spec.id) ?? []) {
      const taken = byBinding.get(binding);
      if (taken === undefined) {
        byBinding.set(binding, spec.id);
        continue;
      }
      const loser = fromFile.has(spec.id) && !fromFile.has(taken) ? taken : spec.id;
      const winner = loser === spec.id ? taken : spec.id;
      if (loser === taken) byBinding.set(binding, spec.id);
      problems.push(
        `${describeBinding(binding)} is asked to do two things — ${winner} has it, ${loser} does not.`,
      );
      dropBinding(byAction, loser, binding);
    }
  }

  // A chord that both does something and begins something longer would have
  // to wait to find out which, and a key that waits is a key that feels
  // broken. The shorter one keeps it: acting the moment it is pressed is the
  // promise every other key here makes.
  for (const binding of [...byBinding.keys()]) {
    const chords = binding.split(" ");
    for (let i = 1; i < chords.length; i++) {
      const head = chords.slice(0, i).join(" ");
      if (!byBinding.has(head)) continue;
      problems.push(
        `${describeBinding(binding)} can never be pressed: ${describeBinding(head)} ` +
          `already does something on its own.`,
      );
      dropBinding(byAction, byBinding.get(binding)!, binding);
      byBinding.delete(binding);
      break;
    }
  }

  // What is left of the sequences: the chords that mean "wait, there is more".
  const prefixes = new Set<string>();
  for (const binding of byBinding.keys()) {
    const chords = binding.split(" ");
    for (let i = 1; i < chords.length; i++) prefixes.add(chords.slice(0, i).join(" "));
  }

  return { byBinding, byAction, prefixes, problems };
}

function dropBinding(byAction: Map<Action, string[]>, action: Action, binding: string): void {
  const kept = (byAction.get(action) ?? []).filter((each) => each !== binding);
  byAction.set(action, kept);
}

/** What needs a document open, asked of an action's name. */
export function needsDocument(action: Action): boolean {
  return BY_ID.get(action)?.needsDocument === true;
}

/* -------------------------------------------------------------- for eyes */

const SHOWN: Record<string, string> = {
  left: "←",
  right: "→",
  up: "↑",
  down: "↓",
  pageup: "Page Up",
  pagedown: "Page Down",
  escape: "Escape",
  space: "Space",
  enter: "Enter",
  tab: "Tab",
  home: "Home",
  end: "End",
  backspace: "Backspace",
  delete: "Delete",
  "-": "−",
};

/** A chord as it should be read: ⌘⇧F on a Mac, Ctrl+Shift+F elsewhere. */
export function describeChord(chord: string): string {
  const parsed = /^(mod\+)?(ctrl\+)?(alt\+)?(shift\+)?(.*)$/.exec(chord);
  if (!parsed) return chord;
  const [, mod, ctrl, alt, shift, key = ""] = parsed;
  // A bare ⇧ and a letter is the letter, capitalised: nobody reads G as ⇧G.
  if (!mod && !ctrl && !alt && shift && isLetter(key)) return key.toUpperCase();
  const name = /^f\d+$/.test(key)
    ? key.toUpperCase()
    : (SHOWN[key] ?? (isLetter(key) && (mod || ctrl || alt) ? key.toUpperCase() : key));
  return prefix(mod, ctrl, alt, shift) + name;
}

function prefix(mod?: string, ctrl?: string, alt?: string, shift?: string): string {
  if (isMac) {
    // Apple's order, which is not the order a chord is written in.
    return (ctrl ? "⌃" : "") + (alt ? "⌥" : "") + (shift ? "⇧" : "") + (mod ? "⌘" : "");
  }
  return (
    (mod ? "Ctrl+" : "") + (ctrl ? "Ctrl+" : "") + (alt ? "Alt+" : "") + (shift ? "Shift+" : "")
  );
}

/** A whole binding, sequences included: `g g` reads as "g then g". */
export function describeBinding(binding: string): string {
  return binding.split(" ").map(describeChord).join(" ");
}
