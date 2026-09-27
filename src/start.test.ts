import { describe, expect, it } from 'vitest';
import { accountFor, autoChoice, agentId, agentOptions, autoModel, autoPicks, choices, initialChoice, initialPlace, modelFamily, modelMeter, modelName, parseAgent, placesFor, settledPlace, takesModel } from './start';
import type { Meter, StartAccount, Usage } from './types';

const ghostty = { place: 'terminal' as const, label: 'Ghostty' };
const claudeApp = { place: 'claude-app' as const, label: 'Claude app' };
const codexApp = { place: 'codex-app' as const, label: 'Codex app' };

const ACCOUNTS: StartAccount[] = [
  { name: 'claude', harness: 'claude', places: [ghostty, claudeApp], chosen: null, unavailable: null, defaultModel: null },
  { name: 'claude2', harness: 'claude', places: [ghostty], chosen: null, unavailable: null, defaultModel: null },
  { name: 'codex', harness: 'codex', places: [codexApp], chosen: null, unavailable: null, defaultModel: null },
];

const NOW = Date.UTC(2026, 8, 24, 12);
const HOUR = 3_600_000;

function meter(key: string, percent: number, resetsAt: number | null = null): Meter {
  return { key, label: key, window: key, percent, resetsAt, severity: percent >= 90 ? 'critical' : 'normal', model: null };
}

/** An account's 5-hour and weekly use, and when its week resets. */
function usage(account: string, session: number, week: number, weekResetsIn: number | null = 5 * 24, extra: Partial<Usage> = {}): Usage {
  return {
    account,
    harness: account.startsWith('codex') ? 'codex' : 'claude',
    plan: null,
    meters: [meter('session', session, NOW + 2 * HOUR), meter('week', week, weekResetsIn === null ? null : NOW + weekResetsIn * HOUR)],
    asOf: NOW,
    error: null,
    needsSignIn: false,
    loading: false,
    ...extra,
  };
}

const pick = (u: Usage[]) => autoPicks(ACCOUNTS, u, NOW).claude;

describe('Auto', () => {
  it('without a reset soon, starts the account whose fullest limit is least full', () => {
    // claude's fullest limit is 38%, claude2's is 45%: claude has more room, though its 5-hour is fuller.
    expect(pick([usage('claude', 38, 20), usage('claude2', 12, 45)])).toEqual({ account: 'claude', reason: 'room', resetsAt: null });
    expect(pick([usage('claude', 90, 10), usage('claude2', 10, 30)])?.account).toBe('claude2');
  });

  it('spends room first on an account whose weekly limit resets within 48 hours', () => {
    const u = [usage('claude', 10, 20), usage('claude2', 30, 70, 20)];
    expect(pick(u)).toEqual({ account: 'claude2', reason: 'reset', resetsAt: NOW + 20 * HOUR });
    // Past 48 hours a reset doesn't count.
    expect(pick([usage('claude', 10, 20), usage('claude2', 30, 70, 49)])?.account).toBe('claude');
    // A week that hasn't started has nothing to lose.
    expect(pick([usage('claude', 10, 20), usage('claude2', 30, 0, null)])?.account).toBe('claude');
  });

  it('takes the soonest reset when both reset within 48 hours', () => {
    expect(pick([usage('claude', 10, 20, 40), usage('claude2', 30, 70, 6)])).toEqual({ account: 'claude2', reason: 'reset', resetsAt: NOW + 6 * HOUR });
  });

  it('passes over an account near a limit while another has room', () => {
    // claude2's week resets soon, but its 5-hour window is nearly used up.
    expect(pick([usage('claude', 10, 20), usage('claude2', 95, 70, 6)])).toEqual({ account: 'claude', reason: 'room', resetsAt: null });
    // When every account is near a limit, the rules apply to all of them.
    expect(pick([usage('claude', 92, 20), usage('claude2', 95, 70, 6)])?.account).toBe('claude2');
  });

  it('passes over accounts without numbers unless none have them', () => {
    expect(pick([usage('claude', 0, 0, null, { meters: [], needsSignIn: true }), usage('claude2', 80, 20)])?.account).toBe('claude2');
    expect(pick([usage('claude', 20, 20, 3, { needsSignIn: true }), usage('claude2', 80, 20)])?.account).toBe('claude2');
    expect(pick([])).toEqual({ account: 'claude', reason: 'first', resetsAt: null });
  });

  it('counts only the limits the chosen model spends', () => {
    const fable = (percent: number, resetsIn: number): Meter => ({ ...meter('model:Fable', percent, NOW + resetsIn * HOUR), model: 'Fable' });
    // claude is out of Fable but has the most room otherwise, and its week resets in a day.
    const u = [usage('claude', 10, 40, 24, { meters: [...usage('claude', 10, 40, 24).meters, fable(100, 24)] }), usage('claude2', 20, 50)];
    expect(autoPicks(ACCOUNTS, u, NOW, 'opus').claude).toEqual({ account: 'claude', reason: 'reset', resetsAt: NOW + 24 * HOUR });
    expect(autoPicks(ACCOUNTS, u, NOW, 'opus[1m]').claude?.account).toBe('claude');
    expect(autoPicks(ACCOUNTS, u, NOW, 'opusplan').claude?.account).toBe('claude');
    // On Fable, or a model that could be Fable, its Fable limit rules it out.
    expect(autoPicks(ACCOUNTS, u, NOW, 'fable').claude?.account).toBe('claude2');
    expect(autoPicks(ACCOUNTS, u, NOW, 'best').claude?.account).toBe('claude2');
    expect(autoPicks(ACCOUNTS, u, NOW).claude?.account).toBe('claude2');
    // Without a model, each account's own default decides.
    const opusDefault = ACCOUNTS.map((a) => (a.name === 'claude' ? { ...a, defaultModel: 'claude-opus-5-5' } : a));
    expect(autoPicks(opusDefault, u, NOW).claude?.account).toBe('claude');
  });

  it('is only offered for a harness with more than one account that can start sessions', () => {
    const lone: StartAccount[] = [ACCOUNTS[0], { ...ACCOUNTS[1], places: [], chosen: null, unavailable: 'no', defaultModel: null }, ACCOUNTS[2]];
    expect(autoPicks(lone, [], NOW)).toEqual({});
    expect(choices(ACCOUNTS, autoPicks(ACCOUNTS, [], NOW))).toEqual([autoChoice('claude'), 'claude', 'claude2', 'codex']);
  });

  it('opens only where every account it could start can open', () => {
    expect(placesFor(autoChoice('claude'), ACCOUNTS)).toEqual([ghostty]);
    expect(placesFor('claude', ACCOUNTS)).toEqual([ghostty, claudeApp]);
    expect(placesFor('codex', ACCOUNTS)).toEqual([codexApp]);
    expect(placesFor(null, ACCOUNTS)).toEqual([]);
  });

  it('resolves to an account when you start', () => {
    const picks = { claude: { account: 'claude2', reason: 'room' as const, resetsAt: null } };
    expect(accountFor(autoChoice('claude'), picks)).toBe('claude2');
    expect(accountFor('codex', picks)).toBe('codex');
    expect(accountFor('auto:codex', picks)).toBeNull();
  });
});

describe('the sheet opens with', () => {
  const picks = { claude: { account: 'claude2', reason: 'room' as const, resetsAt: null } };

  it('the last choice while it is still offered, else the first one', () => {
    expect(initialChoice(ACCOUNTS, picks, 'codex')).toBe('codex');
    expect(initialChoice(ACCOUNTS, picks, 'claude3')).toBe(autoChoice('claude'));
    expect(initialChoice(ACCOUNTS, {}, undefined)).toBe('claude');
    const none: StartAccount[] = [{ name: 'codex2', harness: 'codex', places: [], chosen: null, unavailable: 'no', defaultModel: null }];
    expect(initialChoice(none, {}, undefined)).toBeNull();
  });

  it('the place that choice last opened in, while it is still offered', () => {
    expect(initialPlace('claude', ACCOUNTS, 'claude-app')).toBe('claude-app');
    expect(initialPlace('claude2', ACCOUNTS, 'claude-app')).toBe('terminal');
    expect(initialPlace('codex', ACCOUNTS, undefined)).toBe('codex-app');
    expect(initialPlace(null, ACCOUNTS, undefined)).toBeNull();
  });

  it("the place an account's settings name, over the last one", () => {
    const set = ACCOUNTS.map((a) => (a.name === 'claude' ? { ...a, chosen: 'terminal' as const } : a));
    expect(initialPlace('claude', set, 'claude-app')).toBe('terminal');
    expect(settledPlace('claude', set)).toBe('terminal');
    // Auto lands on either Claude account, and only one of them names a place.
    expect(settledPlace(autoChoice('claude'), set)).toBeNull();
    const both = set.map((a) => (a.harness === 'claude' ? { ...a, chosen: 'terminal' as const } : a));
    expect(settledPlace(autoChoice('claude'), both)).toBe('terminal');
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
    const picks = autoPicks(ACCOUNTS, [], NOW);
    expect(agentOptions(ACCOUNTS, MODELS, picks).map((o) => o.id)).toEqual([
      autoChoice('claude'),
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

  it('reads an id back as the choice and model it stands for', () => {
    for (const [choice, model] of [
      [autoChoice('claude'), null],
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
