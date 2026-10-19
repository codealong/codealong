// Searching Settings (ui/SettingsFind.tsx): what the words typed find among a section's settings.
// It reads the settings as the form shows them, so whatever a field says is found without a list
// to keep beside it: a setting is a field, a card (an account, a device, a routing group) or one
// of Jev's features, and it's found by its label, what it says under it, the choices it offers,
// what's typed in it and the name of its section, every word somewhere in those, by the quick
// panel's rule (quick.ts `wordAt`). A setting with settings inside it (a routing group's card among
// the groups) shows whole when its own words match, and otherwise with only the ones inside that
// do. What isn't a setting (a section's intro, a button to add one) shows only in a setting it's
// part of, but for what a section says now (an error, what an action did), which always shows.

import { queryWords, wordAt } from './quick';

/** A word the settings say otherwise: the app's lists call them PRs, Settings pull requests. */
const SAID_AS: Record<string, string> = { pr: 'pull request', prs: 'pull requests' };

/** Text as it's matched: lowercased, with a curly apostrophe as the one a keyboard types. */
function fold(text: string): string {
  return text.toLowerCase().replace(/[‘’]/g, "'");
}

/** The words to look for. */
export function findWords(query: string): string[] {
  return queryWords(fold(query));
}

/** Whether `text` has every word. */
export function says(text: string, words: string[]): boolean {
  const hay = fold(text);
  return words.every((w) => wordAt(hay, w, words.length) >= 0 || (w in SAID_AS && hay.includes(SAID_AS[w])));
}

/** Where `text` has the words, every time, as [start, end) to mark; none when folding it would
 *  move its letters about (a letter that lowercases to two). */
export function spansIn(text: string, words: string[]): [number, number][] {
  const hay = fold(text);
  if (hay.length !== text.length) return [];
  const out: [number, number][] = [];
  for (const w of words) {
    for (let at = wordAt(hay, w, words.length); at >= 0; at = wordAt(hay, w, words.length, at + w.length)) out.push([at, at + w.length]);
    const said = SAID_AS[w];
    if (said) for (let at = hay.indexOf(said); at >= 0; at = hay.indexOf(said, at + said.length)) out.push([at, at + said.length]);
  }
  return out.sort((a, b) => a[0] - b[0]);
}

/** An element of a section as the search reads it: whether it's a setting, or something the
 *  section says now (`alert`), the text directly in it, and the elements in it. */
export interface Piece<T> {
  at: T;
  setting: boolean;
  alert: boolean;
  text: string;
  kids: Piece<T>[];
}

export interface Found<T> {
  /** What to hide: what's outside every setting, the settings that don't match, and in one
   *  found by a setting inside it, the others inside it. */
  hidden: T[];
  /** How many settings show, not counting those inside another. */
  count: number;
  /** Whether something the section says now shows outside its settings: a section with nothing
   *  that matches but an error still shows, for the error. */
  alerting: boolean;
  /** Every setting that shows, to keep showing while the words are the same (`kept`). */
  shown: T[];
}

const allText = <T>(p: Piece<T>): string => [p.text, ...p.kids.map(allText)].join(' ');
/** A setting's own words: its text but for the settings inside it. */
const ownText = <T>(p: Piece<T>): string => [p.text, ...p.kids.filter((k) => !k.setting).map(ownText)].join(' ');
const holdsSetting = <T>(p: Piece<T>): boolean => p.kids.some((k) => k.setting || holdsSetting(k));
/** Whether it holds something the section says now, outside any setting in it. */
const holdsAlert = <T>(p: Piece<T>): boolean => p.kids.some((k) => !k.setting && (k.alert || holdsAlert(k)));

/**
 * What of `root` (a section's pane) shows for `words`. `context` is matched with every setting:
 * the section's name, so the section's name finds all of it. A setting in `kept` showed for these
 * words before, or has the keyboard, and stays, so what you're changing doesn't go from under you
 * when what it says changes with it. A setting that's an alert (a device asking to link) shows
 * whatever the words.
 */
export function foundIn<T>(root: Piece<T>, words: string[], context: string, kept: ReadonlySet<T> = new Set()): Found<T> {
  const out: Found<T> = { hidden: [], count: 0, alerting: holdsAlert(root), shown: [] };
  // What's in `p`, inside a setting or not (`inSetting`), with `ctx` the words around it. Whether
  // a setting in it shows.
  const walk = (p: Piece<T>, ctx: string, inSetting: boolean, into: Found<T>): boolean => {
    let any = false;
    for (const k of p.kids) {
      let found = false;
      let show: boolean;
      if (k.setting) show = found = setting(k, ctx, into);
      else if (holdsSetting(k) || holdsAlert(k)) {
        found = walk(k, ctx, inSetting, into);
        show = found || holdsAlert(k);
      } else show = inSetting || k.alert;
      if (!show) into.hidden.push(k.at);
      if (found) any = true;
      if (found && k.setting && !inSetting) into.count++;
    }
    return any;
  };
  const setting = (s: Piece<T>, ctx: string, into: Found<T>): boolean => {
    if (!s.alert && !kept.has(s.at) && !says(`${ctx} ${allText(s)}`, words)) return false;
    into.shown.push(s.at);
    const own = `${ctx} ${ownText(s)}`;
    if (!holdsSetting(s) || says(own, words)) return true;
    // Found by what's in it: the ones in it that match, unless the words are spread over several,
    // when it shows whole.
    const inner: Found<T> = { hidden: [], count: 0, alerting: false, shown: [] };
    if (walk(s, own, true, inner)) {
      into.hidden.push(...inner.hidden);
      into.shown.push(...inner.shown);
    }
    return true;
  };
  walk(root, context, false, out);
  return out;
}
