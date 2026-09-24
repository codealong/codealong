import { describe, expect, it } from 'vitest';
import { accountFor, autoPicks, choices, initialChoice, initialPlace, placesFor } from './start';
import type { Meter, StartAccount, Usage } from './types';

const ghostty = { place: 'terminal' as const, label: 'Ghostty' };
const claudeApp = { place: 'claude-app' as const, label: 'Claude app' };
const codexApp = { place: 'codex-app' as const, label: 'Codex app' };

const ACCOUNTS: StartAccount[] = [
  { name: 'claude', harness: 'claude', places: [ghostty, claudeApp], unavailable: null },
  { name: 'claude2', harness: 'claude', places: [ghostty], unavailable: null },
  { name: 'codex', harness: 'codex', places: [codexApp], unavailable: null },
];

function meter(label: string, percent: number): Meter {
  return { key: label, label, window: label, percent, resetsAt: null, severity: 'normal', model: null };
}

function usage(account: string, percents: number[], extra: Partial<Usage> = {}): Usage {
  return { account, harness: account.startsWith('codex') ? 'codex' : 'claude', plan: null, meters: percents.map((p, i) => meter(`m${i}`, p)), asOf: 1, error: null, needsSignIn: false, loading: false, ...extra };
}

describe('Auto', () => {
  it('starts the account whose fullest limit is least full', () => {
    // claude's fullest limit is 38%, claude2's is 45%: claude has more room, though its 5-hour is fuller.
    const u = [usage('claude', [38, 20]), usage('claude2', [12, 45]), usage('codex', [5])];
    expect(autoPicks(ACCOUNTS, u)).toEqual({ claude: 'claude' });
    expect(autoPicks(ACCOUNTS, [usage('claude', [90]), usage('claude2', [10])])).toEqual({ claude: 'claude2' });
  });

  it('passes over accounts without numbers unless none have them', () => {
    expect(autoPicks(ACCOUNTS, [usage('claude', [], { needsSignIn: true }), usage('claude2', [80])])).toEqual({ claude: 'claude2' });
    expect(autoPicks(ACCOUNTS, [usage('claude', [70], { needsSignIn: true }), usage('claude2', [80])])).toEqual({ claude: 'claude2' });
    expect(autoPicks(ACCOUNTS, [])).toEqual({ claude: 'claude' });
  });

  it('is only offered for a harness with more than one account that can start sessions', () => {
    const lone: StartAccount[] = [ACCOUNTS[0], { ...ACCOUNTS[1], places: [], unavailable: 'no' }, ACCOUNTS[2]];
    expect(autoPicks(lone, [])).toEqual({});
    expect(choices(ACCOUNTS, autoPicks(ACCOUNTS, []))).toEqual(['auto:claude', 'claude', 'claude2', 'codex']);
  });

  it('opens only where every account it could start can open', () => {
    expect(placesFor('auto:claude', ACCOUNTS)).toEqual([ghostty]);
    expect(placesFor('claude', ACCOUNTS)).toEqual([ghostty, claudeApp]);
    expect(placesFor('codex', ACCOUNTS)).toEqual([codexApp]);
    expect(placesFor(null, ACCOUNTS)).toEqual([]);
  });

  it('resolves to an account when you start', () => {
    const picks = { claude: 'claude2' };
    expect(accountFor('auto:claude', picks)).toBe('claude2');
    expect(accountFor('codex', picks)).toBe('codex');
    expect(accountFor('auto:codex', picks)).toBeNull();
  });
});

describe('the sheet opens with', () => {
  const picks = { claude: 'claude2' };

  it('the last choice while it is still offered, else the first one', () => {
    expect(initialChoice(ACCOUNTS, picks, 'codex')).toBe('codex');
    expect(initialChoice(ACCOUNTS, picks, 'claude3')).toBe('auto:claude');
    expect(initialChoice(ACCOUNTS, {}, undefined)).toBe('claude');
    const none: StartAccount[] = [{ name: 'codex2', harness: 'codex', places: [], unavailable: 'no' }];
    expect(initialChoice(none, {}, undefined)).toBeNull();
  });

  it('the place that choice last opened in, while it is still offered', () => {
    expect(initialPlace('claude', ACCOUNTS, 'claude-app')).toBe('claude-app');
    expect(initialPlace('claude2', ACCOUNTS, 'claude-app')).toBe('terminal');
    expect(initialPlace('codex', ACCOUNTS, undefined)).toBe('codex-app');
    expect(initialPlace(null, ACCOUNTS, undefined)).toBeNull();
  });
});
