/* Small pieces of interface: menus that hang off a button, switches, the
   settings window, and the one-line notice at the bottom of the screen.
 *
 * Nothing here appears on its own. A menu is dismissed by Escape, by clicking
 * elsewhere, or by choosing something; the window, which is the one thing that
 * does sit in front of the document, only ever opens because someone asked
 * for it, and closes the same three ways. */

import { hydrateIcons, iconMarkup } from "./icons";
import { parseColor, toHex } from "./themes";

let openMenu: (() => void) | null = null;
let openAnchor: HTMLElement | null = null;

export function closeMenus(): void {
  openMenu?.();
}

export function isMenuOpen(): boolean {
  return openMenu !== null;
}

export function showPopover(
  anchor: HTMLElement,
  build: (close: () => void) => HTMLElement,
  align: "left" | "right" = "left",
): void {
  // The button that opened a menu closes it again: pressing Theme twice should
  // leave nothing behind, not blink the same menu back into place.
  const reopening = openAnchor === anchor;
  closeMenus();
  if (reopening) return;

  const popover = document.createElement("div");
  popover.className = "popover";
  const close = () => {
    popover.remove();
    document.removeEventListener("pointerdown", onPointerDown, true);
    document.removeEventListener("keydown", onKeyDown, true);
    window.removeEventListener("resize", close);
    if (openMenu === close) {
      openMenu = null;
      openAnchor = null;
    }
    anchor.setAttribute("aria-expanded", "false");
  };

  const onPointerDown = (event: PointerEvent) => {
    const target = event.target as Node;
    if (!popover.contains(target) && !anchor.contains(target)) close();
  };
  /** Everything in the menu that can be pressed, in the order it is read. */
  const items = (): HTMLElement[] =>
    [...popover.querySelectorAll<HTMLElement>("button, [href], input, select")].filter(
      (item) => !item.hasAttribute("disabled"),
    );

  const step = (by: number) => {
    const all = items();
    if (all.length === 0) return;
    const at = all.indexOf(document.activeElement as HTMLElement);
    // Off the list entirely — arriving from the button that opened the menu —
    // means starting at whichever end the key is pointing away from.
    const next = at < 0 ? (by > 0 ? 0 : all.length - 1) : (at + by + all.length) % all.length;
    all[next].focus();
  };

  const onKeyDown = (event: KeyboardEvent) => {
    switch (event.key) {
      case "Escape":
        event.stopPropagation();
        event.preventDefault();
        close();
        return;
      case "ArrowDown":
        step(1);
        break;
      case "ArrowUp":
        step(-1);
        break;
      case "Home":
        items()[0]?.focus();
        break;
      case "End":
        items().at(-1)?.focus();
        break;
      case "Tab":
        // A menu is a place you are in, not a stop on the way through the
        // page: Tab moves within it and Escape is how you leave.
        step(event.shiftKey ? -1 : 1);
        break;
      default:
        return;
    }
    event.stopPropagation();
    event.preventDefault();
  };

  popover.append(build(close));
  document.getElementById("popovers")!.append(popover);
  hydrateIcons(popover);

  const rect = anchor.getBoundingClientRect();
  const width = popover.offsetWidth;
  const left =
    align === "right"
      ? Math.min(rect.right - width, window.innerWidth - width - 8)
      : Math.min(rect.left, window.innerWidth - width - 8);
  popover.style.left = `${Math.max(8, left)}px`;
  popover.style.top = `${rect.bottom + 6}px`;
  const overflow = rect.bottom + 6 + popover.offsetHeight - window.innerHeight + 8;
  if (overflow > 0) popover.style.maxHeight = `${popover.offsetHeight - overflow}px`;

  anchor.setAttribute("aria-expanded", "true");
  document.addEventListener("pointerdown", onPointerDown, true);
  document.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("resize", close);
  openMenu = close;
  openAnchor = anchor;

  // Opened from the keyboard, the menu takes the keyboard with it. Opened by a
  // click it does not, because moving the focus would put a ring around the
  // first item of a menu somebody is already pointing at.
  if (anchor.matches(":focus-visible")) items()[0]?.focus();
}

export function section(title: string): HTMLElement {
  const element = document.createElement("div");
  element.className = "popover-section";
  element.textContent = title;
  return element;
}

export function divider(): HTMLElement {
  const element = document.createElement("div");
  element.className = "popover-divider";
  return element;
}

export type MenuItemOptions = {
  label: string;
  note?: string;
  icon?: string;
  checked?: boolean;
  lead?: HTMLElement;
  trail?: HTMLElement;
  onSelect?: () => void;
};

export function menuItem(options: MenuItemOptions): HTMLButtonElement {
  const button = document.createElement("button");
  button.className = "popover-item";
  if (options.checked) button.classList.add("current");

  const check = document.createElement("span");
  check.className = "check";
  check.innerHTML = options.checked ? iconMarkup("check") : "";
  button.append(check);
  if (options.lead) button.append(options.lead);

  if (options.icon) {
    const icon = document.createElement("span");
    icon.dataset.icon = options.icon;
    icon.style.display = "flex";
    button.append(icon);
  }

  const label = document.createElement("span");
  label.className = "popover-label";
  label.textContent = options.label;
  button.append(label);

  if (options.note) {
    const note = document.createElement("span");
    note.className = "popover-note";
    note.textContent = options.note;
    button.append(note);
  }
  if (options.trail) button.append(options.trail);
  if (options.onSelect) button.addEventListener("click", options.onSelect);
  return button;
}

/** A theme, two letters wide.
 *
 * The colours go through the same reader the page does. Handing the raw
 * strings to CSS meant the browser understood things the renderer does not —
 * `steelblue` showed as steel blue here and rendered as black on the page, so
 * the one place in the app that is supposed to show you what you are about to
 * get was the one place that lied about it. */
export function swatch(text: string, background: string, letter = "A"): HTMLElement {
  const element = document.createElement("span");
  element.className = "swatch";
  element.style.background = toHex(parseColor(background, [255, 255, 255]));
  element.style.color = toHex(parseColor(text));
  element.textContent = letter;
  return element;
}

export function row(label: string, control: HTMLElement, note?: string): HTMLElement {
  const element = document.createElement("div");
  element.className = "popover-row";
  const text = document.createElement("label");
  text.append(label);
  if (note) {
    const hint = document.createElement("span");
    hint.className = "popover-note";
    hint.textContent = note;
    text.append(hint);
  }
  element.append(text, control);
  return element;
}

export function toggle(on: boolean, onChange: (value: boolean) => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.className = "switch";
  button.setAttribute("aria-pressed", String(on));
  button.addEventListener("click", () => {
    const next = button.getAttribute("aria-pressed") !== "true";
    button.setAttribute("aria-pressed", String(next));
    onChange(next);
  });
  return button;
}

export function stepper(
  value: number,
  range: { min: number; max: number; step: number },
  onChange: (value: number) => void,
  format: (value: number) => string = String,
): HTMLElement {
  const group = document.createElement("div");
  group.className = "zoom-group";
  const readout = document.createElement("span");
  readout.className = "btn zoom-level";
  readout.textContent = format(value);

  const make = (icon: string, delta: number) => {
    const button = document.createElement("button");
    button.className = "btn icon-only";
    button.dataset.icon = icon;
    button.addEventListener("click", () => {
      value = Math.max(range.min, Math.min(range.max, value + delta));
      readout.textContent = format(value);
      onChange(value);
    });
    return button;
  };

  group.append(make("minus", -range.step), readout, make("plus", range.step));
  return group;
}

export function textField(value: string, onInput: (value: string) => void): HTMLInputElement {
  const input = document.createElement("input");
  input.type = "text";
  input.value = value;
  input.addEventListener("input", () => onInput(input.value));
  return input;
}

/** A colour, as a swatch and as the hex behind it.
 *
 * `show` is on it because one colour in the theme editor is derived from
 * another: moving the selection area moves the ink on it, and the field that
 * did not move still has to say what it is now. */
export type ColorField = HTMLElement & { show(value: string): void };

export function colorField(value: string, onInput: (value: string) => void): ColorField {
  const wrap = document.createElement("span");
  wrap.style.display = "flex";
  wrap.style.gap = "6px";
  wrap.style.alignItems = "center";

  const picker = document.createElement("input");
  picker.type = "color";
  picker.value = value;
  const text = document.createElement("input");
  text.type = "text";
  text.value = value;
  text.style.width = "84px";

  picker.addEventListener("input", () => {
    text.value = picker.value;
    onInput(picker.value);
  });
  text.addEventListener("input", () => {
    if (/^#[0-9a-f]{6}$/i.test(text.value)) {
      picker.value = text.value;
      onInput(text.value);
    }
  });

  wrap.append(picker, text);
  return Object.assign(wrap, {
    show(next: string) {
      picker.value = next;
      text.value = next;
    },
  });
}

export function actions(...buttons: HTMLElement[]): HTMLElement {
  const element = document.createElement("div");
  element.className = "popover-actions";
  element.append(...buttons);
  return element;
}

export function button(
  label: string,
  onClick: () => void,
  variant: "plain" | "primary" | "danger" = "plain",
): HTMLButtonElement {
  const element = document.createElement("button");
  element.className =
    variant === "primary" ? "btn btn-primary" : variant === "danger" ? "btn btn-danger" : "btn";
  element.textContent = label;
  element.addEventListener("click", onClick);
  return element;
}

/* ------------------------------------------------------------------ window */

// Almost always one at a time — but a confirmation (`confirmDeleteTheme`) has
// to stand in front of Settings without evicting it, so this is a stack
// rather than a single slot. Only the topmost window's own key handler acts;
// the others below it just let the event through to it.
const windowStack: (() => void)[] = [];

export function isWindowOpen(): boolean {
  return windowStack.length > 0;
}

export function closeWindow(): void {
  windowStack.at(-1)?.();
}

/** A panel in the middle of the screen with a title bar of its own: the one
    piece of interface allowed to sit in front of the document, because it is
    only ever there on request. */
export function showWindow(
  title: string,
  build: (close: () => void) => HTMLElement,
  onClose?: () => void,
  size: "fit" | "full" = "fit",
): void {
  closeMenus();

  const returnFocusTo = document.activeElement as HTMLElement | null;
  const scrim = document.createElement("div");
  scrim.className = "window-scrim";

  const frame = document.createElement("div");
  frame.className = "window";
  frame.dataset.size = size;
  frame.tabIndex = -1;
  frame.setAttribute("role", "dialog");
  frame.setAttribute("aria-modal", "true");
  frame.setAttribute("aria-label", title);

  const close = () => {
    scrim.remove();
    document.removeEventListener("keydown", onKeyDown, true);
    const at = windowStack.indexOf(close);
    if (at !== -1) windowStack.splice(at, 1);
    onClose?.();
    returnFocusTo?.focus();
  };

  const focusable = (): HTMLElement[] =>
    [...frame.querySelectorAll<HTMLElement>(
      "button, [href], input, select, textarea, [tabindex]:not([tabindex='-1'])",
    )].filter((item) => !item.hasAttribute("disabled") && item.offsetParent !== null);

  const onKeyDown = (event: KeyboardEvent) => {
    if (windowStack.at(-1) !== close) return;
    if (event.key === "Escape") {
      event.stopPropagation();
      event.preventDefault();
      close();
      return;
    }
    // Keep Tab inside the window. It says `aria-modal`, and a window that
    // claims to be modal and then lets the keyboard walk out behind the scrim
    // — into a document nobody can see the focus ring on — is worse than one
    // that never claimed it.
    if (event.key !== "Tab") return;
    const all = focusable();
    if (all.length === 0) return;
    const first = all[0];
    const last = all[all.length - 1];
    const active = document.activeElement as HTMLElement | null;
    if (!frame.contains(active)) {
      event.preventDefault();
      (event.shiftKey ? last : first).focus();
    } else if (event.shiftKey && active === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  };

  const bar = document.createElement("header");
  bar.className = "window-bar";
  const heading = document.createElement("span");
  heading.className = "window-title";
  heading.textContent = title;
  const dismiss = document.createElement("button");
  dismiss.className = "btn icon-only";
  dismiss.dataset.icon = "close";
  dismiss.title = "Close";
  dismiss.setAttribute("aria-label", "Close");
  dismiss.addEventListener("click", close);
  bar.append(heading, dismiss);

  frame.append(bar, build(close));
  scrim.append(frame);
  scrim.addEventListener("pointerdown", (event) => {
    if (event.target === scrim) close();
  });

  document.getElementById("windows")!.append(scrim);
  hydrateIcons(scrim);
  document.addEventListener("keydown", onKeyDown, true);
  frame.focus();
  windowStack.push(close);
}

/** Ask for a document's password.
 *
 * An encrypted PDF is not a broken one, and it used to be reported as though
 * it were: the load rejected, the app went back to the start screen, and the
 * notice said something had gone wrong. Nothing had. Resolves to null if the
 * reader would rather not, which is a perfectly good answer and closes the
 * document quietly.
 */
export function askForPassword(wrong: boolean): Promise<string | null> {
  return new Promise((resolve) => {
    let answered: string | null = null;

    showWindow(
      "This document is locked",
      (close) => {
        const body = document.createElement("div");
        body.className = "window-ask";

        const field = document.createElement("input");
        field.type = "password";
        field.autocomplete = "off";
        field.setAttribute("aria-label", "Password");

        const submit = () => {
          answered = field.value;
          close();
        };

        field.addEventListener("keydown", (event) => {
          if (event.key !== "Enter") return;
          event.preventDefault();
          submit();
        });

        body.append(
          text(
            "lede",
            wrong
              ? "That password was not right. Try again."
              : "It needs a password before it can be opened.",
          ),
          field,
          actions(
            button("Not now", close),
            button("Open", submit, "primary"),
          ),
        );

        // The field is the only thing anyone came here to use.
        queueMicrotask(() => field.focus());
        return body;
      },
      () => resolve(answered),
    );
  });
}

/** Ask before a theme is gone for good. Resolves to whether the reader
    confirmed the deletion. */
export function confirmDeleteTheme(name: string): Promise<boolean> {
  return new Promise((resolve) => {
    let confirmed = false;

    showWindow(
      "Delete theme",
      (close) => {
        const body = document.createElement("div");
        body.className = "window-ask";

        body.append(
          text("lede", `Do you really want to delete theme ${name}?`),
          actions(
            button("Cancel", close),
            button(
              "Delete",
              () => {
                confirmed = true;
                close();
              },
              "danger",
            ),
          ),
        );

        return body;
      },
      () => resolve(confirmed),
    );
  });
}

/** A labelled line in a window: what the setting is on the left, the control
    that changes it on the right. */
export function field(
  label: string,
  control: HTMLElement,
  note?: string,
): HTMLElement {
  const element = document.createElement("div");
  element.className = "field";

  const text = document.createElement("div");
  text.className = "field-text";
  const name = document.createElement("span");
  name.className = "field-label";
  name.textContent = label;
  text.append(name);
  if (note) {
    const hint = document.createElement("span");
    hint.className = "field-note";
    hint.textContent = note;
    text.append(hint);
  }

  const holder = document.createElement("div");
  holder.className = "field-control";
  holder.append(control);

  element.append(text, holder);
  return element;
}

/** A row of buttons where exactly one is on: for a setting with two or three
    answers, all of which are worth reading at a glance. */
export function segmented<T extends string>(
  choices: { value: T; label: string; icon?: string }[],
  current: T,
  onChange: (value: T) => void,
): HTMLElement {
  const group = document.createElement("div");
  group.className = "segmented";

  for (const choice of choices) {
    const option = document.createElement("button");
    option.className = "btn";
    if (choice.icon) option.dataset.icon = choice.icon;
    option.append(choice.label);
    option.setAttribute("aria-pressed", String(choice.value === current));
    option.addEventListener("click", () => {
      for (const sibling of group.children) {
        sibling.setAttribute("aria-pressed", String(sibling === option));
      }
      onChange(choice.value);
    });
    group.append(option);
  }
  return group;
}

/** A heading, a paragraph of explanation, or the quiet line of small print at
    the bottom of a pane. */
export function text(
  kind: "title" | "lede" | "group" | "note",
  content: string,
): HTMLElement {
  const element = document.createElement(kind === "title" ? "h2" : "p");
  element.className = `pane-${kind}`;
  element.textContent = content;
  return element;
}

let noticeTimer = 0;

/** The one line at the bottom of the screen.
 *
 * `done` marks something that worked, and gets a green tick in front of it.
 * The line itself keeps the same quiet surface either way: a whole panel
 * turning green over a copied file name would be a lot of colour for very
 * little news. */
export function notice(message: string, kind: "plain" | "done" = "plain"): void {
  const element = document.getElementById("notice");
  if (!element) return;
  // Unhidden first, then filled. It is a live region, and a `hidden` one is
  // out of the accessibility tree entirely — so a message written before the
  // element came back is a message nothing announces.
  element.hidden = false;
  element.replaceChildren();
  if (kind === "done") {
    const tick = document.createElement("span");
    tick.className = "notice-tick";
    tick.innerHTML = iconMarkup("check");
    element.append(tick);
  }
  element.append(message);
  window.clearTimeout(noticeTimer);
  noticeTimer = window.setTimeout(() => {
    element.hidden = true;
  }, 4200);
}
