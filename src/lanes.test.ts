import { describe, expect, it } from 'vitest';
import { ageFiltered, bayOf, branchLabel, buildBays, bySession, closable, filterByAge, homeOf, inboxCounts, limitWords, liveOrLatest, narrowed, relaned, rowItems, sessionFlag, sessionHeadline, sessionPrs, stoppable, withoutAccount, worktreeRow } from './lanes';
import * as fx from './testing/fixtures';
import type { Lane, Pr, PrItem, Session } from './types';

const NOW = 1_790_200_000_000;
const DAY = 86_400_000;

const session = (key: string, status: Session['status'], lastActivity: number, lastPromptAt = lastActivity): Session => fx.session({ key, status, lastActivity, lastPromptAt });
const pr = (state: Pr['state'], failed = 0): Pr => fx.pr({ state, checks: fx.checks({ passed: 3, failed }) });
const lane = (sessions: Session[], p: Pr | null): Lane => {
  const [bay, glyph] = bayOf(sessions, p);
  return fx.lane({ id: '/r', sessions, pr: p, bay, glyph, headline: 'from the engine', detail: 'x' });
};

describe('bayOf', () => {
  it('follows the engine: agents first, then the PR', () => {
    expect(bayOf([session('a', 'needs', NOW)], pr('open'))).toEqual(['needs', 'needs']);
    // A PR that needs work doesn't make the worktree need you.
    expect(bayOf([session('a', 'idle', NOW)], pr('open', 2))).toEqual(['review', 'idle']);
    expect(bayOf([], pr('changes'))).toEqual(['review', 'idle']);
    expect(bayOf([session('a', 'unread', NOW)], pr('open', 2))).toEqual(['unread', 'unread']);
    expect(bayOf([session('a', 'working', NOW)], pr('open', 2))).toEqual(['working', 'working']);
    expect(bayOf([], pr('merged'))).toEqual(['merged', 'idle']);
    expect(bayOf([], null)).toEqual(['idle', 'idle']);
  });
});

describe('ageFiltered', () => {
  const stuck = session('stuck', 'needs', NOW - 3 * DAY);
  const mine = session('mine', 'working', NOW - 60_000);

  it("drops old sessions and takes the lane's status from the rest", () => {
    const l = ageFiltered(lane([stuck, mine], null), NOW - DAY, 'in the last 24 hours');
    expect(l.sessions.map((s) => s.key)).toEqual(['mine']);
    expect([l.bay, l.glyph]).toEqual(['working', 'working']);
  });

  it('writes a plain headline when nothing active is left', () => {
    const l = ageFiltered(lane([stuck], pr('open')), NOW - DAY, 'in the last 24 hours');
    expect(l.bay).toBe('review');
    expect(l.headline).toBe('#7 Fix it');
    expect(ageFiltered(lane([stuck], null), NOW - DAY, 'in the last 24 hours').headline).toBe('No agents active in the last 24 hours');
  });

  it('takes the headline from the agent that sets the new status', () => {
    const done = { ...session('done', 'unread', NOW - 3_600_000), headline: '`claude` · Fix it', detail: 'Pushed the fix' };
    const l = ageFiltered(lane([stuck, done], pr('merged')), NOW - DAY, 'in the last 24 hours');
    expect([l.bay, l.headline, l.detail]).toEqual(['unread', '`claude` · Fix it', 'Pushed the fix']);
  });

  it('names an agent that is left when the one the engine named is filtered out, in the same bay', () => {
    // Two wait on you; the engine names the one that has waited longest, which is older than the cutoff.
    const waited = { ...session('waited', 'needs', NOW - 3 * DAY), headline: '`claude` asks you · Old question', detail: 'Which one?' };
    const asking = { ...session('asking', 'needs', NOW - 3_600_000), headline: '`codex` asks for approval · New work', detail: 'rm -rf build' };
    const l = { ...lane([waited, asking], null), headline: waited.headline, detail: waited.detail };
    const aged = ageFiltered(l, NOW - DAY, 'in the last 24 hours');
    expect(aged.sessions.map((s) => s.key)).toEqual(['asking']);
    expect([aged.bay, aged.glyph, aged.headline, aged.detail]).toEqual(['needs', 'needs', asking.headline, asking.detail]);
    // Still the same copy while the worktree and the cutoff are, so its row skips rendering.
    expect(ageFiltered(l, NOW - DAY, 'in the last 24 hours')).toBe(aged);
    // The one that's first in the engine's order, whatever order they come in.
    const fresh = { ...session('fresh', 'needs', NOW - 60_000), statusSince: NOW - 60_000, headline: '`claude` needs you · Newest', detail: null };
    const out = ageFiltered({ ...l, sessions: [waited, fresh, asking] }, NOW - DAY, 'in the last 24 hours');
    expect(out.headline).toBe(asking.headline);
  });

  it('leaves archived sessions out of the status, as the engine does', () => {
    const archived = { ...session('put-away', 'needs', NOW - 60_000), archived: true };
    const old = session('old', 'idle', NOW - 3 * DAY);
    const l = fx.lane({ id: '/r', sessions: [old, archived], pr: null, bay: 'idle', glyph: 'idle', headline: 'from the engine', detail: 'x' });
    const aged = ageFiltered(l, NOW - DAY, 'in the last 24 hours');
    expect([aged.bay, aged.glyph, aged.headline]).toEqual(['idle', 'idle', 'from the engine']);
  });

  it('leaves a lane alone when nothing in it is filtered out', () => {
    const l = lane([stuck, mine], null);
    expect(ageFiltered(l, NOW - 4 * DAY, '')).toBe(l);
    expect(ageFiltered(l, 0, '')).toBe(l);
  });

  it('gives back the same copy for the same lane and cutoff, so its row can skip rendering between pushes', () => {
    const l = lane([stuck, mine], null);
    const first = ageFiltered(l, NOW - DAY, 'in the last 24 hours');
    expect(ageFiltered(l, NOW - DAY, 'in the last 24 hours')).toBe(first);
    const later = ageFiltered(l, NOW - DAY + 15_000, 'in the last 24 hours');
    expect(later).not.toBe(first);
    expect(later.sessions.map((s) => s.key)).toEqual(['mine']);
    expect(ageFiltered({ ...l }, NOW - DAY, 'in the last 24 hours')).not.toBe(first);
  });
});

describe('filterByAge', () => {
  const quiet = { ...lane([session('old', 'unread', NOW - 5 * DAY)], null), id: '/r/quiet', lastActivity: NOW - 5 * DAY };
  const busy = { ...lane([session('mine', 'working', NOW - 60_000)], null), id: '/r/busy', lastActivity: NOW - 60_000 };

  it('keeps pinned worktrees however quiet, with only their recent sessions', () => {
    const shown = filterByAge([quiet, busy], NOW - DAY, new Set(['/r/quiet']), 'in the last 24 hours');
    expect(shown.map((l) => l.id)).toEqual(['/r/quiet', '/r/busy']);
    expect(shown[0].sessions).toEqual([]);
    expect([shown[0].bay, shown[0].headline]).toEqual(['idle', 'No agents active in the last 24 hours']);
  });

  it('drops quiet worktrees that are not pinned, and filters nothing without a cutoff', () => {
    expect(filterByAge([quiet, busy], NOW - DAY, new Set(), '').map((l) => l.id)).toEqual(['/r/busy']);
    const all = [quiet, busy];
    expect(filterByAge(all, 0, new Set(), '')).toBe(all);
  });
});

describe('buildBays', () => {
  it('shows each worktree once, in its bay, with a line for each agent that needs you, works or just finished', () => {
    const busy = { ...lane([session('a', 'needs', NOW), session('b', 'working', NOW - 1000), session('c', 'unread', NOW), session('d', 'idle', NOW)], null), id: '/r/busy', lastActivity: NOW };
    const quiet = { ...lane([session('e', 'idle', NOW)], pr('open')), id: '/r/quiet', lastActivity: NOW };
    const bays = buildBays([busy, quiet]);
    expect(bays.map((g) => [g.bay, g.rows.map((r) => r.key)])).toEqual([
      ['needs', ['/r/busy']],
      ['review', ['/r/quiet']],
    ]);
    expect(bays[0].rows[0].agents.map((a) => a.key)).toEqual(['/r/busy::a', '/r/busy::b', '/r/busy::c']);
    // What you can select: a worktree's agents, or the worktree when it has none.
    expect(bays.flatMap((g) => g.rows.flatMap(rowItems)).map((r) => r.key)).toEqual(['/r/busy::a', '/r/busy::b', '/r/busy::c', '/r/quiet']);
  });

  it('orders by status, then by when you last sent a prompt, not by activity', () => {
    // `old` is busier right now, but you asked `recent` more recently.
    const old = session('old', 'working', NOW, NOW - 3_600_000);
    const recent = session('recent', 'working', NOW - 300_000, NOW - 600_000);
    const done = session('done', 'unread', NOW - 60_000, NOW - 60_000);
    const one = { ...lane([old, done, recent], null), id: '/r/one', lastActivity: NOW };
    expect(buildBays([one])[0].rows[0].agents.map((a) => a.session.key)).toEqual(['recent', 'old', 'done']);
    const a = { ...lane([session('x', 'working', NOW, NOW - 3_600_000)], null), id: '/r/a', lastActivity: NOW };
    const b = { ...lane([session('y', 'working', NOW - 120_000, NOW - 1_800_000)], null), id: '/r/b', lastActivity: NOW - 120_000 };
    expect(buildBays([a, b])[0].rows.map((r) => r.key)).toEqual(['/r/b', '/r/a']);
    expect([old, recent, done].sort(bySession).map((s) => s.key)).toEqual(['recent', 'old', 'done']);
  });

  it('lists Needs you as a queue, the longest wait first, as the engine does', () => {
    const waiting = (key: string, since: number, prompt: number, extra: Partial<Session> = {}) => ({ ...session(key, 'needs', since, prompt), statusSince: since, ...extra });
    // You prompted `fresh` last, but `waited` has waited longer.
    const waited = waiting('waited', NOW - 3_600_000, NOW - 4_000_000);
    const fresh = waiting('fresh', NOW - 60_000, NOW - 120_000);
    expect([fresh, waited].sort(bySession).map((s) => s.key)).toEqual(['waited', 'fresh']);
    const at = (id: string, sessions: Session[], p: Pr | null = null) => ({ ...lane(sessions, p), id, lastActivity: NOW });
    const lanes = [
      at('/r/new', [fresh]),
      // Needs you only through a snoozed agent: after those waiting.
      at('/r/snoozed', [waiting('z', NOW - 7_200_000, NOW - 7_300_000, { snoozedUntil: NOW + 60_000 })]),
      at('/r/old', [session('b', 'working', NOW, NOW), waited]),
    ];
    expect(buildBays(lanes)[0].rows.map((r) => r.key)).toEqual(['/r/old', '/r/new', '/r/snoozed']);
  });
});

describe('narrowed', () => {
  const mixed = { ...lane([session('w', 'working', NOW), session('u', 'unread', NOW), session('i', 'idle', NOW)], pr('open', 1)), id: '/r/mixed' };

  it('narrows a worktree to the agents a filter is about, in that bay', () => {
    expect(mixed.bay).toBe('working');
    const yours = narrowed(mixed, ['unread'], '')!;
    expect([yours.bay, yours.sessions.map((s) => s.key)]).toEqual(['unread', ['u']]);
    expect(narrowed(mixed, ['working'], '')!.sessions.map((s) => s.key)).toEqual(['w']);
    expect(narrowed(mixed, [], '')).toBe(mixed);
  });

  it('narrows to the agents in any of several statuses, in the bay they put it in', () => {
    const both = narrowed(mixed, ['unread', 'working'], '')!;
    expect([both.bay, both.sessions.map((s) => s.key)]).toEqual(['working', ['w', 'u']]);
    const idle = { ...lane([session('i', 'idle', NOW)], pr('open')), id: '/r/idle' };
    expect(narrowed(idle, ['needs', 'working'], '')).toBeNull();
    // A PR status keeps the worktree whole, whatever else is picked.
    expect(narrowed(idle, ['needs', 'review'], '')).toBe(idle);
  });

  it('gives back the same copy for the same worktree and filters, so its row can skip rendering between pushes', () => {
    const first = narrowed(mixed, ['unread', 'working'], '')!;
    expect(narrowed(mixed, ['working', 'unread'], '')).toBe(first);
    expect(narrowed(mixed, ['working'], '')).not.toBe(first);
    expect(narrowed({ ...mixed }, ['unread', 'working'], '')).not.toBe(first);
  });

  it('keeps a worktree whole for its PR; Needs you is only about its agents', () => {
    expect(narrowed(mixed, ['work'], '')).toBe(mixed);
    expect(narrowed(mixed, ['review'], '')).toBeNull();
    expect(narrowed(mixed, ['needs'], '')).toBeNull();
    const failing = { ...lane([session('u', 'unread', NOW)], pr('open', 2)), id: '/r/failing' };
    expect(failing.bay).toBe('unread');
    expect(narrowed(failing, ['needs'], '')).toBeNull();
    expect(narrowed(failing, ['work'], '')).toBe(failing);
    const changes = { ...lane([session('c', 'idle', NOW)], pr('changes')), id: '/r/changes' };
    expect([narrowed(changes, ['review'], ''), narrowed(changes, ['work'], '')]).toEqual([changes, changes]);
    const inReview = { ...lane([session('c', 'idle', NOW)], pr('open')), id: '/r/review' };
    expect(narrowed(inReview, ['review'], '')).toBe(inReview);
    expect(narrowed(inReview, ['work'], '')).toBeNull();
  });
});

describe('inboxCounts', () => {
  it('counts agents, and worktrees for what their PR needs', () => {
    const busy = { ...lane([session('a', 'needs', NOW), session('b', 'working', NOW), session('c', 'working', NOW), session('d', 'unread', NOW)], null), id: '/r/busy' };
    const failing = { ...lane([session('e', 'unread', NOW)], pr('open', 2)), id: '/r/failing' };
    const changes = { ...lane([], pr('changes')), id: '/r/changes' };
    const inReview = { ...lane([], pr('open')), id: '/r/review' };
    // Needs you counts agents alone; the PRs that need work count under PR needs work.
    expect(inboxCounts([busy, failing, changes, inReview])).toEqual({ needs: 1, working: 2, unread: 2, review: 2, work: 2 });
  });
});

describe('a reusable worktree’s earlier work', () => {
  // A reply from before the worktree last took new work, never read; and one of the current work.
  const shipped = { ...session('shipped', 'unread', NOW - DAY), account: 'claude', title: 'Ship it', earlier: true };
  const current = { ...session('current', 'idle', NOW - 60_000), account: 'claude', title: 'Next thing' };
  const l = { ...lane([current], null), sessions: [current, shipped] };

  it('gets no line on the row, and stays out of its status and counts', () => {
    expect(worktreeRow(l).agents).toEqual([]);
    expect(relaned(l, l.sessions).bay).toBe('idle');
    expect(inboxCounts([l]).unread).toBe(0);
    expect(narrowed(l, ['unread'], '')).toBeNull();
  });
});

describe('an archived session', () => {
  // Archived while it waited on you: still listed, but out of its worktree's status (board.rs).
  const gone = { ...session('gone', 'needs', NOW), archived: true };
  const current = session('current', 'working', NOW - 60_000);
  const l = { ...lane([current], null), id: '/r/archived', sessions: [current, gone] };

  it('stays out of the status filters and their counts, as it does the row and the bay', () => {
    expect(worktreeRow(l).agents.map((a) => a.session.key)).toEqual(['current']);
    expect(relaned(l, l.sessions).bay).toBe('working');
    expect(inboxCounts([l])).toEqual({ needs: 0, working: 1, unread: 0, review: 0, work: 0 });
    expect(narrowed(l, ['needs'], '')).toBeNull();
    expect(narrowed(l, ['needs', 'working'], '')!.sessions.map((s) => s.key)).toEqual(['current']);
  });
});

describe('withoutAccount', () => {
  it('drops the account the chip already shows', () => {
    expect(withoutAccount('`claude` · Fix the test')).toBe('Fix the test');
    expect(withoutAccount('`claude2` is stuck · Fix the test')).toBe('is stuck · Fix the test');
    expect(withoutAccount('`codex` asks for approval · Fix the test')).toBe('asks for approval · Fix the test');
    expect(withoutAccount('Checks failing on #7')).toBe('Checks failing on #7');
  });
});

describe('sessionHeadline', () => {
  const base = session('a', 'needs', NOW);
  const s = (over: Partial<Session>): Session => ({ ...base, account: 'claude2', harness: 'claude', title: 'Fix the test', excerpt: null, ...over });

  it('names the session on every status, as the engine does', () => {
    expect(sessionHeadline(s({ excerpt: 'Bash · `rm -rf build`' }))[0]).toBe('`claude2` is waiting for approval · Fix the test');
    expect(sessionHeadline(s({ harness: 'codex', excerpt: 'Bash · `make`' }))[0]).toBe('`claude2` asks for approval · Fix the test');
    expect(sessionHeadline(s({ harness: 'antigravity', excerpt: 'Run command · `make`' }))[0]).toBe('`claude2` asks for approval · Fix the test');
    expect(sessionHeadline(s({}))).toEqual(['`claude2` needs you · Fix the test', null]);
    expect(sessionHeadline(s({ status: 'error', excerpt: 'API error' }))[0]).toBe('`claude2` stopped with an error · Fix the test');
  });

  it('leaves a finished turn its title alone, which its glyph and bay already say finished', () => {
    expect(sessionHeadline(s({ status: 'unread', excerpt: 'Pushed the fix' }))).toEqual(['`claude2` · Fix the test', 'Pushed the fix']);
  });

  it('says a session whose reply asks you something asks you, not that it waits for approval', () => {
    expect(sessionHeadline(s({ asks: true, excerpt: 'Asks you · Want the PR?' }))).toEqual(['`claude2` asks you · Fix the test', 'Want the PR?']);
    // A question held through the plugin, which what you type doesn't answer.
    expect(sessionHeadline(s({ asks: false, excerpt: 'Asks you · Which color?' }))).toEqual(['`claude2` asks you · Fix the test', 'Which color?']);
  });
});

describe('sessionFlag', () => {
  const base = session('a', 'needs', NOW);
  const s = (over: Partial<Session>): Session => ({ ...base, account: 'claude2', harness: 'claude', title: 'Fix the test', excerpt: null, ...over });
  const flag = (over: Partial<Session>) => sessionFlag(s(over))?.label ?? null;

  it('labels what a session waits on you for, which its headline said before its title', () => {
    expect(flag({ excerpt: 'Bash · `rm -rf build`' })).toBe('Needs approval');
    expect(flag({ harness: 'codex', excerpt: 'Run · `make`' })).toBe('Needs approval');
    expect(flag({ harness: 'opencode', excerpt: 'Bash · `npm install`' })).toBe('Needs approval');
    expect(flag({ harness: 'antigravity', excerpt: 'Run command · `rm -rf build`' })).toBe('Needs approval');
    expect(flag({ asks: true, excerpt: 'Asks you · Want the PR?' })).toBe('Asks you');
    // A question held through the plugin, which what you type doesn't answer.
    expect(flag({ excerpt: 'Asks you · Which color?' })).toBe('Asks you');
    expect(flag({ stalledSince: NOW - 3_600_000, excerpt: 'No activity for {for:1}' })).toBe('Stuck');
    expect(sessionFlag(s({ status: 'error', excerpt: 'API error' }))).toMatchObject({ label: 'Error', tone: 'bad' });
  });

  it('says which usage limit stopped a session, when it lifts and where it could go on', () => {
    const limit = { meter: 'week', resetsAt: NOW + 3 * 3_600_000, spendCapped: true, at: NOW - 60_000, moveTo: 'claude' };
    const stopped = s({ status: 'error', limit, excerpt: 'You’ve hit your monthly spend limit' });
    expect(sessionFlag(stopped)).toMatchObject({ label: 'Limit', tone: 'bad' });
    expect(sessionHeadline(stopped)).toEqual(['`claude2` hit its usage limit · Fix the test', 'You’ve hit your monthly spend limit']);
    expect(limitWords('claude2', limit, NOW)).toBe('It stopped at claude2’s weekly limit, with no extra usage to carry it. It lifts in 3h. claude has room: move it there to go on now.');
    // Beside a button that offers the move, the move goes unsaid.
    expect(limitWords('claude2', { ...limit, spendCapped: false, meter: 'session' }, NOW, true)).toBe('It stopped at claude2’s 5-hour limit. It lifts in 3h.');
    expect(limitWords('claude2', { ...limit, meter: 'model:Opus', spendCapped: false, resetsAt: null, moveTo: null }, NOW)).toBe('It stopped at claude2’s weekly Opus limit. No other account has room.');
    expect(limitWords('claude2', { ...limit, spendCapped: false, resetsAt: NOW - 1 }, NOW)).toBe('It stopped at claude2’s weekly limit. It has lifted since: send it something to go on.');
    // Seen, it rests with the limit still said; tried again, it's working and says nothing of it.
    expect(flag({ status: 'idle', limit })).toBe('Limit');
    expect(flag({ status: 'working', limit })).toBeNull();
  });

  it('leaves out what the glyph and bay already say', () => {
    expect(flag({})).toBeNull();
    for (const status of ['working', 'unread', 'idle', 'stopped'] as const) expect(flag({ status, excerpt: 'Pushed the fix' })).toBeNull();
  });
});

describe('closable', () => {
  const live = (over: Partial<Session>): Session => ({ ...session('claude:c:1', 'idle', NOW), live: true, surface: 'cli', pid: 42, attachCommand: null, ...over });

  it('offers Close for terminals and background jobs running here', () => {
    expect(closable(live({}))).toBe(true);
    expect(closable(live({ pid: null, attachCommand: 'claude attach 6e07c149' }))).toBe(true);
  });

  it('leaves out what has no terminal of its own, or runs elsewhere', () => {
    expect(closable(live({ live: false }))).toBe(false);
    expect(closable(live({ surface: 'app' }))).toBe(false);
    expect(closable(live({ hosted: true }))).toBe(false);
    expect(closable(live({ pid: null }))).toBe(false);
    expect(closable(live({ key: '@ab12:claude:c:1' }))).toBe(false);
  });
});

describe('stoppable', () => {
  const running = (over: Partial<Session>): Session => ({ ...session('claude:c:1', 'working', NOW), live: true, ...over });

  it('offers Stop where the engine says it can reach the turn', () => {
    expect(stoppable(running({ stoppable: true }))).toBe(true);
    expect(stoppable(running({}))).toBe(false);
  });

  it('still offers it for a working session Codealong runs on a Mac from before', () => {
    expect(stoppable(running({ hosted: true }))).toBe(true);
    expect(stoppable(running({ hosted: true, status: 'idle' }))).toBe(false);
  });
});

describe('homeOf and liveOrLatest', () => {
  const own = { ...session('k', 'idle', NOW), live: false };
  const visit = { ...own, visitor: 'worked here' };
  const lanes = [fx.lane({ id: '/other', sessions: [visit] }), fx.lane({ id: '/r', sessions: [own] })];

  it('finds a session where it runs, not where it visits', () => {
    expect(homeOf(lanes, 'k')?.lane.id).toBe('/r');
    expect(homeOf(lanes, 'gone')).toBeNull();
    expect(homeOf(lanes, null)).toBeNull();
  });

  it('picks the running session, else the latest', () => {
    const at = (key: string, live: boolean, t: number) => ({ session: { ...session(key, 'idle', t), live } });
    expect(liveOrLatest([at('a', false, 3), at('b', true, 1), at('c', false, 2)])?.session.key).toBe('b');
    expect(liveOrLatest([at('a', false, 1), at('b', false, 3)])?.session.key).toBe('b');
    expect(liveOrLatest([])).toBeNull();
  });
});

describe('sessionPrs', () => {
  const s = { ...session('k', 'idle', NOW), branch: 'fix' };
  const item = (number: number, keys: string[]): PrItem => fx.prItem({ number, url: `u/${number}`, sessions: keys.map((key) => ({ key, lane: '/r' })) });
  const onBranch = (branch: string, p: Pr | null) => ({ ...lane([s], p), branch });

  it('lists the PRs the session worked on, in the board’s order', () => {
    const prs = [item(9, ['other']), item(8, ['k']), item(5, ['other', 'k'])];
    expect(sessionPrs(prs, onBranch('fix', pr('open')), s).map((p) => p.number)).toEqual([8, 5]);
  });

  it('falls back to its worktree’s PR on the same branch, and only then', () => {
    expect(sessionPrs([item(9, ['other'])], onBranch('fix', pr('open')), s).map((p) => p.number)).toEqual([7]);
    expect(sessionPrs([], onBranch('main', pr('open')), s)).toEqual([]);
    expect(sessionPrs([], onBranch('fix', null), s)).toEqual([]);
    expect(sessionPrs([], onBranch('fix', pr('open')), { ...s, branch: null }).map((p) => p.number)).toEqual([7]);
  });
});

describe('branchLabel', () => {
  it('is the branch, or the commit a detached HEAD is at', () => {
    expect(branchLabel({ branch: 'fix-build', detached: null })).toBe('fix-build');
    expect(branchLabel({ branch: null, detached: '1a2b3c4' })).toBe('detached @ 1a2b3c4');
    // The branch wins where git gave both.
    expect(branchLabel({ branch: 'main', detached: '1a2b3c4' })).toBe('main');
  });

  it('is nothing until git has said', () => {
    expect(branchLabel({ branch: null, detached: null })).toBeNull();
  });
});
