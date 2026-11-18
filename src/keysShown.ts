// Whether a window shows keyboard shortcuts beside what they do: all the time where Settings says
// so (`[appearance] show_keys`, `Board.showKeys`), and otherwise only while ⌘ or ⌃ (Ctrl where
// there's no ⌘) is held alone a moment, as a cheat sheet over the window. Tooltips and menus say
// keys either way. The page's root says which (`data-keys`: `always`, `held`, or none), which
// controls.css hides keycaps and the lines that only say keys (`.ca-keyhint`) by; what can't be
// hidden by a style (a placeholder) asks `useKeysAlways`.

import { useEffect, useSyncExternalStore } from 'react';

/** How long ⌘ or ⌃ is held alone before the keys show: a chord pressed at once shows nothing. */
export const HOLD_MS = 450;

/** The keys whose hold shows the keys. */
const SHOWS = new Set(['Meta', 'Control']);
/** Keys held with them that still wait: ⇧ and ⌥ start chords whose keys are worth seeing too. */
const WAITS = new Set(['Shift', 'Alt']);

/** What the keys pressed say about showing the keys, on a clock of its caller's: a press of ⌘ or ⌃
 *  alone starts the wait, any other key before it's over calls it off (a chord, not a look), and
 *  letting go, or the window losing the keyboard, hides them. */
export class Hold {
  private since: number | null = null;
  private shown = false;

  down(key: string, now: number): void {
    if (SHOWS.has(key)) {
      if (this.since === null && !this.shown) this.since = now;
    } else if (!WAITS.has(key)) {
      this.since = null;
    }
  }

  up(key: string): void {
    if (SHOWS.has(key)) this.lost();
  }

  lost(): void {
    this.since = null;
    this.shown = false;
  }

  /** Whether the keys show at `now`: once the hold has lasted, they show until it ends. */
  at(now: number): boolean {
    if (!this.shown && this.since !== null && now - this.since >= HOLD_MS) this.shown = true;
    return this.shown;
  }
}

let always = false;
const heard = new Set<() => void>();

function say(held: boolean) {
  const root = document.documentElement;
  if (always) root.dataset.keys = 'always';
  else if (held) root.dataset.keys = 'held';
  else delete root.dataset.keys;
}

/** Keeps the window's page showing keys as Settings says (`on`, the board's `showKeys`; a window
 *  without a board passes nothing, and its keys show only while held), and while ⌘ or ⌃ is held.
 *  Each window's root calls it once. */
export function useKeysShown(on: boolean | undefined) {
  useEffect(() => {
    if (always === !!on) return;
    always = !!on;
    say(false);
    heard.forEach((f) => f());
  }, [on]);
  useEffect(() => {
    const hold = new Hold();
    let timer: number | undefined;
    const show = () => say(hold.at(Date.now()));
    const down = (e: KeyboardEvent) => {
      hold.down(e.key, Date.now());
      window.clearTimeout(timer);
      if (SHOWS.has(e.key)) timer = window.setTimeout(show, HOLD_MS);
    };
    const up = (e: KeyboardEvent) => {
      hold.up(e.key);
      if (SHOWS.has(e.key)) {
        window.clearTimeout(timer);
        say(false);
      }
    };
    const lost = () => {
      hold.lost();
      window.clearTimeout(timer);
      say(false);
    };
    // Capturing, so a key a field or a list takes for itself still counts.
    window.addEventListener('keydown', down, true);
    window.addEventListener('keyup', up, true);
    window.addEventListener('blur', lost);
    say(false);
    return () => {
      window.removeEventListener('keydown', down, true);
      window.removeEventListener('keyup', up, true);
      window.removeEventListener('blur', lost);
      window.clearTimeout(timer);
    };
  }, []);
}

/** Whether keys show all the time, for what says them where a style can't hide them. */
export function useKeysAlways(): boolean {
  return useSyncExternalStore(
    (f) => {
      heard.add(f);
      return () => heard.delete(f);
    },
    () => always,
  );
}
