import { describe, expect, it } from 'vitest';
import { accountLimitFor, accountLimitPhrase, accountLimitSnoozeable, accountLimitSnoozeTitle, accountLimitWords } from './accountLimit';
import * as fx from './testing/fixtures';

const NOW = fx.NOW;
const MIN = 60_000;
const HOUR = 3_600_000;

describe('accountLimitFor', () => {
  it('returns a limit when a spent meter is at 100% and extra usage is not usable', () => {
    const s = fx.session({ account: 'claude-work', model: 'claude-opus-4-6', status: 'idle' });
    const usage = [
      fx.usage({
        account: 'claude-work',
        meters: [
          fx.meter({ key: 'session', label: '5h', percent: 45, resetsAt: NOW + 2 * HOUR }),
          fx.meter({ key: 'week', label: 'Week', percent: 100, resetsAt: NOW + 9 * HOUR, severity: 'critical' }),
        ],
      }),
    ];
    expect(accountLimitFor(s, [fx.lane({ sessions: [s] })], usage, NOW)).toEqual({
      account: 'claude-work',
      meter: 'week',
      resetsAt: NOW + 9 * HOUR,
      spendCapped: false,
      refused: false,
    });
  });

  it('picks the binding meter with the latest reset when multiple meters are at 100%', () => {
    const s = fx.session({ account: 'claude-work', model: 'claude-opus-4-6', status: 'unread' });
    const usage = [
      fx.usage({
        account: 'claude-work',
        extraUsable: false,
        meters: [
          fx.meter({ key: 'session', label: '5h', percent: 100, resetsAt: NOW + 2 * HOUR, severity: 'critical' }),
          fx.meter({ key: 'week', label: 'Week', percent: 100, resetsAt: NOW + 9 * HOUR, severity: 'critical' }),
        ],
      }),
    ];
    expect(accountLimitFor(s, [fx.lane({ sessions: [s] })], usage, NOW)).toEqual({
      account: 'claude-work',
      meter: 'week',
      resetsAt: NOW + 9 * HOUR,
      spendCapped: true,
      refused: false,
    });
  });

  it('does not trigger from a 100% meter when extra usage is usable', () => {
    const s = fx.session({ account: 'claude-work', model: 'claude-opus-4-6', status: 'idle' });
    const usage = [
      fx.usage({
        account: 'claude-work',
        extraUsable: true,
        meters: [fx.meter({ key: 'week', label: 'Week', percent: 100, resetsAt: NOW + 9 * HOUR, severity: 'critical' })],
      }),
    ];
    expect(accountLimitFor(s, [fx.lane({ sessions: [s] })], usage, NOW)).toBeNull();
  });

  it('ignores stale 100% meters whose reset time has already passed', () => {
    const s = fx.session({ account: 'claude-work', model: 'claude-opus-4-6', status: 'idle' });
    const usage = [
      fx.usage({
        account: 'claude-work',
        meters: [fx.meter({ key: 'week', label: 'Week', percent: 100, resetsAt: NOW - MIN, severity: 'critical' })],
      }),
    ];
    expect(accountLimitFor(s, [fx.lane({ sessions: [s] })], usage, NOW)).toBeNull();
  });

  it('skips sessions that already stopped at a limit, are working, or are archived', () => {
    const usage = [
      fx.usage({
        account: 'claude-work',
        meters: [fx.meter({ key: 'week', label: 'Week', percent: 100, resetsAt: NOW + 9 * HOUR, severity: 'critical' })],
      }),
    ];
    const stopped = fx.session({
      account: 'claude-work',
      status: 'error',
      limit: { meter: 'week', resetsAt: NOW + 9 * HOUR, at: NOW - MIN, moveTo: null },
    });
    const working = fx.session({ account: 'claude-work', status: 'working' });
    const archived = fx.session({ account: 'claude-work', status: 'idle', archived: true });
    expect(accountLimitFor(stopped, [], usage, NOW)).toBeNull();
    expect(accountLimitFor(working, [], usage, NOW)).toBeNull();
    expect(accountLimitFor(archived, [], usage, NOW)).toBeNull();
  });

  it('respects per-model limits on both meters and refusals', () => {
    const opusSession = fx.session({ account: 'claude', model: 'claude-opus-4-6', status: 'idle', lastActivity: NOW - 30 * MIN });
    const sonnetSession = fx.session({ account: 'claude', model: 'claude-sonnet-4-6', status: 'idle', lastActivity: NOW - 2 * MIN });
    const unknownModel = fx.session({ account: 'claude', model: undefined, status: 'idle', lastActivity: NOW - 30 * MIN });

    const meterUsage = [
      fx.usage({
        account: 'claude',
        meters: [
          fx.meter({ key: 'week', label: 'Week', percent: 40, resetsAt: NOW + 24 * HOUR }),
          fx.meter({ key: 'model:opus', label: 'Opus', percent: 100, resetsAt: NOW + 5 * HOUR, model: 'Opus', severity: 'critical' }),
        ],
      }),
    ];
    expect(accountLimitFor(opusSession, [], meterUsage, NOW)?.meter).toBe('model:Opus');
    expect(accountLimitFor(unknownModel, [], meterUsage, NOW)?.meter).toBe('model:Opus');
    expect(accountLimitFor(sonnetSession, [], meterUsage, NOW)).toBeNull();

    // Active refusal on another session for model:Opus (even when meters haven't refreshed yet)
    const refused = fx.session({
      key: 'claude:claude:other',
      account: 'claude',
      model: 'claude-opus-4-6',
      status: 'error',
      limit: { meter: 'model:Opus', resetsAt: NOW + 4 * HOUR, at: NOW - 5 * MIN, moveTo: null },
    });
    const lanes = [fx.lane({ sessions: [refused, opusSession, sonnetSession] })];
    expect(accountLimitFor(opusSession, lanes, [], NOW)).toEqual({
      account: 'claude',
      meter: 'model:Opus',
      resetsAt: NOW + 4 * HOUR,
      spendCapped: false,
      refused: true,
    });
    expect(accountLimitFor(sonnetSession, lanes, [], NOW)).toBeNull();
  });

  it('uses active refusals on another session and clears them after a succeeding turn or reset', () => {
    const target = fx.session({ key: 'claude:claude-work:s1', account: 'claude-work', model: 'claude-opus-4-6', status: 'idle', lastActivity: NOW - 30 * MIN });
    const refused = fx.session({
      key: 'claude:claude-work:s2',
      account: 'claude-work',
      model: 'claude-opus-4-6',
      status: 'error',
      lastActivity: NOW - 10 * MIN,
      limit: { meter: 'week', resetsAt: NOW + 6 * HOUR, spendCapped: true, at: NOW - 10 * MIN, moveTo: null },
    });
    // Active refusal beats even extraUsable: true when usage was read before the refusal.
    const olderUsage = [fx.usage({ account: 'claude-work', asOf: NOW - 15 * MIN, extraUsable: true, meters: [] })];
    expect(accountLimitFor(target, [fx.lane({ sessions: [target, refused] })], olderUsage, NOW)).toEqual({
      account: 'claude-work',
      meter: 'week',
      resetsAt: NOW + 6 * HOUR,
      spendCapped: true,
      refused: true,
    });

    // Cleared by a fresher usage read where extra usage is usable again.
    const fresherExtra = [fx.usage({ account: 'claude-work', asOf: NOW - MIN, extraUsable: true, meters: [] })];
    expect(accountLimitFor(target, [fx.lane({ sessions: [target, refused] })], fresherExtra, NOW)).toBeNull();

    // Cleared by a fresher usage read where the meter dropped back below 90% (reset used).
    const fresherReset = [
      fx.usage({
        account: 'claude-work',
        asOf: NOW - MIN,
        meters: [fx.meter({ key: 'week', label: 'Week', percent: 0, resetsAt: NOW + 6 * HOUR })],
      }),
    ];
    expect(accountLimitFor(target, [fx.lane({ sessions: [target, refused] })], fresherReset, NOW)).toBeNull();

    // Cleared by a later completed turn on the same account.
    const succeeded = fx.session({
      key: 'claude:claude-work:s3',
      account: 'claude-work',
      model: 'claude-sonnet-4-6',
      status: 'idle',
      lastActivity: NOW - 2 * MIN,
    });
    expect(accountLimitFor(target, [fx.lane({ sessions: [target, refused, succeeded] })], [], NOW)).toBeNull();
  });

  it('scopes refusals to the session’s own device', () => {
    const localTarget = fx.session({ key: 'claude:claude-work:local', account: 'claude-work', status: 'idle' });
    const remoteRefused = fx.session({
      key: '@mini:claude:claude-work:remote',
      account: 'claude-work',
      status: 'error',
      limit: { meter: 'week', resetsAt: NOW + 5 * HOUR, at: NOW - 5 * MIN, moveTo: null },
    });
    const remoteLane = fx.lane({
      id: '@mini:/r/app',
      device: { short: 'mini', name: 'Mac mini', color: 'purple', online: true, lastSeen: NOW },
      sessions: [remoteRefused],
    });
    expect(accountLimitFor(localTarget, [remoteLane], [], NOW, null)).toBeNull();

    const remoteTarget = fx.session({ key: '@mini:claude:claude-work:s2', account: 'claude-work', status: 'idle' });
    expect(accountLimitFor(remoteTarget, [remoteLane], [], NOW, 'mini')?.refused).toBe(true);
  });
});

describe('accountLimit copy helpers', () => {
  it('formats phrase, words, and snooze metadata', () => {
    const limit = { account: 'claude-work', meter: 'week', resetsAt: NOW + 9 * HOUR, spendCapped: false, refused: false };
    expect(accountLimitPhrase(limit, NOW)).toBe('claude-work at weekly limit · lifts in 9h');
    expect(accountLimitWords(limit, NOW, true)).toBe('claude-work is at its weekly limit. It lifts in 9h. Starting a new turn will hit it unless you move to another account.');
    expect(accountLimitWords({ ...limit, spendCapped: true }, NOW, false)).toBe('claude-work is at its weekly limit, with no extra usage to carry it. It lifts in 9h. Starting a new turn will hit it.');
    expect(accountLimitPhrase({ ...limit, meter: 'model:Opus', resetsAt: null }, NOW)).toBe('claude-work at weekly Opus limit');
    expect(accountLimitSnoozeable(limit, NOW)).toBe(true);
    expect(accountLimitSnoozeable({ ...limit, resetsAt: NOW + 2 * MIN }, NOW)).toBe(false);
    expect(accountLimitSnoozeTitle(NOW + 9 * HOUR, NOW)).toMatch(/^It leaves what waits on you until .+, when the limit lifts$/);
  });
});
