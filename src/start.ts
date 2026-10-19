// The rules for starting a session from the quick panel: the account and model Auto starts,
// what the agent menu offers (every account on every model it can start on), and where each
// choice can open. Auto chooses among every account that can start a session, those of the
// first account's harness first (or from `[routing] auto` when set), so it keeps to one agent
// while its accounts have room. start.rs's `auto_pick` has the same rule for `codealong start`.

import { resetsPhrase } from './format';
import { HARNESS_WORD } from './harness';
import { failedNotice } from './notices';
import { readStored, writeStored, type Codec } from './stored';
import type { Harness, Lane, Meter, Notice, ResumeIn, RoutingGroup, StartAccount, StartOptions, StartPlace, Usage } from './types';

/** The notice while where each account can start a session can't be read (the main window's
 *  Review with and the quick panel ask), which ends once it's read. */
export const START_OPTIONS_NOTICE = 'start-options';
export function startOptionsFailed(e: unknown): Notice {
  return failedNotice(START_OPTIONS_NOTICE, 'Can’t read where sessions start', 'Couldn’t read where each account can start a session, so the last answer read stands', e);
}

/** The notice while where a start opened can't be saved as its accounts' `start`
 *  (`keepPlaceFor`), which ends once one is saved. */
export const KEEP_PLACE_NOTICE = 'keep-start-place';
export function keepPlaceFailed(e: unknown): Notice {
  return failedNotice(KEEP_PLACE_NOTICE, 'Can’t save where sessions start', 'Couldn’t save where the quick panel started a session as its account’s start in config.toml', e);
}

/** An account's name, a routing group's name, or `auto` for Auto. */
export type Choice = string;

export const AUTO: Choice = 'auto';
const REMEMBER_KEY = 'codealong.start';
/** An account this close to any limit could stall a new session. */
const NEAR_LIMIT = 90;
/** The room below an account's cap that Auto leaves on the account whose week resets soonest,
 *  and a fallback group on each member in turn, rather than spend it first, for the sessions
 *  already running there; `[routing] buffer` sets another (start.rs `BUFFER`). */
export const BUFFER = 5;

export function isAuto(c: Choice | null): boolean {
  return c === AUTO;
}

export function isGroup(c: Choice | null, groups: RoutingGroup[] = []): RoutingGroup | undefined {
  return c ? groups.find((g) => g.name === c) : undefined;
}

export function capOf(usageCap?: number | null): number {
  return usageCap != null && usageCap >= 1 && usageCap <= 100 ? usageCap : NEAR_LIMIT;
}

/** Where an account's buffer starts: below it, a start there leaves the sessions running on it
 *  their room (start.rs `buffer_line`). */
function bufferLine(usageCap?: number | null, buffer?: number | null): number {
  return Math.max(0, capOf(usageCap) - (buffer ?? BUFFER));
}

/** Families that have their own weekly limits on Claude plans, as the usage report names them. */
const FAMILIES = ['fable', 'opus', 'sonnet', 'haiku'];

/** The families a limit is for: one, or several joined by ` & ` as Antigravity's `Claude & GPT`
 *  is, lowercased. */
function familiesOf(limitModel: string): string[] {
  return limitModel
    .split(' & ')
    .map((f) => f.trim().toLowerCase())
    .filter(Boolean);
}

/**
 * The limits a session on `model` spends: those for every model, and those for a family its name
 * has. A model whose name has none of Claude's families nor any the account's limits are for
 * (none given and no account default, or `best`) could be any of them, so every limit counts.
 * `opusplan` names Opus, and its Sonnet half has no limit of its own. start.rs's `spends` has the
 * same rule.
 */
export function metersFor(u: Usage, model: string | null): Meter[] {
  const m = model?.toLowerCase() ?? '';
  const named = (limitModel: string) => familiesOf(limitModel).some((f) => m.includes(f));
  if (!FAMILIES.some((f) => m.includes(f)) && !u.meters.some((x) => x.model !== null && named(x.model))) return u.meters;
  return u.meters.filter((x) => x.model === null || named(x.model));
}

function near(m: Meter, cap?: number | null): boolean {
  if (cap != null && cap >= 1 && cap <= 100) return m.percent >= cap;
  return m.severity === 'critical' || m.percent >= NEAR_LIMIT;
}

/** Whether a model's own limit (Fable's week, say) still has room on an account. */
export function modelHasRoom(u: Usage | undefined, model: string, cap?: number | null): boolean {
  const own = modelMeter(u, model);
  return !own || !near(own, cap);
}

/** Auto's account, and why: its weekly limit resets soonest, so its room is the first to be
 *  lost (`reset`); it has the most room (`room`); or there are no numbers to go by (`first`). */
export interface AutoPick {
  account: string;
  reason: 'reset' | 'room' | 'first';
  /** The weekly reset, for `reset`. */
  resetsAt: number | null;
  /** The explicit model a routing group member set, when Auto or a group picked one. */
  model?: string | null;
}

/** Whether an account's command line would take `model`: one it lists, and for Claude Code any,
 *  since `--model` takes a model's full name too. The adapters' `offers_model` has the same rule
 *  (Codex's also takes the models its picker hides), which `codealong start --model` goes by. */
function offersModel(a: StartAccount, model: string): boolean {
  return a.harness === 'claude' || (a.models ?? []).some((m) => m.id === model);
}

function startable(accounts: StartAccount[]): StartAccount[] {
  return accounts.filter((a) => a.places.length > 0);
}

const REFUSED_FOR = 31 * 24 * 3_600_000;

/** Lanes for the Mac a start targets: this Mac's when mixed with another's, or all of them when
 *  already scoped to a remote Mac (`Quick.tsx`). */
function targetLanes(lanes: Lane[]): Lane[] {
  return lanes.some((l) => !l.device) ? lanes.filter((l) => !l.device) : lanes;
}

function modelMatches(sessionModel: string, memberModel: string): boolean {
  const sm = sessionModel.toLowerCase();
  const mm = memberModel.toLowerCase();
  return (
    sm === mm ||
    sm.includes(mm) ||
    mm.includes(sm) ||
    (modelFamily(sessionModel) !== null && modelFamily(sessionModel) === modelFamily(memberModel) && sm.endsWith('[1m]') === mm.endsWith('[1m]'))
  );
}

/** The accounts a usage limit has stopped on the target Mac's sessions (start.rs `refused_accounts`). */
export function refusedAccounts(lanes: Lane[], now: number): Set<string> {
  const sessions = targetLanes(lanes).flatMap((l) => l.sessions);
  const refused = new Map<string, number>();
  for (const s of sessions) {
    const l = s.limit;
    if (!l || (l.resetsAt != null && l.resetsAt <= now) || now - l.at >= REFUSED_FOR) continue;
    refused.set(s.account, Math.max(refused.get(s.account) ?? l.at, l.at));
  }
  const out = new Set<string>();
  for (const [acct, at] of refused) {
    const through = sessions.some((s) => s.account === acct && !s.limit && s.status !== 'working' && s.lastActivity > at);
    if (!through) out.add(acct);
  }
  return out;
}

/** The accounts Auto prefers among those that can start sessions: those of the first account's
 *  harness, so Auto keeps to one agent until its accounts are out of room. start.rs's
 *  `preferred_names` has the same rule. */
export function preferredNames(accounts: StartAccount[]): string[] {
  const can = startable(accounts);
  return can.filter((a) => a.harness === can[0]?.harness).map((a) => a.name);
}

interface UsageCandidate {
  name: string;
  preferred: boolean;
  on: string | null;
  usageCap?: number | null;
  buffer?: number | null;
}

function pickByUsage(candidates: UsageCandidate[], usage: Usage[], now: number): { index: number; pick: AutoPick } | null {
  const measured: { index: number; name: string; preferred: boolean; full: number; near: boolean; reset: number | null; line: number }[] = [];
  const unknown: { index: number; c: UsageCandidate }[] = [];
  for (let i = 0; i < candidates.length; i++) {
    const c = candidates[i];
    const u = usage.find((x) => x.account === c.name);
    if (u?.needsSignIn) continue;
    const meters = u ? metersFor(u, c.on) : [];
    if (meters.length === 0) {
      unknown.push({ index: i, c });
      continue;
    }
    const resets = meters.filter((m) => (m.key === 'week' || m.key.startsWith('model:')) && m.resetsAt !== null && m.resetsAt > now).map((m) => m.resetsAt as number);
    measured.push({
      index: i,
      name: c.name,
      preferred: c.preferred,
      full: Math.max(...meters.map((m) => m.percent)),
      near: meters.some((m) => near(m, c.usageCap)),
      reset: resets.length > 0 ? Math.min(...resets) : null,
      line: bufferLine(c.usageCap, c.buffer),
    });
  }
  const leastFull = (pool: typeof measured) => {
    const m = pool.reduce((best, x) => (x.full < best.full ? x : best));
    return { index: m.index, pick: { account: m.name, reason: 'room' as const, resetsAt: null } };
  };
  const best = (pool: typeof measured) => {
    const soon = pool.filter((m) => m.reset !== null && m.full < m.line).sort((a, b) => (a.reset as number) - (b.reset as number) || a.full - b.full);
    return soon.length > 0 ? { index: soon[0].index, pick: { account: soon[0].name, reason: 'reset' as const, resetsAt: soon[0].reset } } : leastFull(pool);
  };
  for (const pref of [true, false]) {
    const roomy = measured.filter((m) => m.preferred === pref && !m.near);
    if (roomy.length > 0) return best(roomy);
    const waiting = unknown.find((x) => x.c.preferred === pref);
    if (waiting) return { index: waiting.index, pick: { account: waiting.c.name, reason: 'first', resetsAt: null } };
  }
  if (measured.length > 0) return leastFull(measured);
  const firstIdx = candidates.findIndex((c) => c.preferred);
  const idx = firstIdx >= 0 ? firstIdx : 0;
  return candidates[idx] ? { index: idx, pick: { account: candidates[idx].name, reason: 'first', resetsAt: null } } : null;
}

/**
 * The account Auto starts, for a session on `model` (null for each account's default; else only
 * the accounts whose command line takes it, `offersModel`). Only the limits each account's model spends count, so an account out
 * of Fable can still start Opus, and an account at its `usageCap` (90% by default) of one of them has no room. Preferred
 * accounts with room go first, then a preferred one without numbers yet, then the others the
 * same way, and only when all are near a limit the least full of them. Among accounts with
 * room, the one whose weekly limit resets soonest wins, so no week's room goes unused, while its
 * fullest limit is under `usageCap` less its `buffer` (`BUFFER`); without such a reset, the one
 * whose fullest limit is least full. Signed-out accounts are passed over. Null when no account can start a
 * session.
 */
export function autoPick(accounts: StartAccount[], usage: Usage[], now: number, model: string | null = null, autoGroup?: RoutingGroup | null, lanes: Lane[] = []): AutoPick | null {
  if (autoGroup) return pickFromGroup(autoGroup, accounts, usage, now, model, null, lanes);
  const preferred = preferredNames(accounts);
  const refused = refusedAccounts(lanes, now);
  const matching = startable(accounts).filter((a) => model === null || offersModel(a, model));
  const unrefused = matching.filter((a) => !refused.has(a.name));
  const pool = unrefused.length > 0 ? unrefused : matching;
  const candidates = pool.map((a) => ({ name: a.name, preferred: preferred.includes(a.name), on: model ?? a.defaultModel, usageCap: a.usageCap, buffer: a.buffer }));
  return pickByUsage(candidates, usage, now)?.pick ?? null;
}

/** The account and model a routing group picks now (start.rs `pick_from_group`). */
export function pickFromGroup(
  group: RoutingGroup,
  accounts: StartAccount[],
  usage: Usage[],
  now: number,
  model: string | null = null,
  avoid: Harness | null = null,
  lanes: Lane[] = [],
): AutoPick | null {
  const can = startable(accounts);
  const refused = refusedAccounts(lanes, now);
  const preferred = preferredNames(can);
  const explicitMembers = group.members.length > 0;
  // A fallback group's order is its preference, so the first harness gets no more.
  const equal = group.strategy === 'fallback';
  const rawMembers = explicitMembers
    ? group.members.map((m) => ({ account: m.account, model: m.model?.trim() || null, preferred: true }))
    : can.map((a) => ({ account: a.name, model: null, preferred: equal || preferred.includes(a.name) }));
  const askedModel = model?.trim() || null;
  const members = rawMembers.flatMap((m) => {
    const a = can.find((x) => x.name === m.account);
    if (!a) return [];
    if (askedModel && !offersModel(a, askedModel)) return [];
    const explicit = askedModel ?? m.model;
    if (explicit && !offersModel(a, explicit)) return [];
    const checkModel = explicit ?? a.defaultModel;
    const u = usage.find((x) => x.account === a.name);
    const usable = !u?.needsSignIn && !refused.has(a.name);
    const spent = u ? metersFor(u, checkModel) : [];
    const roomy = usable && spent.every((x) => !near(x, a.usageCap));
    // Room short of its buffer, or no numbers yet.
    const clear = roomy && spent.every((x) => x.percent < bufferLine(a.usageCap, a.buffer));
    return [{ account: a, preferred: m.preferred, explicitModel: explicit, checkModel, roomy, clear, usable }];
  });
  if (members.length === 0) return null;
  const roomy = members.filter((m) => m.roomy);
  let pool = roomy.length > 0 ? roomy : members.filter((m) => m.usable);
  if (pool.length === 0) pool = members;
  if (avoid && pool.some((m) => m.account.harness !== avoid)) {
    pool = pool.filter((m) => m.account.harness !== avoid);
  }
  const byUsage = (slice: typeof pool): AutoPick | null => {
    const res = pickByUsage(
      slice.map((m) => ({ name: m.account.name, preferred: m.preferred, on: m.checkModel, usageCap: m.account.usageCap, buffer: m.account.buffer })),
      usage,
      now,
    );
    if (!res) return null;
    const chosen = slice[res.index];
    return { ...res.pick, account: chosen.account.name, model: chosen.explicitModel };
  };
  if (group.strategy === 'alternate-harness') {
    const recent = targetLanes(lanes)
      .flatMap((l) => l.sessions)
      .filter((s) => !s.visitor && !s.helper);
    const harnesses: Harness[] = [];
    for (const m of pool) {
      if (!harnesses.includes(m.account.harness)) harnesses.push(m.account.harness);
    }
    const lastHarness = (h: Harness) => Math.max(0, ...recent.filter((s) => s.harness === h).map((s) => s.startedAt ?? s.lastActivity));
    const chosenHarness = harnesses.reduce((best, h) => (lastHarness(h) < lastHarness(best) ? h : best));
    return byUsage(pool.filter((m) => m.account.harness === chosenHarness));
  }
  if (group.strategy === 'round-robin') {
    const recent = targetLanes(lanes)
      .flatMap((l) => l.sessions)
      .filter((s) => !s.visitor && !s.helper);
    const lastMember = (m: (typeof pool)[number]) => {
      const shared = pool.filter((x) => x.account.name === m.account.name).length > 1;
      const targetModel = m.explicitModel ?? m.checkModel;
      return Math.max(
        0,
        ...recent
          .filter((s) => {
            if (s.account !== m.account.name) return false;
            if (!shared) return true;
            if (targetModel) return !!s.model && modelMatches(s.model, targetModel);
            return !s.model || !pool.some((x) => x.account.name === m.account.name && x.explicitModel && modelMatches(s.model!, x.explicitModel));
          })
          .map((s) => s.startedAt ?? s.lastActivity),
      );
    };
    const chosen = pool.reduce((best, m) => (lastMember(m) < lastMember(best) ? m : best));
    return { account: chosen.account.name, reason: 'first', resetsAt: null, model: chosen.explicitModel };
  }
  if (group.strategy === 'fallback') {
    // The members in their order, each until it's within its buffer; once every one is, the
    // buffers are the sessions' running there, so the most room.
    const first = pool.find((m) => m.clear);
    return first ? { account: first.account.name, reason: 'first', resetsAt: null, model: first.explicitModel } : byUsage(pool);
  }
  return byUsage(pool);
}

/** The choices the sheet lists: Auto when there's more than one account to choose between,
 *  any routing groups, then every account. */
export function choices(accounts: StartAccount[], pick: AutoPick | null, groups: RoutingGroup[] = []): Choice[] {
  return [...(offersAuto(accounts, pick) ? [AUTO] : []), ...groups.map((g) => g.name), ...accounts.map((a) => a.name)];
}

function offersAuto(accounts: StartAccount[], pick: AutoPick | null): boolean {
  return !!pick && startable(accounts).length > 1;
}

/** Where a choice can open. Auto or a routing group opens where the account it picks can. */
export function placesFor(choice: Choice | null, accounts: StartAccount[], pick: AutoPick | null, groups: RoutingGroup[] = [], usage: Usage[] = [], now = 0, lanes: Lane[] = []): StartAccount['places'] {
  const name = accountFor(choice, pick, groups, accounts, usage, now, lanes);
  return accounts.find((a) => a.name === name)?.places ?? [];
}

/** The account a choice starts right now. */
export function accountFor(choice: Choice | null, pick: AutoPick | null, groups: RoutingGroup[] = [], accounts: StartAccount[] = [], usage: Usage[] = [], now = 0, lanes: Lane[] = []): string | null {
  if (!choice) return null;
  if (isAuto(choice)) return pick?.account ?? null;
  const g = isGroup(choice, groups);
  if (g) return pickFromGroup(g, accounts, usage, now, null, null, lanes)?.account ?? null;
  return choice;
}

/** The choice the panel opens with: `wanted` (what a failed start had) while it's still
 *  offered, else the first on the list that can start a session, which is Auto where there's one. */
export function initialChoice(accounts: StartAccount[], pick: AutoPick | null, wanted: Choice | undefined, groups: RoutingGroup[] = [], usage: Usage[] = [], now = 0, lanes: Lane[] = []): Choice | null {
  const offered = choices(accounts, pick, groups).filter((c) => placesFor(c, accounts, pick, groups, usage, now, lanes).length > 0);
  return wanted && offered.includes(wanted) ? wanted : (offered[0] ?? null);
}

/** A model as `--model` takes it, and what menus call it (start.rs's CLAUDE_MODELS). */
export interface ModelOption {
  id: string;
  label: string;
}

/** One line of the agent menu: Auto, a routing group, or an account on a model. */
export interface AgentOption {
  /** `auto`, a routing group name, or `<account>|<model>` with no model for the account's own default. */
  id: string;
  choice: Choice;
  /** Null for the account's default model (or the one its app picks). */
  model: string | null;
}

export function agentId(choice: Choice, model: string | null, groups: RoutingGroup[] = []): string {
  return isAuto(choice) || isGroup(choice, groups) ? choice : `${choice}|${model ?? ''}`;
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

/** What the agent menu lists: Auto when it's offered, any routing groups that can start here,
 *  then every account, on its own default and, where Codealong can ask for one, on each of its
 *  models (`models` for an account that came without its own list). */
export function agentOptions(accounts: StartAccount[], models: ModelOption[], pick: AutoPick | null, groups: RoutingGroup[] = [], usage: Usage[] = [], now = 0, lanes: Lane[] = []): AgentOption[] {
  const out: AgentOption[] = offersAuto(accounts, pick) ? [{ id: AUTO, choice: AUTO, model: null }] : [];
  for (const g of groups) {
    if (pickFromGroup(g, accounts, usage, now, null, null, lanes)) out.push({ id: g.name, choice: g.name, model: null });
  }
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
/** The models to name an account's by: its own list, else those every account lists between them
 *  (an account from a Mac that sends none). */
export function ownModels(a: StartAccount | undefined, all: ModelOption[]): ModelOption[] {
  return a?.models?.length ? a.models : all;
}

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
export function autoModel(defaultModel: string | null, u: Usage | undefined, cap?: number | null): { model: string | null; spent: Meter | null } {
  const own = modelMeter(u, defaultModel);
  if (!own || !near(own, cap)) return { model: null, spent: null };
  const long = /\[1m\]$/i.test(defaultModel ?? '');
  const family = modelFamily(defaultModel);
  for (const f of FALLBACK) {
    if (f === family) continue;
    const m = modelMeter(u, f);
    if (!m || !near(m, cap)) return { model: long ? `${f}[1m]` : f, spent: own };
  }
  return { model: null, spent: own };
}

/** The place an account's `start` setting names; for Auto or a routing group, the one the account it picks names. */
export function settledPlace(choice: Choice | null, accounts: StartAccount[], pick: AutoPick | null, groups: RoutingGroup[] = [], usage: Usage[] = [], now = 0, lanes: Lane[] = []): StartPlace | null {
  const name = accountFor(choice, pick, groups, accounts, usage, now, lanes);
  return accounts.find((a) => a.name === name)?.chosen ?? null;
}

/** The accounts a start in `place` keeps it for as their `start` (start.rs `keep_place`): the
 *  account chosen, or for Auto or a routing group every account it picks among that can start
 *  there, so whichever it picks next, and `codealong start` on auto, opens there too. */
export function keepPlaceFor(choice: Choice | null, accounts: StartAccount[], place: StartPlace, groups: RoutingGroup[] = [], autoGroup?: string): string[] {
  if (!choice) return [];
  const gName = isAuto(choice) ? autoGroup : choice;
  const g = gName ? groups.find((x) => x.name === gName) : undefined;
  const names = g ? (g.members.length > 0 ? g.members.map((m) => m.account) : preferredNames(accounts)) : isAuto(choice) ? preferredNames(accounts) : [choice];
  return names.filter((n) => accounts.find((a) => a.name === n)?.places.some((p) => p.place === place));
}

/** `o` with `place` as the `start` of the accounts named, as reading them again would have it
 *  once `keepPlaceFor`'s are saved. */
export function withChosen(o: StartOptions, names: string[], place: StartPlace): StartOptions {
  return { ...o, accounts: o.accounts.map((a) => (names.includes(a.name) ? { ...a, chosen: place } : a)) };
}

/** Where a choice opens at first: where its account's settings say, else where it last opened,
 *  while that's still offered. */
export function initialPlace(choice: Choice | null, accounts: StartAccount[], pick: AutoPick | null, last: StartPlace | undefined, groups: RoutingGroup[] = [], usage: Usage[] = [], now = 0, lanes: Lane[] = []): StartPlace | null {
  const places = placesFor(choice, accounts, pick, groups, usage, now, lanes);
  const want = settledPlace(choice, accounts, pick, groups, usage, now, lanes) ?? last;
  return places.find((p) => p.place === want)?.place ?? places[0]?.place ?? null;
}

/** Whether a place starts the account's command, which takes a model and --worktree. */
export function onCommandLine(place: StartPlace): boolean {
  return place === 'terminal' || place === 'background';
}

/** Whether a start there, or a resume, takes the model you pick: the command line, or the adapter;
 *  and for a start, the Codex app, whose thread Codealong makes on it (codex/mod.rs
 *  `start_in_app`). A resume there goes as the app's own box sends it. */
export function placeTakesModel(place: StartPlace, resuming = false): boolean {
  return onCommandLine(place) || place === 'codealong' || (place === 'codex-app' && !resuming);
}

/** A place's line under its name in a menu of places (StartControls.tsx `placeLine`): what it is
 *  beyond its name, and that an app goes on its own model, as a model picked gives way there. */
export function placeNote(place: StartPlace, behind = false, resuming = false): string {
  switch (place) {
    case 'terminal':
      return 'A new tab, in front';
    case 'background':
      return behind ? 'A tab behind your windows' : 'No window; attach a terminal any time';
    case 'codealong':
      return 'Its panel here takes your replies';
    default:
      return placeTakesModel(place, resuming) ? 'A new thread there, on the model picked' : 'On the app’s own model';
  }
}

/** What's picked in the quick panel's agent menu and its places, null for nothing picked. */
export interface StartPicks {
  /** An agent menu id (`agentId`). */
  agent: string | null;
  effort: string | null;
  place: StartPlace | null;
  newWorktree: boolean;
  /** A resume's picks (`resumeChoice`), which an app takes no model for (`placeTakesModel`). */
  resuming?: boolean;
}

/** What the picks come to among what's offered: the account a start runs, where, on which model
 *  and effort, and whether its harness makes the new worktree. */
export interface ChosenStart {
  /** The agent menu's line that's chosen: the one picked while it's offered, else the first. */
  agentNow: string | null;
  choice: Choice | null;
  /** The model picked with the account; null for its own. */
  chosenModel: string | null;
  autoFor: boolean;
  account: string | null;
  accountInfo: StartAccount | undefined;
  harness: Harness | null;
  /** The places it can open in: only those that take a model, once a model or effort is picked. */
  places: StartAccount['places'];
  placeNow: StartPlace | null;
  makesWorktree: boolean;
  inNewWorktree: boolean;
  takesAModel: boolean;
  /** The model asked for: Auto's pick, or the one picked, where the place takes one. */
  modelNow: string | null;
  defaultModel: string | null;
  /** What it starts on: the model asked for, else the account's own, where the place takes one. */
  startsOn: string | null;
  /** The model the effort row speaks of. */
  effortModel: string | null;
  /** The levels the effort row offers: its account's. */
  efforts: { id: string; label: string }[];
  effortNow: string | null;
  effortWord: string | null;
  /** What the agent chip says of the model (`modelWords`). */
  modelShown: string | null;
}

/**
 * What `picks` come to, given the accounts and what the agent menu offers (`agentOptions`, with
 * Auto's `pick` and the `first` choice). A choice with no place of its own picked opens where its
 * account's settings say, else where it last opened (`lastPlace`, read only then), else in the
 * first place it can. A model or an effort is asked for on the command line or of Codealong's
 * adapter, so picking one leaves only those places; the effort goes wherever the model does, to
 * a harness that takes one, except to Haiku.
 */
export function startChoice(
  accounts: StartAccount[],
  models: ModelOption[],
  efforts: { id: string; label: string }[],
  usage: Usage[],
  pick: AutoPick | null,
  agents: AgentOption[],
  first: Choice | null,
  picks: StartPicks,
  lastPlace: (choice: Choice) => StartPlace | undefined,
  groups: RoutingGroup[] = [],
  now = 0,
  lanes: Lane[] = [],
): ChosenStart {
  const { agent, effort, place, newWorktree, resuming = false } = picks;
  const acceptedAgent = (id: string) => {
    if (agents.some((o) => o.id === id)) return true;
    const { choice: c, model: m } = parseAgent(id);
    const a = accounts.find((x) => x.name === c);
    return !!a && a.places.length > 0 && (!m || (takesModel(a) && offersModel(a, m)));
  };
  const agentNow = agent && acceptedAgent(agent) ? agent : first ? agentId(first, null, groups) : null;
  const { choice, model: chosenModel } = agentNow ? parseAgent(agentNow) : { choice: null, model: null };
  const group = isGroup(choice, groups);
  const autoFor = isAuto(choice) || !!group;
  const activePick = isAuto(choice) ? pick : group ? pickFromGroup(group, accounts, usage, now, null, null, lanes) : null;
  const account = activePick ? activePick.account : accountFor(choice, pick, groups, accounts, usage, now, lanes);
  const accountInfo = accounts.find((a) => a.name === account);
  const harness = accountInfo?.harness ?? null;
  const autoModelPick = autoFor
    ? activePick?.model
      ? { model: activePick.model, spent: null }
      : autoModel(accountInfo?.defaultModel ?? null, usage.find((u) => u.account === account), accountInfo?.usageCap)
    : null;
  const allPlaces = placesFor(choice, accounts, pick, groups, usage, now, lanes);
  // The levels its account takes (Codex's go past Claude Code's), else those offered for any.
  const levels = accountInfo?.efforts?.length ? accountInfo.efforts : efforts;
  const takesEffort = (accountInfo?.efforts?.length ?? 0) > 0;
  const tuned = (!!chosenModel || !!activePick?.model || (!!effort && takesEffort)) && allPlaces.some((p) => placeTakesModel(p.place, resuming));
  const places = tuned ? allPlaces.filter((p) => placeTakesModel(p.place, resuming)) : allPlaces;
  const placeNow = [place, choice ? (settledPlace(choice, accounts, pick, groups, usage, now, lanes) ?? lastPlace(choice)) : undefined].map((p) => places.find((x) => x.place === p)?.place).find(Boolean) ?? places[0]?.place ?? null;
  // A harness whose command line makes a worktree for a session (Claude Code's `--worktree`).
  const makesWorktree = !!placeNow && onCommandLine(placeNow) && !!accountInfo?.worktree;
  // A harness whose command line takes a model; Codealong hands it to an adapter the same way.
  const placeTakes = !!placeNow && placeTakesModel(placeNow, resuming);
  const takesAModel = placeTakes && ((accountInfo?.models?.length ?? 0) > 0 || (!!chosenModel && !!accountInfo && offersModel(accountInfo, chosenModel)));
  const modelNow = takesAModel ? (autoFor ? (autoModelPick?.model ?? null) : chosenModel) : null;
  const defaultModel = accountInfo?.defaultModel ?? null;
  const startsOn = takesAModel ? (modelNow ?? defaultModel) : null;
  // A Mac whose Codealong doesn't know effort yet lists no levels. An account that lists no
  // models still takes one (a Codex account Codex hasn't run in has no list yet).
  const effortTaken = placeTakes && takesEffort && modelFamily(startsOn) !== 'haiku' && levels.some((e) => e.id === effort);
  const effortNow = effortTaken ? effort : null;
  return {
    agentNow,
    choice,
    chosenModel,
    autoFor,
    account,
    accountInfo,
    harness,
    places,
    placeNow,
    makesWorktree,
    inNewWorktree: makesWorktree && newWorktree,
    takesAModel,
    modelNow,
    defaultModel,
    startsOn,
    effortModel: (autoFor ? autoModelPick?.model : chosenModel) ?? defaultModel,
    efforts: levels,
    effortNow,
    effortWord: levels.find((e) => e.id === effortNow)?.label ?? null,
    modelShown: modelWords(takesAModel, placeNow, harness, modelNow, defaultModel, ownModels(accountInfo, models)),
  };
}

// What the quick panel's start row, its label while it starts and the agent menu say. Each is
// the words for one choice of account, model, effort and place, worked out from the choice.

/** What a session starts on, for the start row and the label while it starts:
 *  " on Opus at high effort", or nothing for the account's own. */
export function onModelWords(startsOn: string | null, effortWord: string | null, models: ModelOption[]): string {
  return `${startsOn ? ` on ${modelName(startsOn, models)}` : ''}${effortWord ? ` at ${effortWord.toLowerCase()} effort` : ''}`;
}

/** What the agent chip says of the model: the one a session starts on where Codealong hands the
 *  model over (`takesAModel`), else who picks it. Null when nothing need be said. */
export function modelWords(takesAModel: boolean, place: StartPlace | null, harness: Harness | null, model: string | null, defaultModel: string | null, models: ModelOption[]): string | null {
  if (!takesAModel) {
    if (place === 'claude-app') return 'model chosen in the Claude app';
    if (place === 'codex-app') return 'model chosen in Codex';
    if (harness === 'opencode') return 'OpenCode’s model';
    if (harness === 'codex') return 'Codex’s model';
    if (harness === 'antigravity') return 'Antigravity’s model';
    if (harness === 'cursor') return 'Cursor’s model';
    return null;
  }
  if (model) return modelName(model, models);
  return defaultModel ? modelName(defaultModel, models) : 'default model';
}

/** A start as the start row describes it. */
export interface StartPlan {
  /** The worktree it starts in, when there's one. */
  lane: Pick<Lane, 'repo' | 'callsign' | 'startsChats'> | null;
  account: string | null;
  place: StartPlace | null;
  /** On another Mac: its name once your devices are read, and why it didn't say where its
   *  accounts can start, if it didn't. Null on this one. */
  mac: { name: string | null; error: string | null } | null;
  /** The board isn't read yet. */
  reading: boolean;
  /** Claude Code makes a new worktree for it. */
  newWorktree: boolean;
  /** `onModelWords`. */
  onModel: string;
  /** Something is typed, which goes as its prompt. */
  prompted: boolean;
  /** What the terminal place is called: Ghostty, iTerm2… */
  terminal: string;
  /** Background opens the terminal behind your windows: the account's harness has no background
   *  service of its own. */
  behind: boolean;
}

/** The start row's line: what starting does, and where. */
export function startHappens(p: StartPlan): string {
  const mac = p.mac?.name ?? 'That Mac';
  const there = startHappensThere(p, mac);
  return p.mac && p.lane && p.account && p.place ? `On ${mac}: ${there}` : there;
}

/** Where a start goes, as its words say it: the worktree, or a chat of its own (the worktree
 *  chip says No project). */
function startsIn(lane: Pick<Lane, 'callsign' | 'startsChats'>): string {
  return lane.startsChats ? 'a chat of its own' : lane.callsign;
}

function startHappensThere({ lane, account, place, mac: on, reading, newWorktree, onModel, prompted, terminal, behind }: StartPlan, mac: string): string {
  if (!lane) return on ? `No worktrees in this project on ${mac}.` : reading ? 'Reading worktrees…' : 'No worktrees in this project yet.';
  if (!account || !place) return on?.error ? `${mac} didn’t say where its accounts can start: ${on.error}` : '';
  const does = startDoes({ lane, who: `${account}${onModel}`, onModel, place, newWorktree, prompted, terminal, behind });
  // What the start row has room to add after the words it shares with Jev's suggestions.
  const more =
    place === 'background' && !behind
      ? ' Attach a terminal any time.'
      : place === 'codealong'
        ? ' Its panel here shows the reply, takes the next prompt and answers what it asks.'
        : place === 'claude-app'
          ? ' You send it from there.'
          : place === 'codex-app' && !prompted
            ? ' You write the prompt there.'
            : '';
  return `${does}${more}`;
}

// A start's title, where and line are said the same by the start row and Jev's start
// suggestions above it, so the start ⌘⏎ makes reads against theirs at a glance.

/** A start's title: where it starts. */
export function startTitleWords(lane: Pick<Lane, 'repo' | 'callsign' | 'startsChats' | 'origin'>, newWorktree: boolean): string {
  if (lane.startsChats) return 'Start a chat in no project';
  return newWorktree ? `Start in a new worktree of ${lane.repo}` : `Start in ${lane.origin === 'main' ? `${lane.callsign} (main)` : lane.callsign}`;
}

/** The project and repo a start is in, at the right of its row. */
export function startWhereWords(lane: Pick<Lane, 'project' | 'repo' | 'startsChats'>): string {
  return lane.startsChats ? 'No project' : `${lane.project} · ${lane.repo}`;
}

/** What a start does: "Starts claude on Opus in api in the background." `who` is the account
 *  with what it starts on (`onModelWords`), which is `onModel`; `newWorktree`, that its harness
 *  makes one for it. */
export function startDoes(s: { lane: Pick<Lane, 'repo' | 'callsign' | 'startsChats'>; who: string; onModel?: string; place: StartPlace | null; newWorktree: boolean; prompted: boolean; terminal: string; behind: boolean }): string {
  // The row's title says a chat is in no project.
  const there = s.lane.startsChats ? 'a folder of its own' : s.lane.callsign;
  if (s.place === 'codealong') return `Codealong runs ${s.who} in ${there}.`;
  if (s.place === 'claude-app') return `The Claude app opens a new session in ${there}${s.prompted ? ' with this prompt filled in' : ''}.`;
  // Codealong makes the thread a prompt starts, on what's picked; without one the app picks.
  if (s.place === 'codex-app') return s.prompted ? `The Codex app starts a new thread${s.onModel ?? ''} in ${there} on this prompt.` : `Codex opens a new thread in ${there}.`;
  const how = s.place === 'terminal' ? ` in ${s.terminal}` : s.place === 'background' ? (s.behind ? ` in ${s.terminal}, behind your windows` : ' in the background') : '';
  return s.newWorktree && !s.lane.startsChats ? `Claude Code makes a new worktree of ${s.lane.repo} and starts ${s.who} there${how}.` : `Starts ${s.who} in ${there}${how}.`;
}

/** What the window that opened the panel says while a start is under way. `mac` is the other
 *  Mac's name, when it starts on one Codealong has heard from. */
export function startingLabel(place: StartPlace, account: string, onModel: string, lane: Pick<Lane, 'repo' | 'callsign' | 'startsChats'>, newWorktree: boolean, mac: string | null): string {
  const onMac = mac ? ` on ${mac}` : '';
  if (place === 'terminal' || place === 'background') return `Starting ${account}${onModel} in ${newWorktree ? `a new worktree of ${lane.repo}` : startsIn(lane)}${place === 'background' ? ' in the background' : ''}${onMac}…`;
  if (place === 'codealong') return `Starting ${account}${onModel} in Codealong${onMac}…`;
  if (place === 'claude-app') return `Opening a new session in the Claude app${onMac}…`;
  return `Opening a new thread in Codex${onMac}…`;
}

/** What the agent chip says: Auto, the routing group, or the account, and after it the account
 *  Auto or the group picked, the model and the effort. With no accounts, why: another Mac's may
 *  not have come yet. */
export function agentChipWords(c: { auto: boolean; group?: string | null; account: string | null; accounts: number; remote: { read: boolean; error: string | null } | null; model: string | null; effortWord: string | null }): { label: string; detail: string } {
  const none = c.remote && !c.remote.read ? (c.remote.error ? 'Couldn’t read its accounts' : 'Reading accounts…') : 'No accounts';
  const label = c.accounts === 0 ? none : c.group ? c.group : c.auto ? 'Auto' : (c.account ?? 'No account');
  const detail = [c.auto || c.group ? c.account : null, c.model, c.effortWord ? `${c.effortWord} effort` : null].filter(Boolean).join(' · ');
  return { label, detail };
}

/** Why Auto starts the account and model it does. `fallback`: it isn't one of the preferred
 *  accounts, which are all near a limit. */
export function autoWhy(p: AutoPick, spent: Meter | null, fallback: boolean, now: number): string {
  if (fallback) return 'preferred accounts are near their limits';
  if (spent) return `${spent.label}’s week is ${Math.round(spent.percent)}% used`;
  if (p.reason === 'reset' && p.resetsAt) return `week resets ${resetsPhrase(p.resetsAt, now)}`;
  return p.reason === 'room' ? 'most room' : '';
}

/** The agent menu's Auto line: what its filter matches, and the account, model and why. */
export function autoOptionWords(pick: AutoPick, accounts: StartAccount[], usage: Usage[], models: ModelOption[], now: number): { text: string; detail: string } {
  const a = accounts.find((x) => x.name === pick.account);
  const m = pick.model ? { model: pick.model, spent: null } : autoModel(a?.defaultModel ?? null, usage.find((u) => u.account === pick.account), a?.usageCap);
  const model = m.model ?? a?.defaultModel ?? null;
  const why = autoWhy(pick, m.spent, !preferredNames(accounts).includes(pick.account), now);
  return {
    text: `auto ${pick.account} ${a ? HARNESS_WORD[a.harness] : ''} ${model ?? ''}`,
    detail: [pick.account, model ? modelName(model, models) : null, why].filter(Boolean).join(' · '),
  };
}

/** A routing group's line in the agent menu: what its filter matches, and the account, model and strategy. */
export function groupOptionWords(group: RoutingGroup, pick: AutoPick, accounts: StartAccount[], usage: Usage[], models: ModelOption[], now: number): { text: string; detail: string } {
  const a = accounts.find((x) => x.name === pick.account);
  const m = pick.model ? { model: pick.model, spent: null } : autoModel(a?.defaultModel ?? null, usage.find((u) => u.account === pick.account), a?.usageCap);
  const model = m.model ?? a?.defaultModel ?? null;
  const strat = group.strategy === 'alternate-harness' ? 'alternates harnesses' : group.strategy === 'round-robin' ? 'round robin' : autoWhy(pick, m.spent, false, now);
  return {
    text: `${group.name} ${pick.account} ${a ? HARNESS_WORD[a.harness] : ''} ${model ?? ''}`,
    detail: [pick.account, model ? modelName(model, models) : null, strat].filter(Boolean).join(' · '),
  };
}

/** An account's heading in the agent menu: its agent and plan, and what shows when its
 *  meters don't. */
export function accountHeadWords(a: StartAccount, u: Usage | undefined, note: string): { detail: string; note: string } {
  const cant = a.places.length === 0;
  return { detail: [HARNESS_WORD[a.harness], u?.plan].filter(Boolean).join(' · '), note: cant ? 'Can’t start from here' : note };
}

/** One of an account's lines in the agent menu: what its filter matches, and what it says. Its
 *  models go by its own names for them: two harnesses can list one id (Codex and Cursor each have
 *  `gpt-5.6-sol`) under names of their own. */
export function agentOptionWords(o: Pick<AgentOption, 'model'>, a: StartAccount, all: ModelOption[]): { text: string; label: string; detail: string | null } {
  const models = ownModels(a, all);
  const cant = a.places.length === 0;
  const text = `${a.name} ${HARNESS_WORD[a.harness]} ${o.model ? `${o.model} ${modelName(o.model, models)}` : 'default'}`;
  if (o.model) return { text, label: modelName(o.model, models), detail: models.find((m) => m.id === o.model)?.label.split(': ')[1] ?? null };
  if (takesModel(a)) return { text, label: `Default${a.defaultModel ? ` · ${modelName(a.defaultModel, models)}` : ''}`, detail: a.harness === 'claude' ? 'From its settings.json' : `The one ${HARNESS_WORD[a.harness]} starts on` };
  const detail = cant ? (a.unavailable ?? null) : a.harness === 'codex' ? 'The model is chosen in Codex' : a.harness === 'opencode' ? 'The model comes from OpenCode’s settings' : null;
  return { text, label: a.name, detail };
}

/** What the effort row under the agent menu's models says beside the levels: who picks for a
 *  harness that picks its own (an account that lists no levels: `takesEffort`), that Haiku takes
 *  none, or null for the keys that step it. */
export function effortNote(harness: Harness | null, model: string | null, takesEffort = harness === 'claude'): string | null {
  if (harness && !takesEffort) return `${HARNESS_WORD[harness]} picks its own`;
  return modelFamily(model) === 'haiku' ? 'Haiku takes none' : null;
}

/** What the effort row's Default says it is: where the account's own level is set. */
export function effortDefaultTitle(harness: Harness | null): string {
  return harness === 'codex' ? 'The account’s own: config.toml’s model_reasoning_effort, else the model’s' : 'The account’s own: settings.json’s effortLevel, else the model’s';
}

/** What the New worktree box says of itself. */
export function newWorktreeTitle(makesWorktree: boolean, place: StartPlace | null, harness: Harness | null): string {
  const who = harness ? HARNESS_WORD[harness] : 'It';
  if (makesWorktree) return `${who} makes the worktree: its folder, branch and base.`;
  return place && onCommandLine(place) ? `${who} can’t start in a new worktree from here.` : 'The app has its own worktree choice; pick it there before you send.';
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

/** The effort one step along from `effort` (null is the account's own), among `efforts`, kept
 *  within them: ← and → under a model menu. */
export function effortStep(efforts: { id: string }[], effort: string | null, delta: -1 | 1): string | null {
  const steps: (string | null)[] = [null, ...efforts.map((e) => e.id)];
  return steps[Math.min(steps.length - 1, Math.max(0, Math.max(0, steps.indexOf(effort)) + delta))];
}

// Going on with a session that isn't running (start.rs `resume`): a start for the session's own
// account, so the picks come to what they would for a start (`startChoice`), among the places
// its sessions go on in.

/** What a resume picks: a line of the agent menu for the session's account (its own model, or
 *  one of its models), the effort and the place. */
export type ResumePicks = Pick<StartPicks, 'agent' | 'effort' | 'place'>;

/** The session's account as a resume offers it: only the places its sessions go on in, and no
 *  `start` setting, which says where new sessions start. */
export function resumeAccount(a: StartAccount): StartAccount {
  return { ...a, places: a.places.filter((p) => a.resumes?.includes(p.place)), chosen: null };
}

/**
 * What a resume's picks come to (`startChoice` for the session's account alone): where it can go
 * on, only where a model goes once one is picked, and the model and effort where the place takes
 * them. Without a place picked it goes on where its account's sessions last went on from here
 * (`last`), else in a terminal, as one did before there was a choice.
 */
export function resumeChoice(account: StartAccount, models: ModelOption[], picks: ResumePicks, last: StartPlace | undefined): ChosenStart {
  const a = resumeAccount(account);
  const listed = a.models ?? models;
  return startChoice([a], listed, a.efforts ?? [], [], null, agentOptions([a], listed, null), a.name, { ...picks, newWorktree: false, resuming: true }, () => last ?? 'terminal');
}

/** What the engine is asked for, from what the picks came to. */
export function resumeIn(c: Pick<ChosenStart, 'placeNow' | 'modelNow' | 'effortNow'>): ResumeIn | null {
  return c.placeNow ? { place: c.placeNow, model: c.modelNow, effort: c.effortNow } : null;
}

/** What sending to a session that isn't running does, after "it isn't running, so": its box's
 *  placeholder says it. `behind`: Background is a terminal behind your windows. */
export function resumeWords(place: StartPlace, terminal: string, behind = false): string {
  switch (place) {
    case 'background':
      return behind ? `this resumes it in ${terminal}, behind your windows` : 'this resumes it in the background';
    case 'claude-app':
      return 'the Claude app opens it with this on the clipboard';
    case 'codex-app':
      return 'this goes on in the Codex app';
    case 'codealong':
      return 'Codealong runs it from now on';
    default:
      return `this resumes it in ${terminal}`;
  }
}

const RESUMED_KEY = 'codealong.resumed';
const PLACES: StartPlace[] = ['terminal', 'background', 'claude-app', 'codex-app', 'codealong'];

/** The place each account's sessions last went on in, as JSON; what isn't a place is left out. */
const placeByAccount: Codec<Record<string, StartPlace>> = {
  parse: (raw) => {
    const v: unknown = JSON.parse(raw);
    if (typeof v !== 'object' || v === null) return undefined;
    return Object.fromEntries(Object.entries(v).filter((e): e is [string, StartPlace] => PLACES.includes(e[1] as StartPlace)));
  },
  format: (v) => JSON.stringify(v),
};

/** Where `account`'s sessions last went on from this Mac's windows. */
export function resumedIn(account: string): StartPlace | undefined {
  return readStored(RESUMED_KEY, placeByAccount, {} as Record<string, StartPlace>)[account];
}

/** Keeps where a session of `account` went on, for the next one. */
export function rememberResumed(account: string, place: StartPlace): void {
  writeStored(RESUMED_KEY, placeByAccount, { ...readStored(RESUMED_KEY, placeByAccount, {} as Record<string, StartPlace>), [account]: place });
}
