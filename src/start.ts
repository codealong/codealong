// The rules for starting a session from the quick panel: the account and model Auto starts,
// what the agent menu offers (every account on every model it can start on), and where each
// choice can open. Auto chooses among every account that can start a session, preferred ones
// first (config.toml's `preferred`), so it keeps to the agents you'd rather use while they have
// room. start.rs's `auto_pick` has the same rule for `codealong start`.

import { resetsPhrase } from './format';
import { HARNESS_WORD } from './harness';
import { failedNotice } from './notices';
import { readStored, writeStored, type Codec } from './stored';
import type { Harness, Lane, Meter, Notice, ResumeIn, StartAccount, StartOptions, StartPlace, Usage } from './types';

/** The notice while where each account can start a session can't be read (the main window's
 *  Review with and the quick panel ask), which ends once it's read. */
export const START_OPTIONS_NOTICE = 'start-options';
export function startOptionsFailed(e: unknown): Notice {
  return failedNotice(START_OPTIONS_NOTICE, 'Can’t read where sessions start', 'Couldn’t read where each account can start a session, so the last answer read stands', e);
}

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

/** Whether a place starts the account's command, which takes a model and --worktree. */
export function onCommandLine(place: StartPlace): boolean {
  return place === 'terminal' || place === 'background';
}

/** Whether a Claude Code start there takes the model you pick: the command line, or the adapter. */
export function placeTakesModel(place: StartPlace): boolean {
  return onCommandLine(place) || place === 'codealong';
}

/** What's picked in the quick panel's agent menu and its places, null for nothing picked. */
export interface StartPicks {
  /** An agent menu id (`agentId`). */
  agent: string | null;
  effort: string | null;
  place: StartPlace | null;
  newWorktree: boolean;
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
export function startChoice(accounts: StartAccount[], models: ModelOption[], efforts: { id: string; label: string }[], usage: Usage[], pick: AutoPick | null, agents: AgentOption[], first: Choice | null, picks: StartPicks, lastPlace: (choice: Choice) => StartPlace | undefined): ChosenStart {
  const { agent, effort, place, newWorktree } = picks;
  const agentNow = agent && agents.some((o) => o.id === agent) ? agent : first ? agentId(first, null) : null;
  const { choice, model: chosenModel } = agentNow ? parseAgent(agentNow) : { choice: null, model: null };
  const autoFor = isAuto(choice);
  const account = accountFor(choice, pick);
  const accountInfo = accounts.find((a) => a.name === account);
  const harness = accountInfo?.harness ?? null;
  const autoModelPick = autoFor ? autoModel(accountInfo?.defaultModel ?? null, usage.find((u) => u.account === account)) : null;
  const allPlaces = placesFor(choice, accounts, pick);
  const takesEffort = (accountInfo?.efforts?.length ?? 0) > 0;
  const tuned = !!chosenModel || (!!effort && takesEffort);
  const places = tuned ? allPlaces.filter((p) => placeTakesModel(p.place)) : allPlaces;
  const placeNow = [place, choice ? (settledPlace(choice, accounts, pick) ?? lastPlace(choice)) : undefined].map((p) => places.find((x) => x.place === p)?.place).find(Boolean) ?? places[0]?.place ?? null;
  // A harness whose command line makes a worktree for a session (Claude Code's `--worktree`).
  const makesWorktree = !!placeNow && onCommandLine(placeNow) && !!accountInfo?.worktree;
  // A harness whose command line takes a model; Codealong hands it to an adapter the same way.
  const takesAModel = !!placeNow && placeTakesModel(placeNow) && (accountInfo?.models?.length ?? 0) > 0;
  const modelNow = takesAModel ? (autoFor ? (autoModelPick?.model ?? null) : chosenModel) : null;
  const defaultModel = accountInfo?.defaultModel ?? null;
  const startsOn = takesAModel ? (modelNow ?? defaultModel) : null;
  // A Mac whose Codealong doesn't know effort yet lists no levels.
  const effortTaken = takesAModel && takesEffort && modelFamily(startsOn) !== 'haiku';
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
    effortNow,
    effortWord: efforts.find((e) => e.id === effortNow)?.label ?? null,
    modelShown: modelWords(takesAModel, placeNow, harness, modelNow, defaultModel, models),
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

function startHappensThere({ lane: worktree, account, place, mac: on, reading, newWorktree, onModel, prompted, terminal, behind }: StartPlan, mac: string): string {
  if (!worktree) return on ? `No worktrees in this project on ${mac}.` : reading ? 'Reading worktrees…' : 'No worktrees in this project yet.';
  if (!account || !place) return on?.error ? `${mac} didn’t say where its accounts can start: ${on.error}` : '';
  const withPrompt = prompted ? ' with this prompt' : '';
  const lane = { repo: worktree.repo, callsign: startsIn(worktree) };
  switch (place) {
    case 'background':
      if (behind) return `${terminal} starts ${account}${onModel} in ${newWorktree ? `a new worktree of ${lane.repo}` : lane.callsign}${withPrompt}, behind your windows, so you stay where you are.`;
      return newWorktree
        ? `Claude Code makes a new worktree of ${lane.repo} and starts ${account}${onModel} there in the background${withPrompt}. Attach a terminal any time.`
        : `${account}${onModel} starts in the background in ${lane.callsign}${withPrompt}, with no terminal. Attach one any time.`;
    case 'terminal':
      return newWorktree ? `Claude Code makes a new worktree of ${lane.repo} and starts ${account}${onModel} there in ${terminal}${withPrompt}.` : `${terminal} starts ${account}${onModel} in ${lane.callsign}${withPrompt}.`;
    case 'codealong':
      return `Codealong runs ${account}${onModel} in ${lane.callsign}${withPrompt}. Its panel here shows the reply, takes the next prompt and answers what it asks.`;
    case 'claude-app':
      return `The Claude app opens a new session in ${lane.callsign}${prompted ? ' with this prompt filled in' : ''}. You send it from there.`;
    default:
      return prompted ? `The Codex app starts a new thread in ${lane.callsign} on this prompt.` : `Codex opens a new thread in ${lane.callsign}. You write the prompt there.`;
  }
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

/** What the agent chip says: Auto or the account, and after it the account Auto picked, the
 *  model and the effort. With no accounts, why: another Mac's may not have come yet. */
export function agentChipWords(c: { auto: boolean; account: string | null; accounts: number; remote: { read: boolean; error: string | null } | null; model: string | null; effortWord: string | null }): { label: string; detail: string } {
  const none = c.remote && !c.remote.read ? (c.remote.error ? 'Couldn’t read its accounts' : 'Reading accounts…') : 'No accounts';
  const label = c.accounts === 0 ? none : c.auto ? 'Auto' : (c.account ?? 'No account');
  const detail = [c.auto ? c.account : null, c.model, c.effortWord ? `${c.effortWord} effort` : null].filter(Boolean).join(' · ');
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
  const m = autoModel(a?.defaultModel ?? null, usage.find((u) => u.account === pick.account));
  const model = m.model ?? a?.defaultModel ?? null;
  const why = autoWhy(pick, m.spent, !preferredNames(accounts).includes(pick.account), now);
  return {
    text: `auto ${pick.account} ${a ? HARNESS_WORD[a.harness] : ''} ${model ?? ''}`,
    detail: [pick.account, model ? modelName(model, models) : null, why].filter(Boolean).join(' · '),
  };
}

/** An account's heading in the agent menu: its agent, plan and star, and what shows when its
 *  meters don't. */
export function accountHeadWords(a: StartAccount, u: Usage | undefined, note: string): { detail: string; note: string } {
  const cant = a.places.length === 0;
  return { detail: [HARNESS_WORD[a.harness], u?.plan, a.preferred ? 'Preferred' : null].filter(Boolean).join(' · '), note: cant ? 'Can’t start from here' : note };
}

/** One of an account's lines in the agent menu: what its filter matches, and what it says. */
export function agentOptionWords(o: Pick<AgentOption, 'model'>, a: StartAccount, models: ModelOption[]): { text: string; label: string; detail: string | null } {
  const cant = a.places.length === 0;
  const text = `${a.name} ${HARNESS_WORD[a.harness]} ${o.model ? `${o.model} ${modelName(o.model, models)}` : 'default'}`;
  if (o.model) return { text, label: modelName(o.model, models), detail: models.find((m) => m.id === o.model)?.label.split(': ')[1] ?? null };
  if (takesModel(a)) return { text, label: `Default${a.defaultModel ? ` · ${modelName(a.defaultModel, models)}` : ''}`, detail: a.harness === 'claude' ? 'From its settings.json' : `The one ${HARNESS_WORD[a.harness]} starts on` };
  const detail = cant ? (a.unavailable ?? null) : a.harness === 'codex' ? 'The model is chosen in Codex' : a.harness === 'opencode' ? 'The model comes from OpenCode’s settings' : null;
  return { text, label: a.name, detail };
}

/** What the effort row under the agent menu's models says beside the levels: who picks for a
 *  harness that picks its own, that Haiku takes none, or null for the keys that step it. */
export function effortNote(harness: Harness | null, model: string | null): string | null {
  if (harness && harness !== 'claude') return `${HARNESS_WORD[harness]} picks its own`;
  return modelFamily(model) === 'haiku' ? 'Haiku takes none' : null;
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
  return startChoice([a], listed, a.efforts ?? [], [], null, agentOptions([a], listed, null), a.name, { ...picks, newWorktree: false }, () => last ?? 'terminal');
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
