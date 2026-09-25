import { describe, expect, it } from 'vitest';
import { accountFor, autoPicks, choices, initialChoice, initialPlace, placesFor } from './start';
import type { Meter, StartAccount, Usage } from './types';

const ghostty = { place: 'terminal' as const, label: 'Ghostty' };
const claudeApp = { place: 'claude-app' as const, label: 'Claude app' };
const codexApp = { place: 'codex-app' as const, label: 'Codex app' };

const ACCOUNTS: StartAccount[] = [
  { name: 'claude', harness: 'claude', places: [ghostty, claudeApp], unavailable: null, defaultModel: null },
  { name: 'claude2', harness: 'claude', places: [ghostty], unavailable: null, defaultModel: null },
  { name: 'codex', harness: 'codex', places: [codexApp], unavailable: null, defaultModel: null },
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
    const lone: StartAccount[] = [ACCOUNTS[0], { ...ACCOUNTS[1], places: [], unavailable: 'no', defaultModel: null }, ACCOUNTS[2]];
    expect(autoPicks(lone, [], NOW)).toEqual({});
    expect(choices(ACCOUNTS, autoPicks(ACCOUNTS, [], NOW))).toEqual(['auto:claude', 'claude', 'claude2', 'codex']);
  });

  it('opens only where every account it could start can open', () => {
    expect(placesFor('auto:claude', ACCOUNTS)).toEqual([ghostty]);
    expect(placesFor('claude', ACCOUNTS)).toEqual([ghostty, claudeApp]);
    expect(placesFor('codex', ACCOUNTS)).toEqual([codexApp]);
    expect(placesFor(null, ACCOUNTS)).toEqual([]);
  });

  it('resolves to an account when you start', () => {
    const picks = { claude: { account: 'claude2', reason: 'room' as const, resetsAt: null } };
    expect(accountFor('auto:claude', picks)).toBe('claude2');
    expect(accountFor('codex', picks)).toBe('codex');
    expect(accountFor('auto:codex', picks)).toBeNull();
  });
});

describe('the sheet opens with', () => {
  const picks = { claude: { account: 'claude2', reason: 'room' as const, resetsAt: null } };

  it('the last choice while it is still offered, else the first one', () => {
    expect(initialChoice(ACCOUNTS, picks, 'codex')).toBe('codex');
    expect(initialChoice(ACCOUNTS, picks, 'claude3')).toBe('auto:claude');
    expect(initialChoice(ACCOUNTS, {}, undefined)).toBe('claude');
    const none: StartAccount[] = [{ name: 'codex2', harness: 'codex', places: [], unavailable: 'no', defaultModel: null }];
    expect(initialChoice(none, {}, undefined)).toBeNull();
  });

  it('the place that choice last opened in, while it is still offered', () => {
    expect(initialPlace('claude', ACCOUNTS, 'claude-app')).toBe('claude-app');
    expect(initialPlace('claude2', ACCOUNTS, 'claude-app')).toBe('terminal');
    expect(initialPlace('codex', ACCOUNTS, undefined)).toBe('codex-app');
    expect(initialPlace(null, ACCOUNTS, undefined)).toBeNull();
  });
});
