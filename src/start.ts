// The new-session sheet's rules: the account Auto starts, where each choice can open, and the
// choice the sheet opens with. Auto only chooses between accounts of one harness: picking Claude
// Code or Codex is picking an agent, not just whose limits to spend.

import type { Harness, StartAccount, StartPlace, Usage } from './types';

/** An account's name, or `auto:<harness>` for Auto. */
export type Choice = string;

const AUTO = 'auto:';
const HARNESSES: Harness[] = ['claude', 'codex'];
const REMEMBER_KEY = 'codealong.start';

export function autoChoice(h: Harness): Choice {
  return `${AUTO}${h}`;
}

export function autoHarness(c: Choice): Harness | null {
  return c.startsWith(AUTO) ? (c.slice(AUTO.length) as Harness) : null;
}

/** How full an account's fullest limit is, 0 to 100, or null when there are no numbers. */
export function fullest(u: Usage | undefined): number | null {
  if (!u || u.needsSignIn || u.meters.length === 0) return null;
  return Math.max(...u.meters.map((m) => m.percent));
}

/** The account Auto starts for each harness with more than one account that can start
 *  sessions: the one whose fullest limit is least full. Accounts without numbers are picked
 *  only when none have them. */
export function autoPicks(accounts: StartAccount[], usage: Usage[]): Partial<Record<Harness, string>> {
  const out: Partial<Record<Harness, string>> = {};
  for (const h of HARNESSES) {
    const mine = accounts.filter((a) => a.harness === h && a.places.length > 0);
    if (mine.length < 2) continue;
    let pick = mine[0].name;
    let least = Infinity;
    for (const a of mine) {
      const full = fullest(usage.find((u) => u.account === a.name));
      if (full !== null && full < least) {
        least = full;
        pick = a.name;
      }
    }
    out[h] = pick;
  }
  return out;
}

/** The choices the sheet lists: Auto for each harness that has it, then every account. */
export function choices(accounts: StartAccount[], picks: Partial<Record<Harness, string>>): Choice[] {
  return [...HARNESSES.filter((h) => picks[h]).map(autoChoice), ...accounts.map((a) => a.name)];
}

/** Where a choice can open. Auto can land on any account of its harness, so it offers only the
 *  places they all have. */
export function placesFor(choice: Choice | null, accounts: StartAccount[]): StartAccount['places'] {
  if (!choice) return [];
  const h = autoHarness(choice);
  if (!h) return accounts.find((a) => a.name === choice)?.places ?? [];
  const mine = accounts.filter((a) => a.harness === h && a.places.length > 0);
  if (mine.length === 0) return [];
  return mine[0].places.filter((p) => mine.every((a) => a.places.some((q) => q.place === p.place)));
}

/** The account a choice starts right now. */
export function accountFor(choice: Choice | null, picks: Partial<Record<Harness, string>>): string | null {
  if (!choice) return null;
  const h = autoHarness(choice);
  return h ? (picks[h] ?? null) : choice;
}

/** The choice the sheet opens with: the last one you started with while it's still offered,
 *  else the first on the list that can start a session. */
export function initialChoice(accounts: StartAccount[], picks: Partial<Record<Harness, string>>, last: Choice | undefined): Choice | null {
  const offered = choices(accounts, picks).filter((c) => placesFor(c, accounts).length > 0);
  return last && offered.includes(last) ? last : (offered[0] ?? null);
}

/** Where a choice opens at first: where it last opened, while that's still offered. */
export function initialPlace(choice: Choice | null, accounts: StartAccount[], last: StartPlace | undefined): StartPlace | null {
  const places = placesFor(choice, accounts);
  return places.find((p) => p.place === last)?.place ?? places[0]?.place ?? null;
}

interface Remembered {
  choice?: Choice;
  /** The last place each choice opened in. */
  places?: Record<Choice, StartPlace>;
}

export function remembered(): Remembered {
  try {
    return JSON.parse(window.localStorage.getItem(REMEMBER_KEY) ?? '{}') as Remembered;
  } catch {
    return {};
  }
}

/** Keeps the choice you started with, and where, for next time. */
export function remember(choice: Choice, place: StartPlace): void {
  const r = remembered();
  try {
    window.localStorage.setItem(REMEMBER_KEY, JSON.stringify({ choice, places: { ...r.places, [choice]: place } }));
  } catch {
    // Storage can be unavailable; the sheet just won't remember.
  }
}
