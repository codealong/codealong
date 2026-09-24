// The new-session sheet's rules: the account Auto starts, where each choice can open, and the
// choice the sheet opens with. Auto only chooses between accounts of one harness: picking Claude
// Code or Codex is picking an agent, not just whose limits to spend.

import type { Harness, StartAccount, StartPlace, Usage } from './types';

/** An account's name, or `auto:<harness>` for Auto. */
export type Choice = string;

const AUTO = 'auto:';
const HARNESSES: Harness[] = ['claude', 'codex'];
const REMEMBER_KEY = 'codealong.start';
const HOUR = 60 * 60 * 1000;
/** A weekly limit resetting this soon has room that will be lost unless it's used. */
export const RESET_SOON_MS = 48 * HOUR;
/** An account this close to any limit could stall a new session. */
const NEAR_LIMIT = 90;

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

/** When an account's weekly limit resets, if it's been used this week. */
export function weeklyReset(u: Usage | undefined): number | null {
  return u?.meters.find((m) => m.key === 'week')?.resetsAt ?? null;
}

function nearLimit(u: Usage): boolean {
  return u.meters.some((m) => m.severity === 'critical' || m.percent >= NEAR_LIMIT);
}

/** Auto's account for a harness, and why: its weekly limit resets soon (`reset`), it has the
 *  most room (`room`), or there are no numbers to go by (`first`). */
export interface AutoPick {
  account: string;
  reason: 'reset' | 'room' | 'first';
  /** The weekly reset, for `reset`. */
  resetsAt: number | null;
}

export type Picks = Partial<Record<Harness, AutoPick>>;

/**
 * The account Auto starts for each harness with more than one account that can start sessions.
 * Room that's about to reset is spent first: among accounts not near a limit, the one whose
 * weekly limit resets soonest within 48 hours. Without one, the account whose fullest limit is
 * least full. Accounts at 90% of any limit are passed over while another has room, and accounts
 * without numbers are picked only when none have them.
 */
export function autoPicks(accounts: StartAccount[], usage: Usage[], now: number): Picks {
  const out: Picks = {};
  for (const h of HARNESSES) {
    const mine = accounts.filter((a) => a.harness === h && a.places.length > 0);
    if (mine.length < 2) continue;
    const measured = mine.flatMap((a) => {
      const u = usage.find((x) => x.account === a.name);
      const full = fullest(u);
      return u && full !== null ? [{ name: a.name, u, full }] : [];
    });
    if (measured.length === 0) {
      out[h] = { account: mine[0].name, reason: 'first', resetsAt: null };
      continue;
    }
    const roomy = measured.filter((m) => !nearLimit(m.u));
    const pool = roomy.length > 0 ? roomy : measured;
    const soon = pool
      .map((m) => ({ ...m, at: weeklyReset(m.u) }))
      .filter((m): m is typeof m & { at: number } => m.at !== null && m.at > now && m.at - now <= RESET_SOON_MS)
      .sort((a, b) => a.at - b.at || a.full - b.full);
    if (soon.length > 0) {
      out[h] = { account: soon[0].name, reason: 'reset', resetsAt: soon[0].at };
      continue;
    }
    const least = pool.reduce((best, m) => (m.full < best.full ? m : best));
    out[h] = { account: least.name, reason: 'room', resetsAt: null };
  }
  return out;
}

/** The choices the sheet lists: Auto for each harness that has it, then every account. */
export function choices(accounts: StartAccount[], picks: Picks): Choice[] {
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
export function accountFor(choice: Choice | null, picks: Picks): string | null {
  if (!choice) return null;
  const h = autoHarness(choice);
  return h ? (picks[h]?.account ?? null) : choice;
}

/** The choice the sheet opens with: the last one you started with while it's still offered,
 *  else the first on the list that can start a session. */
export function initialChoice(accounts: StartAccount[], picks: Picks, last: Choice | undefined): Choice | null {
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
