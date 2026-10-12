import { describe, expect, it, vi } from 'vitest';
import { resetsPhrase } from './format';
import {
  accountFor,
  accountHeadWords,
  agentChipWords,
  AUTO,
  agentId,
  agentOptions,
  agentOptionWords,
  autoModel,
  autoOptionWords,
  autoPick,
  autoWhy,
  choices,
  effortNote,
  initialChoice,
  initialPlace,
  keepPlaceFor,
  modelFamily,
  modelMeter,
  modelName,
  modelWords,
  newWorktreeTitle,
  onCommandLine,
  onModelWords,
  parseAgent,
  placeNote,
  placesFor,
  placeTakesModel,
  preferredNames,
  resumeAccount,
  resumeChoice,
  resumedIn,
  resumeIn,
  resumeWords,
  rememberResumed,
  settledPlace,
  startChoice,
  startHappens,
  startingLabel,
  startTitleWords,
  startWhereWords,
  takesModel,
  withLists,
  withChosen,
  type AutoPick,
  type StartPicks,
  type StartPlan,
} from './start';
import * as fx from './testing/fixtures';
import type { Meter, StartAccount, Usage } from './types';

const ghostty = { place: 'terminal' as const, label: 'Ghostty' };
const claudeApp = { place: 'claude-app' as const, label: 'Claude app' };
const codexApp = { place: 'codex-app' as const, label: 'Codex app' };

const ACCOUNTS: StartAccount[] = [
  fx.startAccount({ name: 'claude', places: [ghostty, claudeApp] }),
  fx.startAccount({ name: 'claude2', places: [ghostty] }),
  fx.startAccount({ name: 'codex', harness: 'codex', places: [codexApp] }),
];

const NOW = Date.UTC(2026, 8, 24, 12);
const DAY = 86_400_000;
const HOUR = 3_600_000;

function meter(key: string, percent: number, resetsAt: number | null = null): Meter {
  return fx.meter({ key, percent, resetsAt, severity: percent >= 90 ? 'critical' : 'normal' });
}

/** An account's 5-hour and weekly use, and in how many hours its week resets. */
function usage(account: string, session: number, week: number, weekResetsIn: number | null = 5 * 24, extra: Partial<Usage> = {}): Usage {
  return fx.usage({
    account,
    harness: account.startsWith('codex') ? 'codex' : 'claude',
    meters: [meter('session', session, NOW + 2 * HOUR), meter('week', week, weekResetsIn === null ? null : NOW + weekResetsIn * HOUR)],
    asOf: NOW,
    ...extra,
  });
}

const pick = (u: Usage[], accounts = ACCOUNTS) => autoPick(accounts, u, NOW);
const star = (...names: string[]) => ACCOUNTS.map((a) => ({ ...a, preferred: names.includes(a.name) }));

// start.rs's auto_pick tests the same cases.
describe('Auto', () => {
  it('spends the week that resets soonest, so its room is the first used', () => {
    // claude2 has less room, but its week resets first.
    expect(pick([usage('claude', 10, 20, 100), usage('claude2', 30, 70, 90)])).toEqual({ account: 'claude2', reason: 'reset', resetsAt: NOW + 90 * HOUR });
    expect(pick([usage('claude', 10, 20, 40), usage('claude2', 30, 70, 6)])?.account).toBe('claude2');
    // A week that hasn't started has nothing to lose.
    expect(pick([usage('claude', 10, 20, 100), usage('claude2', 0, 0, null)])?.account).toBe('claude');
  });

  it('leaves the last room before a reset to the sessions already running there', () => {
    // At 85% of a limit, its 5-hour one too, claude2's week resetting first no longer counts:
    // claude's is the next.
    expect(pick([usage('claude', 10, 20, 100), usage('claude2', 30, 85, 6)])).toEqual({ account: 'claude', reason: 'reset', resetsAt: NOW + 100 * HOUR });
    expect(pick([usage('claude', 10, 20, 100), usage('claude2', 87, 40, 6)])?.account).toBe('claude');
    expect(pick([usage('claude', 10, 20, 100), usage('claude2', 30, 84, 6)])?.reason).toBe('reset');
    // It still has room, so it's taken when no other account has more.
    expect(pick([usage('claude', 10, 89, 100), usage('claude2', 30, 86, 6)])).toEqual({ account: 'claude2', reason: 'room', resetsAt: null });
  });

  it('without a week under way, starts the account whose fullest limit is least full', () => {
    // claude's fullest limit is 38%, claude2's is 45%: claude has more room, though its 5-hour is fuller.
    expect(pick([usage('claude', 38, 20, null), usage('claude2', 12, 45, null)])).toEqual({ account: 'claude', reason: 'room', resetsAt: null });
  });

  it('passes over an account near a limit while another has room', () => {
    // claude2's week resets soon, but its 5-hour window is nearly used up.
    expect(pick([usage('claude', 10, 20), usage('claude2', 95, 70, 6)])?.account).toBe('claude');
    // When every account is near a limit, the least full.
    expect(pick([usage('claude', 92, 20), usage('claude2', 95, 70, 6), usage('codex', 99, 20)])).toEqual({ account: 'claude', reason: 'room', resetsAt: null });
    // One without numbers may have room.
    expect(pick([usage('claude', 92, 20), usage('claude2', 95, 70, 6)])).toEqual({ account: 'codex', reason: 'first', resetsAt: null });
  });

  it('keeps to preferred accounts while one has room', () => {
    const codexSoon = usage('codex', 0, 10, 3);
    // Without a star, the first account's agent is preferred.
    expect(preferredNames(ACCOUNTS)).toEqual(['claude', 'claude2']);
    expect(pick([usage('claude', 50, 80), usage('claude2', 50, 85), codexSoon])?.account).toBe('claude');
    // Out of room on both: Codex.
    expect(pick([usage('claude', 50, 95), usage('claude2', 91, 85), codexSoon])).toEqual({ account: 'codex', reason: 'reset', resetsAt: NOW + 3 * HOUR });
    // A preferred account without numbers yet still goes first.
    expect(pick([codexSoon])).toEqual({ account: 'claude', reason: 'first', resetsAt: null });
    // Stars decide once there are any.
    expect(preferredNames(star('claude2'))).toEqual(['claude2']);
    expect(pick([usage('claude', 10, 20, 6), usage('claude2', 50, 80), codexSoon], star('claude2'))?.account).toBe('claude2');
    expect(pick([usage('claude', 10, 20, 6), usage('claude2', 50, 95), codexSoon], star('claude2'))?.account).toBe('codex');
  });

  it('passes over signed-out accounts', () => {
    expect(pick([usage('claude', 0, 0, null, { meters: [], needsSignIn: true }), usage('claude2', 80, 20)])?.account).toBe('claude2');
    expect(pick([usage('claude', 20, 20, 3, { needsSignIn: true }), usage('claude2', 80, 20)])?.account).toBe('claude2');
    expect(pick([])).toEqual({ account: 'claude', reason: 'first', resetsAt: null });
  });

  it('counts only the limits the chosen model spends', () => {
    const fable = (percent: number, resetsIn: number): Meter => ({ ...meter('model:Fable', percent, NOW + resetsIn * HOUR), model: 'Fable' });
    // claude is out of Fable, and its week resets first.
    const u = [usage('claude', 10, 40, 24, { meters: [...usage('claude', 10, 40, 24).meters, fable(100, 24)] }), usage('claude2', 20, 50)];
    expect(autoPick(ACCOUNTS, u, NOW, 'opus')).toEqual({ account: 'claude', reason: 'reset', resetsAt: NOW + 24 * HOUR });
    expect(autoPick(ACCOUNTS, u, NOW, 'opus[1m]')?.account).toBe('claude');
    expect(autoPick(ACCOUNTS, u, NOW, 'opusplan')?.account).toBe('claude');
    // On Fable, or a model that could be Fable, its Fable limit rules it out.
    expect(autoPick(ACCOUNTS, u, NOW, 'fable')?.account).toBe('claude2');
    expect(autoPick(ACCOUNTS, u, NOW, 'best')?.account).toBe('claude2');
    expect(autoPick(ACCOUNTS, u, NOW)?.account).toBe('claude2');
    // Without a model, each account's own default decides.
    const opusDefault = ACCOUNTS.map((a) => (a.name === 'claude' ? { ...a, defaultModel: 'claude-opus-5-5' } : a));
    expect(autoPick(opusDefault, u, NOW)?.account).toBe('claude');
  });

  it('asked for a model, considers the accounts whose command line takes it', () => {
    const other: StartAccount = { name: 'agy', harness: 'antigravity', places: [ghostty], chosen: null, unavailable: null, defaultModel: null, preferred: false, models: [{ id: 'gemini-pro', label: 'Gemini Pro' }] };
    // Another harness takes only the models it lists; Claude Code takes any model's name.
    expect(autoPick([other], [], NOW, 'gemini-pro')?.account).toBe('agy');
    expect(autoPick([other], [], NOW, 'opus')).toBeNull();
    expect(autoPick(ACCOUNTS, [], NOW, 'claude-opus-5-5')?.account).toBe('claude');
  });

  it('is only offered with more than one account that can start sessions', () => {
    const can = (a: StartAccount) => a;
    const cant = (a: StartAccount) => ({ ...a, places: [], unavailable: 'no' });
    const lone = [can(ACCOUNTS[0]), cant(ACCOUNTS[1]), cant(ACCOUNTS[2])];
    expect(choices(lone, pick([], lone))).toEqual(['claude', 'claude2', 'codex']);
    expect(choices(ACCOUNTS, pick([]))).toEqual([AUTO, 'claude', 'claude2', 'codex']);
  });

  it('opens where the account it picks can, and resolves to it when you start', () => {
    const p: AutoPick = { account: 'claude2', reason: 'room', resetsAt: null };
    expect(placesFor(AUTO, ACCOUNTS, p)).toEqual([ghostty]);
    expect(placesFor(AUTO, ACCOUNTS, { ...p, account: 'codex' })).toEqual([codexApp]);
    expect(placesFor('claude', ACCOUNTS, p)).toEqual([ghostty, claudeApp]);
    expect(placesFor(null, ACCOUNTS, p)).toEqual([]);
    expect(accountFor(AUTO, p)).toBe('claude2');
    expect(accountFor('codex', p)).toBe('codex');
    expect(accountFor(AUTO, null)).toBeNull();
  });
});

describe('the sheet opens with', () => {
  const p: AutoPick = { account: 'claude2', reason: 'room', resetsAt: null };

  it('the last choice while it is still offered, else the first one', () => {
    expect(initialChoice(ACCOUNTS, p, 'codex')).toBe('codex');
    expect(initialChoice(ACCOUNTS, p, 'claude3')).toBe(AUTO);
    expect(initialChoice(ACCOUNTS, null, undefined)).toBe('claude');
    const none: StartAccount[] = [fx.startAccount({ name: 'codex2', harness: 'codex', unavailable: 'no' })];
    expect(initialChoice(none, null, undefined)).toBeNull();
  });

  it('the place that choice last opened in, while it is still offered', () => {
    expect(initialPlace('claude', ACCOUNTS, p, 'claude-app')).toBe('claude-app');
    expect(initialPlace('claude2', ACCOUNTS, p, 'claude-app')).toBe('terminal');
    expect(initialPlace('codex', ACCOUNTS, p, undefined)).toBe('codex-app');
    expect(initialPlace(null, ACCOUNTS, p, undefined)).toBeNull();
  });

  it("the place an account's settings name, over the last one", () => {
    const set = ACCOUNTS.map((a) => (a.name === 'claude' ? { ...a, chosen: 'terminal' as const } : a));
    expect(initialPlace('claude', set, p, 'claude-app')).toBe('terminal');
    expect(settledPlace('claude', set, p)).toBe('terminal');
    // Auto goes where the account it picks names.
    expect(settledPlace(AUTO, set, p)).toBeNull();
    expect(settledPlace(AUTO, set, { ...p, account: 'claude' })).toBe('terminal');
  });

  it('the place a start there last opened in, kept as the start of its accounts that can go there', () => {
    expect(keepPlaceFor('claude', ACCOUNTS, 'claude-app')).toEqual(['claude']);
    expect(keepPlaceFor('claude2', ACCOUNTS, 'claude-app')).toEqual([]);
    // Auto's for every account it picks among, so whichever it picks next opens there too.
    expect(keepPlaceFor(AUTO, ACCOUNTS, 'terminal')).toEqual(['claude', 'claude2']);
    expect(keepPlaceFor(AUTO, ACCOUNTS, 'claude-app')).toEqual(['claude']);
    expect(keepPlaceFor(AUTO, star('codex'), 'codex-app')).toEqual(['codex']);
    expect(keepPlaceFor(null, ACCOUNTS, 'terminal')).toEqual([]);
    // And in the panel's copy, so the next open goes there before it reads them again.
    const kept = withChosen({ accounts: ACCOUNTS, codexProjects: [], claudeModels: [] }, ['claude', 'claude2'], 'terminal');
    expect(kept.accounts.map((a) => a.chosen)).toEqual(['terminal', 'terminal', null]);
    expect(settledPlace(AUTO, kept.accounts, p)).toBe('terminal');
  });
});

const MODELS = [
  { id: 'fable', label: 'Fable' },
  { id: 'fable[1m]', label: 'Fable (1M context)' },
  { id: 'opus', label: 'Opus' },
  { id: 'opusplan', label: 'Opus Plan Mode: Opus to plan, Sonnet to build' },
];

describe('the agent menu', () => {
  it('lists Auto, then each account on its default and, for Claude Code in the terminal, on each model', () => {
    expect(agentOptions(ACCOUNTS, MODELS, pick([])).map((o) => o.id)).toEqual([
      AUTO,
      'claude|',
      'claude|fable',
      'claude|fable[1m]',
      'claude|opus',
      'claude|opusplan',
      'claude2|',
      'claude2|fable',
      'claude2|fable[1m]',
      'claude2|opus',
      'claude2|opusplan',
      'codex|',
    ]);
    expect(takesModel(ACCOUNTS[2])).toBe(false);
    expect(takesModel({ ...ACCOUNTS[0], places: [claudeApp] })).toBe(false);
  });

  it('lists each account’s own models, and Claude Code’s for a Mac that sends only those', () => {
    const claudeModels = [{ id: 'opus', label: 'Opus' }];
    const claudeEfforts = [{ id: 'high', label: 'High' }];
    const agy: StartAccount = fx.startAccount({ name: 'antigravity', harness: 'antigravity', places: [ghostty], models: [{ id: 'gemini-3.8-flash-low', label: 'Gemini 3.8 Flash (Low)' }], efforts: [], worktree: false });
    // A Mac whose Codealong sends each account's lists: they're taken as sent.
    const sent = withLists({ accounts: [{ ...ACCOUNTS[0], models: claudeModels, efforts: claudeEfforts, worktree: true }, agy], codexProjects: [], claudeModels: [], claudeEfforts: [] });
    expect(sent.map((a) => [a.name, a.models?.map((m) => m.id), a.efforts?.length, a.worktree])).toEqual([['claude', ['opus'], 1, true], ['antigravity', ['gemini-3.8-flash-low'], 0, false]]);
    // An older one sends only Claude Code's, which go to its Claude Code accounts.
    const older = withLists({ accounts: [ACCOUNTS[0], ACCOUNTS[2]], codexProjects: [], claudeModels, claudeEfforts });
    expect(older.map((a) => [a.name, a.models?.length, a.efforts?.length, a.worktree])).toEqual([['claude', 1, 1, true], ['codex', 0, 0, false]]);
    // Any account that lists models takes one on the command line, and the menu offers its own.
    expect(takesModel(agy)).toBe(true);
    expect(takesModel({ ...agy, models: [] })).toBe(false);
    expect(agentOptions(sent, [], null).filter((o) => o.model).map((o) => `${o.choice}|${o.model}`)).toEqual(['claude|opus', 'antigravity|gemini-3.8-flash-low']);
  });

  it('reads an id back as the choice and model it stands for', () => {
    for (const [choice, model] of [
      [AUTO, null],
      ['claude2', null],
      ['claude2', 'fable[1m]'],
      ['my.acct-2', 'claude-opus-5-5'],
    ] as const) {
      expect(parseAgent(agentId(choice, model))).toEqual({ choice, model });
    }
  });

  it('names models the way you’d say them', () => {
    expect(modelName('fable[1m]', MODELS)).toBe('Fable 1M');
    expect(modelName('opusplan', MODELS)).toBe('Opus Plan Mode');
    expect(modelName('claude-fable-5-1', MODELS)).toBe('Fable 5.1');
    expect(modelName('claude-fable-5-1[1m]', MODELS)).toBe('Fable 5.1 1M');
    expect(modelName('claude-haiku-4-5-20251001', MODELS)).toBe('Haiku 4.5');
    expect(modelName('claude-sonnet-5', MODELS)).toBe('Sonnet 5');
    expect(modelName('sonnet[1m]', MODELS)).toBe('sonnet 1M');
    expect(modelName('gpt-5.5', MODELS)).toBe('gpt-5.5');
  });

  it('knows which per-model limit a model counts against', () => {
    expect(modelFamily('claude-fable-5-1[1m]')).toBe('fable');
    expect(modelFamily('opusplan')).toBe('opus');
    expect(modelFamily(null)).toBeNull();
    const u = usage('claude', 10, 20, 100, { meters: [meter('week', 20), { ...meter('model:Fable', 95), model: 'Fable' }] });
    expect(modelMeter(u, 'fable[1m]')?.key).toBe('model:Fable');
    expect(modelMeter(u, 'opus')).toBeNull();
  });
});

describe('Auto’s model', () => {
  const fable = (percent: number, opus?: number) =>
    usage('claude', 10, 20, 100, {
      meters: [meter('week', 20), { ...meter('model:Fable', percent), model: 'Fable' }, ...(opus === undefined ? [] : [{ ...meter('model:Opus', opus), model: 'Opus' }])],
    });

  it('is the account’s default while that model has room', () => {
    expect(autoModel('claude-fable-5-1', fable(40))).toEqual({ model: null, spent: null });
    expect(autoModel('opus[1m]', fable(99))).toEqual({ model: null, spent: null });
    expect(autoModel(null, fable(99))).toEqual({ model: null, spent: null });
  });

  it('moves to the best other model once the default’s own limit is nearly used, keeping its context size', () => {
    expect(autoModel('claude-fable-5-1', fable(95)).model).toBe('opus');
    expect(autoModel('claude-fable-5-1[1m]', fable(92)).model).toBe('opus[1m]');
    expect(autoModel('fable', fable(95)).spent?.key).toBe('model:Fable');
    // Opus nearly used too: Sonnet.
    expect(autoModel('fable', fable(95, 96)).model).toBe('sonnet');
  });
});

// The quick panel's words for a start, pinned as they read: a change to one shows here.
describe('what the quick panel says of a start', () => {
  const models = [
    { id: 'opus', label: 'Opus: the most capable' },
    { id: 'haiku', label: 'Haiku: the fastest' },
  ];
  const where = { repo: 'app', callsign: 'feature' };
  const plan = (over: Partial<StartPlan>): StartPlan => ({ lane: where, account: 'work', place: 'terminal', mac: null, reading: false, newWorktree: false, onModel: '', prompted: false, terminal: 'Ghostty', behind: false, ...over });

  it('says what each place does with the session', () => {
    expect(startHappens(plan({}))).toBe('Starts work in feature in Ghostty.');
    expect(startHappens(plan({ prompted: true, onModel: ' on Opus' }))).toBe('Starts work on Opus in feature in Ghostty.');
    expect(startHappens(plan({ newWorktree: true }))).toBe('Claude Code makes a new worktree of app and starts work there in Ghostty.');
    expect(startHappens(plan({ place: 'background' }))).toBe('Starts work in feature in the background. Attach a terminal any time.');
    expect(startHappens(plan({ place: 'background', newWorktree: true, prompted: true }))).toBe('Claude Code makes a new worktree of app and starts work there in the background. Attach a terminal any time.');
    expect(startHappens(plan({ place: 'background', behind: true, prompted: true }))).toBe('Starts work in feature in Ghostty, behind your windows.');
    expect(startHappens(plan({ place: 'codealong', prompted: true }))).toBe('Codealong runs work in feature. Its panel here shows the reply, takes the next prompt and answers what it asks.');
    expect(startHappens(plan({ place: 'claude-app' }))).toBe('The Claude app opens a new session in feature. You send it from there.');
    expect(startHappens(plan({ place: 'claude-app', prompted: true }))).toBe('The Claude app opens a new session in feature with this prompt filled in. You send it from there.');
    expect(startHappens(plan({ place: 'codex-app' }))).toBe('Codex opens a new thread in feature. You write the prompt there.');
    expect(startHappens(plan({ place: 'codex-app', prompted: true }))).toBe('The Codex app starts a new thread in feature on this prompt.');
  });

  it('says a chat starts on its own', () => {
    const chat = { repo: 'Chats', callsign: 'Chats', startsChats: true };
    expect(startHappens(plan({ lane: chat, prompted: true }))).toBe('Starts work in a folder of its own in Ghostty.');
    expect(startHappens(plan({ lane: chat, place: 'claude-app' }))).toBe('The Claude app opens a new session in a folder of its own. You send it from there.');
    expect(startTitleWords({ ...chat, origin: 'script' }, false)).toBe('Start a chat in no project');
    expect(startWhereWords({ ...chat, project: 'Elsewhere' })).toBe('No project');
    expect(startingLabel('background', 'work', '', chat, false, null)).toBe('Starting work in a chat of its own in the background…');
  });

  it('says why nothing can start, or nothing when it’s only the account or place missing', () => {
    expect(startHappens(plan({ lane: null }))).toBe('No worktrees in this project yet.');
    expect(startHappens(plan({ lane: null, reading: true }))).toBe('Reading worktrees…');
    expect(startHappens(plan({ lane: null, mac: { name: 'Studio', error: null } }))).toBe('No worktrees in this project on Studio.');
    expect(startHappens(plan({ lane: null, mac: { name: null, error: null } }))).toBe('No worktrees in this project on That Mac.');
    expect(startHappens(plan({ account: null }))).toBe('');
    expect(startHappens(plan({ place: null, mac: { name: 'Studio', error: null } }))).toBe('');
    expect(startHappens(plan({ place: null, mac: { name: 'Studio', error: 'it timed out' } }))).toBe('Studio didn’t say where its accounts can start: it timed out');
  });

  it('puts another Mac’s name first', () => {
    expect(startHappens(plan({ mac: { name: 'Studio', error: null } }))).toBe('On Studio: Starts work in feature in Ghostty.');
    expect(startHappens(plan({ mac: { name: null, error: null } }))).toBe('On That Mac: Starts work in feature in Ghostty.');
  });

  it('titles a start by where it starts, as Jev’s suggestions do', () => {
    expect(startTitleWords({ ...where, origin: 'main' }, false)).toBe('Start in feature (main)');
    expect(startTitleWords({ ...where, origin: 'script' }, false)).toBe('Start in feature');
    expect(startTitleWords({ ...where, origin: 'main' }, true)).toBe('Start in a new worktree of app');
    expect(startWhereWords({ ...where, project: 'Work' })).toBe('Work · app');
  });

  it('labels a start while it’s under way', () => {
    expect(startingLabel('terminal', 'work', ' on Opus', where, false, null)).toBe('Starting work on Opus in feature…');
    expect(startingLabel('background', 'work', '', where, true, 'Studio')).toBe('Starting work in a new worktree of app in the background on Studio…');
    expect(startingLabel('codealong', 'work', '', where, false, null)).toBe('Starting work in Codealong…');
    expect(startingLabel('claude-app', 'work', '', where, false, null)).toBe('Opening a new session in the Claude app…');
    expect(startingLabel('codex-app', 'work', '', where, false, 'Studio')).toBe('Opening a new thread in Codex on Studio…');
  });

  it('says the model and effort it starts on', () => {
    expect(onModelWords(null, null, models)).toBe('');
    expect(onModelWords('opus', null, models)).toBe(' on Opus');
    expect(onModelWords('opus', 'High', models)).toBe(' on Opus at high effort');
    expect(onModelWords(null, 'Extra', models)).toBe(' at extra effort');
  });

  it('says on the agent chip which model, or who picks it', () => {
    expect(modelWords(true, 'terminal', 'claude', 'haiku', 'opus', models)).toBe('Haiku');
    expect(modelWords(true, 'terminal', 'claude', null, 'opus', models)).toBe('Opus');
    expect(modelWords(true, 'terminal', 'claude', null, null, models)).toBe('default model');
    expect(modelWords(false, 'claude-app', 'claude', null, 'opus', models)).toBe('model chosen in the Claude app');
    expect(modelWords(false, 'codex-app', 'codex', null, null, models)).toBe('model chosen in Codex');
    expect(modelWords(false, 'terminal', 'opencode', null, null, models)).toBe('OpenCode’s model');
    expect(modelWords(false, 'terminal', 'codex', null, null, models)).toBe('Codex’s model');
    expect(modelWords(false, 'terminal', 'antigravity', null, null, models)).toBe('Antigravity’s model');
    expect(modelWords(false, 'terminal', 'claude', null, null, models)).toBeNull();
  });

  it('puts the account, Auto’s pick, the model and the effort on the agent chip', () => {
    const chip = (over: Partial<Parameters<typeof agentChipWords>[0]>) => agentChipWords({ auto: false, account: 'work', accounts: 2, remote: null, model: 'Opus', effortWord: null, ...over });
    expect(chip({})).toEqual({ label: 'work', detail: 'Opus' });
    expect(chip({ auto: true, effortWord: 'High' })).toEqual({ label: 'Auto', detail: 'work · Opus · High effort' });
    expect(chip({ account: null, model: null })).toEqual({ label: 'No account', detail: '' });
    expect(chip({ accounts: 0 }).label).toBe('No accounts');
    expect(chip({ accounts: 0, remote: { read: false, error: null } }).label).toBe('Reading accounts…');
    expect(chip({ accounts: 0, remote: { read: false, error: 'no answer' } }).label).toBe('Couldn’t read its accounts');
    expect(chip({ accounts: 0, remote: { read: true, error: null } }).label).toBe('No accounts');
  });

  it('says why Auto picks what it does', () => {
    const pick = (reason: AutoPick['reason'], resetsAt: number | null = null): AutoPick => ({ account: 'work', reason, resetsAt });
    expect(autoWhy(pick('room'), null, true, NOW)).toBe('preferred accounts are near their limits');
    expect(autoWhy(pick('room'), fx.meter({ label: 'Fable', percent: 91.4 }), false, NOW)).toBe('Fable’s week is 91% used');
    expect(autoWhy(pick('reset', NOW + DAY), null, false, NOW)).toBe(`week resets ${resetsPhrase(NOW + DAY, NOW)}`);
    expect(autoWhy(pick('reset'), null, false, NOW)).toBe('');
    expect(autoWhy(pick('room'), null, false, NOW)).toBe('most room');
    expect(autoWhy(pick('first'), null, false, NOW)).toBe('');
  });

  it('writes the agent menu’s lines', () => {
    const accounts = [fx.startAccount({ name: 'claude', places: [ghostty], defaultModel: 'opus', preferred: true }), fx.startAccount({ name: 'claude2', places: [ghostty] })];
    expect(autoOptionWords({ account: 'claude', reason: 'room', resetsAt: null }, accounts, [], models, NOW)).toEqual({ text: 'auto claude Claude Code opus', detail: 'claude · Opus · most room' });
    expect(autoOptionWords({ account: 'claude2', reason: 'first', resetsAt: null }, accounts, [], models, NOW)).toEqual({ text: 'auto claude2 Claude Code ', detail: 'claude2 · preferred accounts are near their limits' });

    expect(accountHeadWords(accounts[0], fx.usage({ account: 'claude', plan: 'Max' }), 'No usage yet')).toEqual({ detail: 'Claude Code · Max · Preferred', note: 'No usage yet' });
    expect(accountHeadWords(fx.startAccount({ name: 'off', harness: 'codex' }), undefined, 'No usage yet')).toEqual({ detail: 'Codex', note: 'Can’t start from here' });

    const claude = accounts[0];
    expect(agentOptionWords({ model: 'opus' }, claude, models)).toEqual({ text: 'claude Claude Code opus Opus', label: 'Opus', detail: 'the most capable' });
    expect(agentOptionWords({ model: 'opus[1m]' }, claude, models)).toEqual({ text: 'claude Claude Code opus[1m] opus 1M', label: 'opus 1M', detail: null });
    expect(agentOptionWords({ model: null }, claude, models)).toEqual({ text: 'claude Claude Code default', label: 'Default · Opus', detail: 'From its settings.json' });
    expect(agentOptionWords({ model: null }, fx.startAccount({ name: 'c2', places: [ghostty] }), models).label).toBe('Default');
    const codexCli = fx.startAccount({ name: 'codex', harness: 'codex', places: [ghostty], models: [{ id: 'gpt', label: 'GPT' }] });
    expect(agentOptionWords({ model: null }, codexCli, models)).toEqual({ text: 'codex Codex default', label: 'Default', detail: 'The one Codex starts on' });
    expect(agentOptionWords({ model: null }, fx.startAccount({ name: 'codex', harness: 'codex', places: [codexApp] }), models)).toEqual({ text: 'codex Codex default', label: 'codex', detail: 'The model is chosen in Codex' });
    expect(agentOptionWords({ model: null }, fx.startAccount({ name: 'oc', harness: 'opencode', places: [ghostty], models: [] }), models).detail).toBe('The model comes from OpenCode’s settings');
    expect(agentOptionWords({ model: null }, fx.startAccount({ name: 'ag', harness: 'antigravity', places: [ghostty], models: [] }), models).detail).toBeNull();
    expect(agentOptionWords({ model: null }, fx.startAccount({ name: 'out', harness: 'codex', unavailable: 'Not signed in' }), models)).toEqual({ text: 'out Codex default', label: 'out', detail: 'Not signed in' });
  });

  it('says beside the effort levels who picks, or that Haiku takes none', () => {
    expect(effortNote('codex', null)).toBe('Codex picks its own');
    expect(effortNote('claude', 'haiku')).toBe('Haiku takes none');
    expect(effortNote(null, 'claude-haiku-4-5')).toBe('Haiku takes none');
    expect(effortNote('claude', 'opus')).toBeNull();
  });

  it('says what the New worktree box does there', () => {
    expect(newWorktreeTitle(true, 'terminal', 'claude')).toBe('Claude Code makes the worktree: its folder, branch and base.');
    expect(newWorktreeTitle(true, null, null)).toBe('It makes the worktree: its folder, branch and base.');
    expect(newWorktreeTitle(false, 'background', 'codex')).toBe('Codex can’t start in a new worktree from here.');
    expect(newWorktreeTitle(false, 'claude-app', 'claude')).toBe('The app has its own worktree choice; pick it there before you send.');
    expect(newWorktreeTitle(false, null, null)).toBe('The app has its own worktree choice; pick it there before you send.');
  });

  it('knows which places start the command line, and which take a model', () => {
    expect((['terminal', 'background', 'codealong', 'claude-app', 'codex-app'] as const).map((p) => [onCommandLine(p), placeTakesModel(p)])).toEqual([
      [true, true],
      [true, true],
      [false, true],
      [false, false],
      [false, false],
    ]);
  });

  it('says each place under its name in the menu of places, and that the apps go on their own model', () => {
    expect(placeNote('terminal')).toBe('A new tab, in front');
    expect(placeNote('background')).toBe('No window; attach a terminal any time');
    expect(placeNote('background', true)).toBe('A tab behind your windows');
    expect(placeNote('codealong')).toBe('Its panel here takes your replies');
    for (const p of ['claude-app', 'codex-app'] as const) {
      expect(placeTakesModel(p)).toBe(false);
      expect(placeNote(p)).toBe('On the app’s own model');
    }
  });
});

describe('startChoice', () => {
  const models = [
    { id: 'opus', label: 'Opus: the most capable' },
    { id: 'haiku', label: 'Haiku: the fastest' },
  ];
  const efforts = [
    { id: 'high', label: 'High' },
    { id: 'max', label: 'Max' },
  ];
  const background = { place: 'background' as const, label: 'Background' };
  const codealong = { place: 'codealong' as const, label: 'Codealong' };
  const accounts = [
    fx.startAccount({ name: 'claude', places: [ghostty, background, claudeApp], defaultModel: 'opus', models, efforts, worktree: true }),
    fx.startAccount({ name: 'claude2', places: [ghostty, codealong], defaultModel: 'haiku', models, efforts, worktree: true }),
    fx.startAccount({ name: 'codex', harness: 'codex', places: [codexApp] }),
  ];
  const none: StartPicks = { agent: null, effort: null, place: null, newWorktree: false };
  const choose = (picks: Partial<StartPicks>, usage: Usage[] = [], lastPlace: (c: string) => 'terminal' | 'background' | undefined = () => undefined) => {
    const pick = autoPick(accounts, usage, NOW);
    return startChoice(accounts, models, efforts, usage, pick, agentOptions(accounts, models, pick), initialChoice(accounts, pick, undefined), { ...none, ...picks }, lastPlace);
  };

  it('starts with the first choice, Auto, on its account’s first place and own model', () => {
    const c = choose({});
    expect([c.agentNow, c.autoFor, c.account, c.placeNow, c.takesAModel, c.modelNow, c.startsOn, c.modelShown]).toEqual([AUTO, true, 'claude', 'terminal', true, null, 'opus', 'Opus']);
    expect(c.places.map((p) => p.place)).toEqual(['terminal', 'background', 'claude-app']);
  });

  it('goes back to the first choice when the one picked isn’t offered', () => {
    expect(choose({ agent: 'gone|' }).agentNow).toBe(AUTO);
  });

  it('keeps to the places that take a model once a model or an effort is picked', () => {
    const c = choose({ agent: agentId('claude', 'haiku') });
    expect([c.choice, c.chosenModel, c.autoFor, c.modelNow, c.startsOn, c.modelShown]).toEqual(['claude', 'haiku', false, 'haiku', 'haiku', 'Haiku']);
    expect(c.places.map((p) => p.place)).toEqual(['terminal', 'background']);
    expect(choose({ agent: agentId('claude', null), effort: 'high' }).places.map((p) => p.place)).toEqual(['terminal', 'background']);
    expect(choose({ agent: agentId('codex', null), effort: 'high' }).places.map((p) => p.place)).toEqual(['codex-app']);
  });

  it('opens where it was picked to while that’s offered, else where it last opened, else first', () => {
    expect(choose({ agent: agentId('claude', null), place: 'claude-app' }).placeNow).toBe('claude-app');
    expect(choose({ agent: agentId('claude', null) }, [], (c) => (c === 'claude' ? 'background' : undefined)).placeNow).toBe('background');
    expect(choose({ agent: agentId('claude2', null), place: 'claude-app' }).placeNow).toBe('terminal');
  });

  it('asks only where nothing settles it where it last opened', () => {
    const settled = accounts.map((a) => (a.name === 'claude' ? { ...a, chosen: 'claude-app' as const } : a));
    const asked: string[] = [];
    const pick = autoPick(settled, [], NOW);
    const c = startChoice(settled, models, efforts, [], pick, agentOptions(settled, models, pick), 'claude', { ...none, agent: agentId('claude', null) }, (ch) => {
      asked.push(ch);
      return 'background';
    });
    expect([c.placeNow, asked]).toEqual(['claude-app', []]);
  });

  it('gives the effort only where the model goes, and not to Haiku', () => {
    expect([choose({ effort: 'max' }).effortNow, choose({ effort: 'max' }).effortWord]).toEqual(['max', 'Max']);
    expect(choose({ agent: agentId('claude', null), place: 'claude-app', effort: 'max' }).placeNow).toBe('terminal');
    expect(choose({ agent: agentId('claude2', null), effort: 'max' })).toMatchObject({ startsOn: 'haiku', effortModel: 'haiku', effortNow: null, effortWord: null });
    expect(choose({ agent: agentId('codex', null), effort: 'max' })).toMatchObject({ takesAModel: false, effortNow: null, modelShown: 'model chosen in Codex', harness: 'codex' });
  });

  it('makes a new worktree only where the harness’s command line does', () => {
    expect(choose({ newWorktree: true })).toMatchObject({ makesWorktree: true, inNewWorktree: true });
    expect(choose({ agent: agentId('claude2', null), place: 'codealong', newWorktree: true })).toMatchObject({ placeNow: 'codealong', makesWorktree: false, inNewWorktree: false });
    expect(choose({ agent: agentId('codex', null), newWorktree: true })).toMatchObject({ makesWorktree: false, inNewWorktree: false });
  });

  it('starts Auto on the next model when its own is nearly used, and speaks of that one', () => {
    // Every account is near a limit, so Auto takes the least full, whose own model is nearly used.
    const fable = [fx.usage({ account: 'claude', meters: [fx.meter({ key: 'week', percent: 10 }), fx.meter({ key: 'model:opus', model: 'Opus', percent: 95 })] }), fx.usage({ account: 'claude2', meters: [fx.meter({ key: 'week', percent: 97 })] }), fx.usage({ account: 'codex', meters: [fx.meter({ key: 'week', percent: 99 })] })];
    const c = choose({}, fable);
    expect([c.account, c.modelNow, c.startsOn, c.effortModel]).toEqual(['claude', 'sonnet', 'sonnet', 'sonnet']);
  });
});

describe('resuming a session that isn’t running', () => {
  const models = [
    { id: 'opus', label: 'Opus' },
    { id: 'haiku', label: 'Haiku' },
  ];
  const efforts = [{ id: 'high', label: 'High' }];
  const background = { place: 'background' as const, label: 'Background' };
  const codealong = { place: 'codealong' as const, label: 'Codealong' };
  // Its `start` setting says new sessions start in the Claude app; that's not where one goes on.
  const claude = fx.startAccount({ name: 'claude', places: [claudeApp, ghostty, background, codealong], resumes: ['claude-app', 'terminal', 'background', 'codealong'], chosen: 'claude-app', defaultModel: 'opus', models, efforts });
  const none = { agent: null, effort: null, place: null };

  it('offers only where its sessions go on, without the account’s start setting', () => {
    const a = resumeAccount({ ...claude, resumes: ['terminal', 'codealong'] });
    expect([a.places.map((p) => p.place), a.chosen]).toEqual([['terminal', 'codealong'], null]);
    // A Mac from before resuming had a place offers none.
    expect(resumeAccount({ ...claude, resumes: undefined }).places).toEqual([]);
  });

  it('goes on in a terminal on its own model, unless it went on elsewhere last', () => {
    const c = resumeChoice(claude, [], none, undefined);
    expect([c.placeNow, c.modelNow, c.effortNow, c.modelShown]).toEqual(['terminal', null, null, 'Opus']);
    expect(c.places.map((p) => p.place)).toEqual(['claude-app', 'terminal', 'background', 'codealong']);
    expect(resumeChoice(claude, [], none, 'codealong').placeNow).toBe('codealong');
    expect(resumeIn(resumeChoice(claude, [], { ...none, place: 'background' }, 'codealong'))).toEqual({ place: 'background', model: null, effort: null });
  });

  it('asks for a model and effort as a start does: only where they go, so the app drops out', () => {
    const c = resumeChoice(claude, [], { agent: agentId('claude', 'haiku'), effort: 'high', place: null }, undefined);
    expect(c.places.map((p) => p.place)).toEqual(['terminal', 'background', 'codealong']);
    expect(resumeIn(c)).toEqual({ place: 'terminal', model: 'haiku', effort: null });
    // Haiku takes no effort; Opus does.
    expect(resumeIn(resumeChoice(claude, [], { agent: agentId('claude', 'opus'), effort: 'high', place: 'codealong' }, undefined))).toEqual({ place: 'codealong', model: 'opus', effort: 'high' });
    // The app picks its own.
    expect(resumeIn(resumeChoice(claude, [], { ...none, place: 'claude-app' }, undefined))).toEqual({ place: 'claude-app', model: null, effort: null });
  });

  it('says what a send to it does', () => {
    expect(resumeWords('terminal', 'Ghostty')).toBe('this resumes it in Ghostty');
    expect(resumeWords('background', 'Ghostty')).toBe('this resumes it in the background');
    expect(resumeWords('background', 'Ghostty', true)).toBe('this resumes it in Ghostty, behind your windows');
    expect(resumeWords('claude-app', 'Ghostty')).toBe('the Claude app opens it with this on the clipboard');
    expect(resumeWords('codealong', 'Ghostty')).toBe('Codealong runs it from now on');
  });

  it('remembers where each account’s sessions went on', () => {
    const items = new Map<string, string>();
    vi.stubGlobal('window', { localStorage: { getItem: (k: string) => items.get(k) ?? null, setItem: (k: string, v: string) => void items.set(k, v), removeItem: (k: string) => void items.delete(k) } });
    expect(resumedIn('claude')).toBeUndefined();
    rememberResumed('claude', 'background');
    rememberResumed('codex', 'codealong');
    expect([resumedIn('claude'), resumedIn('codex')]).toEqual(['background', 'codealong']);
    // The way it's written never changes, or what people chose is lost with an update.
    expect(items.get('codealong.resumed')).toBe('{"claude":"background","codex":"codealong"}');
    items.set('codealong.resumed', '{"claude":"moon"}');
    expect(resumedIn('claude')).toBeUndefined();
    vi.unstubAllGlobals();
  });
});
