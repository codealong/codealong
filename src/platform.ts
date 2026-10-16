// The platform the windows run on, and keys as it has them: the one place that knows. The app's
// own chords are written in macOS's terms ('Shift+Cmd+E', 'Ctrl+Cmd+W', menus.ts, queue.ts), and
// everything that matches one (`chordPressed`, `held`, `cmd`) or says one (`chordKeys`,
// `keysIn`) asks here, so on macOS they're pressed and shown as they're written and on Linux as
// `platformChord` makes them.
//
// On Linux a page sees only Ctrl, Alt and Shift: WebKitGTK reports no Super to a page at all, no
// `metaKey` and no `getModifierState` (2.52 under X11, Super on mod4, checked in the VM on
// 2026-10-16). macOS's four modifiers can't go one to one onto three, so ⌘ is Ctrl, ⌥ and ⌃ are
// both Alt, ⇧ is Shift, and `LINUX` names the few chords that would then be another's, or a key
// the desktop keeps. A test holds every chord the app has to being distinct there.

export type Platform = 'mac' | 'linux';

declare global {
  interface Window {
    /** Set by a preview (scripts/preview.sh's PREVIEW_PLATFORM) to draw the windows as another
     *  platform has them. */
    __CODEALONG_PLATFORM__?: Platform;
  }
}

let forced: Platform | null = null;

/** Reads and says keys as `p` has them, for tests; null goes back to the user agent's. */
export function setPlatform(p: Platform | null): void {
  forced = p;
}

/** Linux when WebKitGTK's user agent says so (`X11; Linux`); macOS otherwise, which WKWebView is
 *  and the headless Chrome the preview renders in says it is. Tests run in Node, whose user agent
 *  names no platform. */
export function platform(): Platform {
  if (forced) return forced;
  if (typeof window !== 'undefined' && window.__CODEALONG_PLATFORM__) return window.__CODEALONG_PLATFORM__;
  return typeof navigator !== 'undefined' && /\bLinux\b/.test(navigator.userAgent ?? '') ? 'linux' : 'mac';
}

/** A modifier as the app's chords write it, in macOS's terms (`Option` is `Alt`). */
export type Mod = 'Ctrl' | 'Alt' | 'Shift' | 'Cmd';

/** The order macOS says them in: ⌃⌥⇧⌘. Linux says Ctrl, Alt, Shift. */
const ORDER: Mod[] = ['Ctrl', 'Alt', 'Shift', 'Cmd'];

/** A chord as its modifiers and its key: `Ctrl+Cmd+W` is ⌃⌘ and W. */
export interface Chord {
  mods: Mod[];
  key: string;
}

function asMod(part: string): Mod | null {
  if (part === 'Option') return 'Alt';
  return (ORDER as string[]).includes(part) ? (part as Mod) : null;
}

/** A chord written as `Ctrl+Cmd+W`, its modifiers in macOS's order. A key that's `+` itself ends
 *  the chord (`Cmd++`). */
export function parseChord(chord: string): Chord {
  const parts = chord.endsWith('++') ? [...chord.slice(0, -2).split('+'), '+'] : chord.split('+');
  const key = parts.pop() ?? '';
  const mods = new Set(parts.map(asMod).filter((m): m is Mod => m !== null));
  return { mods: ORDER.filter((m) => mods.has(m)), key };
}

export function writeChord(c: Chord): string {
  return [...ORDER.filter((m) => c.mods.includes(m)), c.key].join('+');
}

/** The chords Linux has otherwise than ⌘ → Ctrl, ⌥ and ⌃ → Alt, ⇧ → Shift would make them: other
 *  modifiers, and for a few another key (`toKey`). */
const LINUX: { mods: Mod[]; key: RegExp; to: Mod[]; toKey?: (key: string) => string }[] = [
  // ⌥⌘ and a digit picks a project, and ⌃⌘ and a digit snoozes, which the rule would make the
  // same: a project is Alt and the digit, as a tab is in most Linux apps.
  { mods: ['Alt', 'Cmd'], key: /^[0-9]$/, to: ['Alt'] },
  // ⌃⇥ and ⌃⇧⇥ go through the views and ⌃` opens a terminal: Alt+Tab is the desktop's, and Ctrl+`
  // is VS Code's terminal key there. No chord is ⌘⇥, ⇧⌘⇥ or ⌘`.
  { mods: ['Ctrl'], key: /^(Tab|`)$/, to: ['Ctrl'] },
  { mods: ['Ctrl', 'Shift'], key: /^Tab$/, to: ['Ctrl', 'Shift'] },
  // ⌃⏎ sends now: Alt+Enter is ⌥⏎'s.
  { mods: ['Ctrl'], key: /^Enter$/, to: ['Ctrl', 'Shift'] },
  // ⌃⌘↑ ⌃⌘↓ move a row among the pins or the todos. Ctrl+Alt+↑ ↓ switch workspaces in GNOME and
  // Xfce, and with Shift move a window to one, so they're Ctrl+Shift+PageUp and PageDown, which
  // move a tab in Linux's browsers and terminals.
  { mods: ['Ctrl', 'Cmd'], key: /^(Up|Down)$/, to: ['Ctrl', 'Shift'], toKey: (key) => (key === 'Up' ? 'PageUp' : 'PageDown') },
];

/** The keys a chord written for macOS is pressed with on this platform, in their own names: on
 *  macOS as it's written; on Linux ⌘ is Ctrl, ⌥ and ⌃ are Alt and ⇧ is Shift, but for `LINUX`. */
export function platformChord(chord: string): string {
  if (platform() === 'mac') return chord;
  const c = parseChord(chord);
  const same = (a: Mod[], b: Mod[]) => a.length === b.length && a.every((m) => b.includes(m));
  const special = LINUX.find((l) => same(l.mods, c.mods) && l.key.test(c.key));
  return writeChord({ mods: special ? special.to : linuxMods(c.mods), key: special?.toKey?.(c.key) ?? c.key });
}

function linuxMods(mods: Mod[]): Mod[] {
  const out = new Set<Mod>();
  for (const m of mods) out.add(m === 'Cmd' ? 'Ctrl' : m === 'Ctrl' ? 'Alt' : m);
  return ORDER.filter((m) => out.has(m));
}

/** What a key event says of its modifiers. */
export type KeyMods = Pick<KeyboardEvent, 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>;

/** Whether exactly these modifiers (written as a chord writes them) are held, as this platform
 *  has them. For a chord with its key, `chordPressed`, which knows `LINUX`'s. */
export function held(e: KeyMods, ...mods: Mod[]): boolean {
  const want = platform() === 'mac' ? mods : linuxMods(mods);
  return e.metaKey === want.includes('Cmd') && e.ctrlKey === want.includes('Ctrl') && e.altKey === want.includes('Alt') && e.shiftKey === want.includes('Shift');
}

/** Whether ⌘ is held, with whatever else: what ⌘-click and ⌘⏎ go by. Ctrl on Linux. */
export function cmd(e: Pick<KeyboardEvent, 'metaKey' | 'ctrlKey'>): boolean {
  return platform() === 'mac' ? e.metaKey : e.ctrlKey;
}

/** Whether ⌃ is held, with whatever else, as this platform has it: Alt on Linux. */
export function ctrl(e: Pick<KeyboardEvent, 'ctrlKey' | 'altKey'>): boolean {
  return platform() === 'mac' ? e.ctrlKey : e.altKey;
}

/** Whether ⌘ or ⌃ is held: a press that's some chord rather than typing or a plain click, which ⌥
 *  alone may still be (it types letters on macOS). On Linux that's Ctrl alone, as ⌃ is Alt there. */
export function chordMod(e: Pick<KeyboardEvent, 'metaKey' | 'ctrlKey'>): boolean {
  return e.metaKey || e.ctrlKey;
}

/** Whether a key is pressed with nothing but ⇧ at most: typing, not a chord. The same everywhere. */
export function typed(e: Pick<KeyboardEvent, 'metaKey' | 'ctrlKey' | 'altKey'>): boolean {
  return !e.metaKey && !e.ctrlKey && !e.altKey;
}

/** Whether a key is pressed with no modifier at all. */
export function bare(e: KeyMods): boolean {
  return typed(e) && !e.shiftKey;
}

/** ⌃ and a letter as macOS's text fields take it (⌃N and ⌃P go down and up a list), whatever
 *  else is held. Linux has no such keys: Ctrl there is ⌘'s. */
export function emacsCtrl(e: Pick<KeyboardEvent, 'key' | 'ctrlKey'>, key: string): boolean {
  return platform() === 'mac' && e.ctrlKey && e.key === key;
}

/** A press macOS takes for a right-click: the primary button with ⌃. Linux has none. */
export function contextPress(e: { button: number; ctrlKey: boolean }): boolean {
  return platform() === 'mac' && e.button === 0 && e.ctrlKey;
}

/** A wheel that zooms: a pinch (which WebKit sends with Ctrl, on both platforms) or ⌘ and the
 *  wheel. */
export function zoomWheel(e: Pick<WheelEvent, 'metaKey' | 'ctrlKey'>): boolean {
  return e.ctrlKey || cmd(e);
}

/** A pinch, which WebKit sends as a wheel with Ctrl. */
export function pinch(e: Pick<WheelEvent, 'ctrlKey'>): boolean {
  return e.ctrlKey;
}

type Key = Pick<KeyboardEvent, 'key' | 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>;

/** Whether a key event is the chord (written for macOS), with exactly its modifiers as this
 *  platform has them. */
export function chordPressed(chord: string, e: Key): boolean {
  const c = parseChord(platformChord(chord));
  const has = (m: Mod) => c.mods.includes(m);
  if (e.metaKey !== has('Cmd') || e.ctrlKey !== has('Ctrl') || e.shiftKey !== has('Shift')) return false;
  const key = c.key;
  // ] and [ go by their keys' codes, which layouts that type them with ⌥ keep, and with ⌥ held or
  // not, as the main window and the quick panel have always taken ].
  if (key === ']') return e.code === 'BracketRight';
  if (key === '[') return e.code === 'BracketLeft';
  if (e.altKey !== has('Alt')) return false;
  // - goes by what it types or where it is, as history.ts has it.
  if (key === '-') return e.key === '-' || e.key === '_' || e.code === 'Minus';
  // ` goes by what it types or where it is too: layouts that make it a dead key type nothing.
  if (key === '`') return e.key === '`' || e.code === 'Backquote';
  if (key === 'Up' || key === 'Down' || key === 'Left' || key === 'Right') return e.key === `Arrow${key}`;
  if (key === 'Enter' || key === 'Tab' || key === 'Backspace' || key === 'Escape' || key === 'PageUp' || key === 'PageDown') return e.key === key;
  if (key === 'Esc') return e.key === 'Escape';
  // A digit goes by its key's code: ⌥ changes what it types (⌥1 is ¡), as ⇧ does on Linux.
  if (/^[0-9]$/.test(key)) return e.code === `Digit${key}`;
  return e.key.toUpperCase() === key;
}

/** Each key's sign as macOS draws it. */
const SIGNS: Record<string, string> = { Ctrl: '⌃', Cmd: '⌘', Alt: '⌥', Shift: '⇧', Up: '↑', Down: '↓', Left: '←', Right: '→', Enter: '⏎', Esc: 'esc', Backspace: '⌫', Tab: '⇥' };
/** Each key's name as Linux says it: modifiers as words, arrows as arrows. */
const NAMES: Record<string, string> = { Up: '↑', Down: '↓', Left: '←', Right: '→', Esc: 'Esc', PageUp: 'PgUp', PageDown: 'PgDn' };

/** A chord (written for macOS) as this platform says it, one key at a time: ⌃ ⌘ W, or Ctrl Alt W. */
export function chordKeys(chord: string): string[] {
  // macOS says them in the order the chord is written, as it always has (⌘⇧↑).
  if (platform() === 'mac') return chord.split('+').map((k) => SIGNS[k] ?? k);
  const c = parseChord(platformChord(chord));
  return [...c.mods, c.key].map((k) => NAMES[k] ?? k);
}

/** A chord as one label: ⌃⌘W, or Ctrl+Alt+W. */
export function chordSigns(chord: string): string {
  return joinKeys(chordKeys(chord));
}

/** Keys said one at a time (`chordKeys`) as one label: ⌃⌘W, or Ctrl+Alt+W. */
export function joinKeys(keys: string[]): string {
  return keys.join(platform() === 'mac' ? '' : '+');
}

/** macOS's signs for keys, back to the names chords write them by. */
const FROM_SIGN: Record<string, string> = { '⌃': 'Ctrl', '⌥': 'Alt', '⇧': 'Shift', '⌘': 'Cmd', '↑': 'Up', '↓': 'Down', '←': 'Left', '→': 'Right', '⏎': 'Enter', '↩': 'Enter', '⌫': 'Backspace', '⇥': 'Tab' };

/** A chord written in macOS's signs (⌃⌘S, ⌘⏎, ⇧⌘↑) as a chord's text (`Ctrl+Cmd+S`), or null for
 *  text that isn't one. */
export function chordOfSigns(signs: string): string | null {
  const m = /^([⌃⌥⇧⌘]*)(.+)$/u.exec(signs);
  if (!m) return null;
  const mods = [...m[1]].map((s) => FROM_SIGN[s]);
  const key = FROM_SIGN[m[2]] ?? (m[2].length === 1 ? m[2].toUpperCase() : m[2]);
  return [...mods, key].join('+');
}

/** Keys written in macOS's signs, for a keycap or a label (⌘K, ⌃⌘↑), as this platform says them. */
export function keysSaid(signs: string): string {
  if (platform() === 'mac' || !/[⌃⌥⇧⌘⏎↩⌫⇥]/u.test(signs)) return signs;
  const chord = chordOfSigns(signs);
  return chord ? chordSigns(chord) : signs;
}

/** A keycap list (`Kbd`), as this platform says it: ['⌘', 'K'] is Ctrl and K on Linux. Keys
 *  already in Linux's words, a global shortcut's, are left as they are. */
export function capsSaid(keys: string[]): string[] {
  if (platform() === 'mac' || !keys.some((k) => /[⌃⌥⇧⌘⏎↩⌫⇥]/u.test(k))) return keys;
  const chord = chordOfSigns(keys.join(''));
  return chord ? chordKeys(chord) : keys;
}

/** Text that names keys in macOS's signs ("Send now (⌃⏎)", "⌘-click adds it"), with each as this
 *  platform says it. On macOS it's the text as written. */
export function keysIn(text: string): string;
export function keysIn(text: string | undefined): string | undefined;
export function keysIn(text: string | undefined): string | undefined {
  if (platform() === 'mac' || text === undefined) return text;
  return (
    text
      // A modifier's click: ⌘-click is Ctrl-click, ⇧-click Shift-click.
      .replace(/([⌃⌥⇧⌘]+)-(click|drag)/gu, (_, mods: string, what: string) => `${linuxWords(mods)}-${what}`)
      // Modifiers and a key: ⌃⌘S, ⇧⌘↑, ⌥⏎, ⌘,, ⌃Tab. A key named in a word goes before a letter,
      // or ⌃Tab would be ⌃T and "ab".
      .replace(/([⌃⌥⇧⌘]+)(Tab|Space|Enter|esc|Esc|[A-Za-z0-9]|[↑↓←→⏎↩⌫⇥`\-=[\],./;'\\])/gu, (all: string) => keysSaid(all))
      // Keys alone, as Linux names them.
      .replace(/[⏎↩]/gu, 'Enter')
      .replace(/⌫/gu, 'Backspace')
      .replace(/⇥/gu, 'Tab')
  );
}

function linuxWords(mods: string): string {
  return linuxMods([...mods].map((s) => FROM_SIGN[s] as Mod)).join('+');
}

/** The words a window says the platform by, where it means the platform: "Follow macOS". */
export function systemName(): string {
  return platform() === 'mac' ? 'macOS' : 'the system';
}
