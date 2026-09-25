// The rules for starting a session from the quick panel: the account and model Auto starts,
// what the agent menu offers (every account on every model it can start on), and where each
// choice can open. Auto only chooses between accounts of one harness: picking Claude Code, Codex
// or OpenCode is picking an agent, not just whose limits to spend.

import { HARNESSES } from './harness';
import type { Harness, Meter, StartAccount, StartPlace, Usage } from './types';

/** An account's name, or `auto:<harness>` for Auto. */
export type Choice = string;

const AUTO = 'auto:';
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

/** Families that have their own weekly limits, as the usage report names them. */
const FAMILIES = ['fable', 'opus', 'sonnet', 'haiku'];

/**
 * The limits a session on `model` spends: those for every model, and the one for its family.
 * A model that names no family (none given and no account default, or `best`) could be any of
 * them, so every limit counts. `opusplan` names Opus, and its Sonnet half has no limit of its own.
 */
export function metersFor(u: Usage, model: string | null): Meter[] {
  const m = model?.toLowerCase() ?? '';
  if (!FAMILIES.some((f) => m.includes(f))) return u.meters;
  return u.meters.filter((x) => x.model === null || m.includes(x.model.toLowerCase()));
}

/** How full an account's fullest limit for a model is, 0 to 100, or null when there are no numbers. */
export function fullest(u: Usage | undefined, model: string | null = null): number | null {
  if (!u || u.needsSignIn || u.meters.length === 0) return null;
  return Math.max(...metersFor(u, model).map((m) => m.percent));
}

/** When an account's weekly room for a model resets, if it's been used this week: the soonest
 *  of its weekly limits, all models' or the model's own. */
export function weeklyReset(u: Usage | undefined, model: string | null = null): number | null {
  if (!u) return null;
  const at = metersFor(u, model)
    .filter((m) => (m.key === 'week' || m.key.startsWith('model:')) && m.resetsAt !== null)
    .map((m) => m.resetsAt as number);
  return at.length > 0 ? Math.min(...at) : null;
}

function near(m: Meter): boolean {
  return m.severity === 'critical' || m.percent >= NEAR_LIMIT;
}

function nearLimit(u: Usage, model: string | null): boolean {
  return metersFor(u, model).some(near);
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
 * The account Auto starts for each harness with more than one account that can start sessions,
 * for a session on `model` (null for each account's default). Only the limits that model spends
 * count, so an account out of Fable can still start Opus.
 * Room that's about to reset is spent first: among accounts not near a limit, the one whose
 * weekly limit resets soonest within 48 hours. Without one, the account whose fullest limit is
 * least full. Accounts at 90% of any limit are passed over while another has room, and accounts
 * without numbers are picked only when none have them.
 */
export function autoPicks(accounts: StartAccount[], usage: Usage[], now: number, model: string | null = null): Picks {
  const out: Picks = {};
  for (const h of HARNESSES) {
    const mine = accounts.filter((a) => a.harness === h && a.places.length > 0);
    if (mine.length < 2) continue;
    const measured = mine.flatMap((a) => {
      const u = usage.find((x) => x.account === a.name);
      const on = model ?? a.defaultModel;
      const full = fullest(u, on);
      return u && full !== null ? [{ name: a.name, u, on, full }] : [];
    });
    if (measured.length === 0) {
      out[h] = { account: mine[0].name, reason: 'first', resetsAt: null };
      continue;
    }
    const roomy = measured.filter((m) => !nearLimit(m.u, m.on));
    const pool = roomy.length > 0 ? roomy : measured;
    const soon = pool
      .map((m) => ({ ...m, at: weeklyReset(m.u, m.on) }))
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

/** The choice the panel opens with: `wanted` (what a failed start had) while it's still
 *  offered, else the first on the list that can start a session, which is Auto where there's one. */
export function initialChoice(accounts: StartAccount[], picks: Picks, wanted: Choice | undefined): Choice | null {
  const offered = choices(accounts, picks).filter((c) => placesFor(c, accounts).length > 0);
  return wanted && offered.includes(wanted) ? wanted : (offered[0] ?? null);
}

/** A model as `--model` takes it, and what menus call it (start.rs's CLAUDE_MODELS). */
export interface ModelOption {
  id: string;
  label: string;
}

/** One line of the agent menu: Auto for a harness, or an account on a model. */
export interface AgentOption {
  /** `auto:<harness>`, or `<account>|<model>` with no model for the account's own default. */
  id: string;
  choice: Choice;
  /** Null for the account's default model (or the one its app picks). */
  model: string | null;
}

export function agentId(choice: Choice, model: string | null): string {
  return autoHarness(choice) ? choice : `${choice}|${model ?? ''}`;
}

/** The choice and model an agent menu id stands for. Account names and models have no `|`. */
export function parseAgent(id: string): { choice: Choice; model: string | null } {
  if (autoHarness(id)) return { choice: id, model: null };
  const at = id.indexOf('|');
  return at < 0 ? { choice: id, model: null } : { choice: id.slice(0, at), model: id.slice(at + 1) || null };
}

/** Whether Codealong can ask an account for a model: only Claude Code takes one, and only on the
 *  command line. The apps and OpenCode pick their own. */
export function takesModel(a: StartAccount | undefined): boolean {
  return !!a && a.harness === 'claude' && a.places.some((p) => p.place === 'terminal');
}

/** What the agent menu lists: Auto for each harness that has it, then every account, on its own
 *  default and, where Codealong can ask for one, on each model. */
export function agentOptions(accounts: StartAccount[], models: ModelOption[], picks: Picks): AgentOption[] {
  const out: AgentOption[] = HARNESSES.filter((h) => picks[h]).map((h) => ({ id: autoChoice(h), choice: autoChoice(h), model: null }));
  for (const a of accounts) {
    out.push({ id: agentId(a.name, null), choice: a.name, model: null });
    if (takesModel(a)) for (const m of models) out.push({ id: agentId(a.name, m.id), choice: a.name, model: m.id });
  }
  return out;
}

/** The family a model or alias belongs to, which per-model limits are named after. */
export function modelFamily(model: string | null): string | null {
  if (!model) return null;
  const m = model.toLowerCase();
  return FAMILIES.find((f) => m.includes(f)) ?? null;
}

/**
 * A model the way you'd say it: an alias by its label, shortened (`opus[1m]` is Opus 1M), a
 * full name by family and version (`claude-fable-5-1[1m]` is Fable 5.1 1M), anything else as
 * written.
 */
export function modelName(model: string, models: ModelOption[]): string {
  const known = models.find((m) => m.id === model);
  if (known) return known.label.split(':')[0].replace(/ \(1M context\)$/, ' 1M');
  const long = /\[1m\]$/i.test(model);
  const base = model.replace(/\[1m\]$/i, '');
  const m = /^claude-([a-z]+)-(\d+)(?:-(\d{1,2}))?(?:-\d{8})?$/i.exec(base);
  const name = m ? `${m[1][0].toUpperCase()}${m[1].slice(1)} ${m[2]}${m[3] ? `.${m[3]}` : ''}` : base;
  return long ? `${name} 1M` : name;
}

/** The limit a model counts against beyond the account's own, like Fable's week. */
export function modelMeter(u: Usage | undefined, model: string | null): Meter | null {
  const family = modelFamily(model);
  if (!u || !family) return null;
  return u.meters.find((m) => m.model?.toLowerCase() === family) ?? null;
}

/** What Auto falls back to, best first, when an account's own model is nearly used up. */
const FALLBACK = ['opus', 'sonnet'];

/**
 * The model Auto starts an account on: its default (settings.json's `model`), unless that model's
 * own limit is nearly used up, as Fable's week can be while the account has room; then the best
 * other model with room, with the default's context size. `model` is null for the default, and
 * `spent` is the limit that ruled the default out.
 */
export function autoModel(defaultModel: string | null, u: Usage | undefined): { model: string | null; spent: Meter | null } {
  const own = modelMeter(u, defaultModel);
  if (!own || !near(own)) return { model: null, spent: null };
  const long = /\[1m\]$/i.test(defaultModel ?? '');
  const family = modelFamily(defaultModel);
  for (const f of FALLBACK) {
    if (f === family) continue;
    const m = modelMeter(u, f);
    if (!m || !near(m)) return { model: long ? `${f}[1m]` : f, spent: own };
  }
  return { model: null, spent: own };
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
