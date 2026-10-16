// Where ⌘F goes in a window: to what finds where you are, as VS Code's ⌘F finds in the editor or
// filters the sidebar, whichever has the keyboard. Each place that finds marks the element around
// it with `data-find`: a session's panel or card (its conversation, ui/ThreadFind.tsx), a PR's
// panel (its files, ui/PrFiles.tsx) and the main window's board (its list's find, ui/ListBar.tsx:
// `data-find="window"`, mainKeys.ts). ⌘F goes to the one that holds the keyboard; with nothing
// holding it, as after a click on a conversation's text or a field let go of with Escape, to the
// one the keyboard or a click was in last; and from anywhere else to the window's board. A window
// with no board, or one whose panel takes the whole window, gives it to the first that shows: a
// card's conversation. ⌥⌘F always opens the list's find, as in Mail and Notes (mainKeys.ts).

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

/** Still a place that finds, and on screen: the filter stops being one while a panel takes the
 *  whole window. */
const shown = (el: Element) => el.isConnected && el.matches(AREA) && el.getClientRects().length > 0;

/** The place that finds ⌘F is for in this window, or none. */
export function findArea(): Element | null {
  const at = document.activeElement;
  const here = at && at !== document.body ? at.closest(AREA) : last;
  if (here && shown(here)) return here;
  const areas = [...document.querySelectorAll(AREA)].filter(shown);
  return areas.find((el) => el.getAttribute('data-find') === 'window') ?? areas[0] ?? null;
}

/** Whether ⌘F is for the place that finds around `el`. */
export function findsAt(el: Element | null | undefined): boolean {
  const area = el?.closest(AREA);
  return !!area && area === findArea();
}
