// Whether a click on a row is one you aimed. A list whose rows come and go while you look at it
// (the quick panel's, as Jev's suggestions and the search of what sessions said come in) can
// move another row under a pointer that's still, and the click meant for the row that was there
// would do the one that came. So a click does a row only when the pointer was last moved onto
// that row; a row that came under it some other way is selected by the click and done by the
// next. A move is told from the mouse events a browser may send as content moves under a still
// pointer by where the pointer is on the screen. Scrolling the list moves rows under a still
// pointer too, but you did that: the row it brings there is the one you aim at, until the list
// itself changes. Kept free of React so the rules have tests.

/** The row the pointer was last moved onto (null off every row), and where it was then. */
export interface Aim {
  key: string | null;
  x: number;
  y: number;
}

/** The key of an aim at whichever row a scroll of yours brought under the pointer. */
const SCROLLED = '\u0000scrolled';

/** The aim after a mouse event over `key` at (x, y) on the screen, and whether the pointer moved:
 *  one that didn't leaves the aim as it was, and hovering shouldn't select what's under it. The
 *  first event after you scrolled counts as a move. */
export function aimAfter(aim: Aim | null, key: string | null, x: number, y: number): { aim: Aim; moved: boolean } {
  if (aim && aim.key !== SCROLLED && aim.x === x && aim.y === y) return { aim, moved: false };
  return { aim: { key, x, y }, moved: true };
}

/** Whether a click on `key` does it, rather than selecting it. */
export const clickDoes = (aim: Aim | null, key: string): boolean => aim?.key === key || aim?.key === SCROLLED;

/** The aim once a click at (x, y) on the screen has selected `key` without doing it, so the next
 *  click there does it: you've now seen what it is. */
export const aimedAt = (key: string, x: number, y: number): Aim => ({ key, x, y });

/** The aim as the list is shown afresh, or the pointer leaves it: on no row, where the pointer
 *  was, so only a move picks one again. */
export const aimedAtNothing = (aim: Aim | null): Aim | null => (aim ? { ...aim, key: null } : null);

/** The aim once you've scrolled the list: whichever row comes under the pointer. */
export const aimedByScroll = (aim: Aim | null): Aim => ({ key: SCROLLED, x: aim?.x ?? NaN, y: aim?.y ?? NaN });

/** The aim once the list's rows have moved by themselves: a scroll aimed at the rows where they
 *  were, and aims at none now. An aim at a row stays on it, wherever it went. */
export const aimAfterChange = (aim: Aim | null): Aim | null => (aim?.key === SCROLLED ? { ...aim, key: null } : aim);
