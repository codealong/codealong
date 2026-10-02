// The rules for starting a session from the quick panel: the account and model Auto starts,
// what the agent menu offers (every account on every model it can start on), and where each
// choice can open. Auto chooses among every account that can start a session, preferred ones
// first (config.toml's `preferred`), so it keeps to the agents you'd rather use while they have
// room. start.rs's `auto_pick` has the same rule for `codealong start`.

import type { Meter, StartAccount, StartOptions, StartPlace, Usage } from './types';

/** An account's name, or `auto` for Auto. */
export type Choice = string;

export const AUTO: Choice = 'auto';
const REMEMBER_KEY = 'codealong.start';
/** An account this close to any limit could stall a new session. */
const NEAR_LIMIT = 90;

export function isAuto(c: Choice | null): boolean {
  return c === AUTO;
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

function near(m: Meter): boolean {
  return m.severity === 'critical' || m.percent >= NEAR_LIMIT;
}

/** Whether a model's own limit (Fable's week, say) still has room on an account. */
export function modelHasRoom(u: Usage | undefined, model: string): boolean {
  const own = modelMeter(u, model);
  return !own || !near(own);
}

/** Auto's account, and why: its weekly limit resets soonest, so its room is the first to be
 *  lost (`reset`); it has the most room (`room`); or there are no numbers to go by (`first`). */
export interface AutoPick {
  account: string;
  reason: 'reset' | 'room' | 'first';
  /** The weekly reset, for `reset`. */
  resetsAt: number | null;
}

/** Whether an account's command line would take `model`: one it lists, and for Claude Code any,
 *  since `--model` takes a model's full name too. The adapters' `offers_model` has the same rule,
 *  which `codealong start --model` goes by. */
function offersModel(a: StartAccount, model: string): boolean {
  return a.harness === 'claude' || (a.models ?? []).some((m) => m.id === model);
}

function startable(accounts: StartAccount[]): StartAccount[] {
  return accounts.filter((a) => a.places.length > 0);
}

/** The accounts Auto prefers among those that can start sessions: the ones marked preferred,
 *  else those of the first account's harness, so that without a star Auto keeps to one agent
 *  until its accounts are out of room. start.rs's `preferred_names` has the same rule. */
export function preferredNames(accounts: StartAccount[]): string[] {
  const can = startable(accounts);
  if (can.some((a) => a.preferred)) return can.filter((a) => a.preferred).map((a) => a.name);
  return can.filter((a) => a.harness === can[0]?.harness).map((a) => a.name);
}

/**
 * The account Auto starts, for a session on `model` (null for each account's default; else only
 * the accounts whose command line takes it, `offersModel`). Only the limits each account's model spends count, so an account out
 * of Fable can still start Opus, and an account at 90% of one of them has no room. Preferred
 * accounts with room go first, then a preferred one without numbers yet, then the others the
 * same way, and only when all are near a limit the least full of them. Among accounts with
 * room, the one whose weekly limit resets soonest wins, so no week's room goes unused; without
 * a reset under way, the one whose fullest limit is least full. Signed-out accounts are passed
 * over. Null when no account can start a session.
 */
export function autoPick(accounts: StartAccount[], usage: Usage[], now: number, model: string | null = null): AutoPick | null {
  const preferred = preferredNames(accounts);
  const candidates = startable(accounts)
    .filter((a) => model === null || offersModel(a, model))
    .map((a) => ({ name: a.name, preferred: preferred.includes(a.name), on: model ?? a.defaultModel }));
  const measured: { name: string; preferred: boolean; full: number; near: boolean; reset: number | null }[] = [];
  const unknown: typeof candidates = [];
  for (const c of candidates) {
    const u = usage.find((x) => x.account === c.name);
    if (u?.needsSignIn) continue;
    const meters = u ? metersFor(u, c.on) : [];
    if (meters.length === 0) {
      unknown.push(c);
      continue;
    }
    const resets = meters.filter((m) => (m.key === 'week' || m.key.startsWith('model:')) && m.resetsAt !== null && m.resetsAt > now).map((m) => m.resetsAt as number);
    measured.push({ name: c.name, preferred: c.preferred, full: Math.max(...meters.map((m) => m.percent)), near: meters.some(near), reset: resets.length > 0 ? Math.min(...resets) : null });
  }
  const leastFull = (pool: typeof measured): AutoPick => ({ account: pool.reduce((best, m) => (m.full < best.full ? m : best)).name, reason: 'room', resetsAt: null });
  const best = (pool: typeof measured): AutoPick => {
    const soon = pool.filter((m) => m.reset !== null).sort((a, b) => (a.reset as number) - (b.reset as number) || a.full - b.full);
    return soon.length > 0 ? { account: soon[0].name, reason: 'reset', resetsAt: soon[0].reset } : leastFull(pool);
  };
  for (const pref of [true, false]) {
    const roomy = measured.filter((m) => m.preferred === pref && !m.near);
    if (roomy.length > 0) return best(roomy);
    const waiting = unknown.find((c) => c.preferred === pref);
    if (waiting) return { account: waiting.name, reason: 'first', resetsAt: null };
  }
  if (measured.length > 0) return leastFull(measured);
  const first = candidates.find((c) => c.preferred) ?? candidates[0];
  return first ? { account: first.name, reason: 'first', resetsAt: null } : null;
}

/** The choices the sheet lists: Auto when there's more than one account to choose between,
 *  then every account. */
export function choices(accounts: StartAccount[], pick: AutoPick | null): Choice[] {
  return [...(offersAuto(accounts, pick) ? [AUTO] : []), ...accounts.map((a) => a.name)];
}

function offersAuto(accounts: StartAccount[], pick: AutoPick | null): boolean {
  return !!pick && startable(accounts).length > 1;
}

/** Where a choice can open. Auto opens where the account it picks can. */
export function placesFor(choice: Choice | null, accounts: StartAccount[], pick: AutoPick | null): StartAccount['places'] {
  const name = accountFor(choice, pick);
  return accounts.find((a) => a.name === name)?.places ?? [];
}

/** The account a choice starts right now. */
export function accountFor(choice: Choice | null, pick: AutoPick | null): string | null {
  if (!choice) return null;
  return isAuto(choice) ? (pick?.account ?? null) : choice;
}

/** The choice the panel opens with: `wanted` (what a failed start had) while it's still
 *  offered, else the first on the list that can start a session, which is Auto where there's one. */
export function initialChoice(accounts: StartAccount[], pick: AutoPick | null, wanted: Choice | undefined): Choice | null {
  const offered = choices(accounts, pick).filter((c) => placesFor(c, accounts, pick).length > 0);
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
  return isAuto(choice) ? choice : `${choice}|${model ?? ''}`;
}

/** The choice and model an agent menu id stands for. Account names and models have no `|`. */
export function parseAgent(id: string): { choice: Choice; model: string | null } {
  if (isAuto(id)) return { choice: id, model: null };
  const at = id.indexOf('|');
  return at < 0 ? { choice: id, model: null } : { choice: id.slice(0, at), model: id.slice(at + 1) || null };
}

/**
 * The accounts with what each one's command line takes: its own lists where its Mac sends them,
 * and for a Mac whose Codealong lists only Claude Code's, those for its Claude Code accounts
 * and nothing for the rest.
 */
export function withLists(o: StartOptions): StartAccount[] {
  return o.accounts.map((a) => {
    const claude = a.harness === 'claude';
    return { ...a, models: a.models ?? (claude ? o.claudeModels : []), efforts: a.efforts ?? (claude ? (o.claudeEfforts ?? []) : []), worktree: a.worktree ?? claude };
  });
}

/** Whether Codealong can ask an account for a model: its harness lists some (`models`; an
 *  account without the list is Claude Code's if it takes any), and only on the command line.
 *  The apps pick their own. */
export function takesModel(a: StartAccount | undefined): boolean {
  return !!a && (a.models ? a.models.length > 0 : a.harness === 'claude') && a.places.some((p) => p.place === 'terminal');
}

/** What the agent menu lists: Auto when it's offered, then every account, on its own
 *  default and, where Codealong can ask for one, on each of its models (`models` for an
 *  account that came without its own list). */
export function agentOptions(accounts: StartAccount[], models: ModelOption[], pick: AutoPick | null): AgentOption[] {
  const out: AgentOption[] = offersAuto(accounts, pick) ? [{ id: AUTO, choice: AUTO, model: null }] : [];
  for (const a of accounts) {
    out.push({ id: agentId(a.name, null), choice: a.name, model: null });
    if (takesModel(a)) for (const m of a.models ?? models) out.push({ id: agentId(a.name, m.id), choice: a.name, model: m.id });
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

/** The place an account's `start` setting names; for Auto, the one the account it picks names. */
export function settledPlace(choice: Choice | null, accounts: StartAccount[], pick: AutoPick | null): StartPlace | null {
  const name = accountFor(choice, pick);
  return accounts.find((a) => a.name === name)?.chosen ?? null;
}

/** Where a choice opens at first: where its account's settings say, else where it last opened,
 *  while that's still offered. */
export function initialPlace(choice: Choice | null, accounts: StartAccount[], pick: AutoPick | null, last: StartPlace | undefined): StartPlace | null {
  const places = placesFor(choice, accounts, pick);
  const want = settledPlace(choice, accounts, pick) ?? last;
  return places.find((p) => p.place === want)?.place ?? places[0]?.place ?? null;
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
