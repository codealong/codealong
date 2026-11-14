// Where you are in a window, for its finds. ⌘F finds in the whole window (ui/WindowFind.tsx),
// starting where you are; ⌥⌘F filters where you are, as Mail and Notes filter their lists: a PR's
// files in its panel or card (ui/PrFiles.tsx), else the main window's list (ui/ListBar.tsx,
// mainKeys.ts), which / opens too. Each place marks the element around it with `data-find`: a
// session's panel or card (its conversation), a PR's panel or card (`files`, its files' filter)
// and the main window's board (`window`, the list's). You're in the one that holds the keyboard;
// with nothing holding it, as after a click on a conversation's text or a field let go of with
// Escape, the one the keyboard or a click was in last; and anywhere else, the window's board. A
// window with no board, or one whose panel takes the whole window, has you in the first that
// shows: a card's conversation.

import { held } from './platform';

const AREA = '[data-find]';

/** The place the keyboard or a click was in last, for when nothing holds the keyboard. */
let last: Element | null = null;
if (typeof window !== 'undefined') {
  const note = (e: Event) => {
    last = e.target instanceof Element ? e.target.closest(AREA) : null;
  };
  window.addEventListener('focusin', note, true);
  window.addEventListener('pointerdown', note, true);
}

/** Still a place that finds, and on screen: the board stops being one while a panel takes the
 *  whole window. */
const shown = (el: Element) => el.isConnected && el.matches(AREA) && el.getClientRects().length > 0;

/** The place you are in this window, or none. */
export function findArea(): Element | null {
  const at = document.activeElement;
  const here = at && at !== document.body ? at.closest(AREA) : last;
  if (here && shown(here)) return here;
  const areas = [...document.querySelectorAll(AREA)].filter(shown);
  return areas.find((el) => el.getAttribute('data-find') === 'window') ?? areas[0] ?? null;
}

/** Whether you're in the place around `el`. Not while a dialog is up that it isn't in, even one
 *  drawn inside its place (a card's, primitives.tsx `DialogHost`): what's typed next is the
 *  dialog's. */
export function findsAt(el: Element | null | undefined): boolean {
  const modal = document.querySelector('[aria-modal="true"]');
  if (modal && el && !modal.contains(el)) return false;
  const area = el?.closest(AREA);
  return !!area && area === findArea();
}

/** ⌥⌘F, by the key's code: ⌥ changes the character it types. */
export function filterPressed(e: Pick<KeyboardEvent, 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>): boolean {
  return held(e, 'Alt', 'Cmd') && e.code === 'KeyF';
}
