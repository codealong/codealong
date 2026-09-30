import { describe, expect, it } from 'vitest';
import { ageFiltered, bayOf, buildBays, bySession, closable, stoppable, filterByAge, homeOf, liveOrLatest, inboxCounts, narrowed, rowItems, sessionFlag, sessionHeadline, sessionPrs, withoutAccount } from './lanes';
import type { Lane, Pr, PrItem, Session } from './types';

const NOW = 1_790_200_000_000;
const DAY = 86_400_000;

const session = (key: string, status: Session['status'], lastActivity: number, lastPromptAt = lastActivity): Session =>
  ({ key, status, lastActivity, lastPromptAt, visitor: null, archived: false }) as Session;
const pr = (state: Pr['state'], failed = 0): Pr => ({ number: 7, title: 'Fix it', state, checks: { passed: 3, failed, pending: 0, skipped: 0, failing: [] } }) as unknown as Pr;
const lane = (sessions: Session[], p: Pr | null): Lane => {
  const [bay, glyph] = bayOf(sessions, p);
  return { id: '/r', sessions, pr: p, bay, glyph, headline: 'from the engine', detail: 'x' } as unknown as Lane;
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
    const done = { ...session('done', 'unread', NOW - 3_600_000), headline: '`claude` · Fix it', detail: 'Pushed the fix' } as Session;
    const l = ageFiltered(lane([stuck, done], pr('merged')), NOW - DAY, 'in the last 24 hours');
    expect([l.bay, l.headline, l.detail]).toEqual(['unread', '`claude` · Fix it', 'Pushed the fix']);
  });

  it('leaves a lane alone when nothing in it is filtered out', () => {
    const l = lane([stuck, mine], null);
    expect(ageFiltered(l, NOW - 4 * DAY, '')).toBe(l);
    expect(ageFiltered(l, 0, '')).toBe(l);
  });
});

describe('filterByAge', () => {
  const quiet = { ...lane([session('old', 'unread', NOW - 5 * DAY)], null), id: '/r/quiet', lastActivity: NOW - 5 * DAY } as Lane;
  const busy = { ...lane([session('mine', 'working', NOW - 60_000)], null), id: '/r/busy', lastActivity: NOW - 60_000 } as Lane;

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
    const busy = { ...lane([session('a', 'needs', NOW), session('b', 'working', NOW - 1000), session('c', 'unread', NOW), session('d', 'idle', NOW)], null), id: '/r/busy', lastActivity: NOW } as Lane;
    const quiet = { ...lane([session('e', 'idle', NOW)], pr('open')), id: '/r/quiet', lastActivity: NOW } as Lane;
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
    const one = { ...lane([old, done, recent], null), id: '/r/one', lastActivity: NOW } as Lane;
    expect(buildBays([one])[0].rows[0].agents.map((a) => a.session.key)).toEqual(['recent', 'old', 'done']);
    const a = { ...lane([session('x', 'working', NOW, NOW - 3_600_000)], null), id: '/r/a', lastActivity: NOW } as Lane;
    const b = { ...lane([session('y', 'working', NOW - 120_000, NOW - 1_800_000)], null), id: '/r/b', lastActivity: NOW - 120_000 } as Lane;
    expect(buildBays([a, b])[0].rows.map((r) => r.key)).toEqual(['/r/b', '/r/a']);
    expect([old, recent, done].sort(bySession).map((s) => s.key)).toEqual(['recent', 'old', 'done']);
  });

  it('lists Needs you as a queue, the longest wait first, as the engine does', () => {
    const waiting = (key: string, since: number, prompt: number, extra: Partial<Session> = {}) => ({ ...session(key, 'needs', since, prompt), statusSince: since, ...extra });
    // You prompted `fresh` last, but `waited` has waited longer.
    const waited = waiting('waited', NOW - 3_600_000, NOW - 4_000_000);
    const fresh = waiting('fresh', NOW - 60_000, NOW - 120_000);
    expect([fresh, waited].sort(bySession).map((s) => s.key)).toEqual(['waited', 'fresh']);
    const at = (id: string, sessions: Session[], p: Pr | null = null) => ({ ...lane(sessions, p), id, lastActivity: NOW }) as Lane;
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
  const mixed = { ...lane([session('w', 'working', NOW), session('u', 'unread', NOW), session('i', 'idle', NOW)], pr('open', 1)), id: '/r/mixed' } as Lane;

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
    const idle = { ...lane([session('i', 'idle', NOW)], pr('open')), id: '/r/idle' } as Lane;
    expect(narrowed(idle, ['needs', 'working'], '')).toBeNull();
    // A PR status keeps the worktree whole, whatever else is picked.
    expect(narrowed(idle, ['needs', 'review'], '')).toBe(idle);
  });

  it('keeps a worktree whole for its PR; Needs you is only about its agents', () => {
    expect(narrowed(mixed, ['work'], '')).toBe(mixed);
    expect(narrowed(mixed, ['review'], '')).toBeNull();
    expect(narrowed(mixed, ['needs'], '')).toBeNull();
    const failing = { ...lane([session('u', 'unread', NOW)], pr('open', 2)), id: '/r/failing' } as Lane;
    expect(failing.bay).toBe('unread');
    expect(narrowed(failing, ['needs'], '')).toBeNull();
    expect(narrowed(failing, ['work'], '')).toBe(failing);
    const changes = { ...lane([session('c', 'idle', NOW)], pr('changes')), id: '/r/changes' } as Lane;
    expect([narrowed(changes, ['review'], ''), narrowed(changes, ['work'], '')]).toEqual([changes, changes]);
    const inReview = { ...lane([session('c', 'idle', NOW)], pr('open')), id: '/r/review' } as Lane;
    expect(narrowed(inReview, ['review'], '')).toBe(inReview);
    expect(narrowed(inReview, ['work'], '')).toBeNull();
  });
});

describe('inboxCounts', () => {
  it('counts agents, and worktrees for what their PR needs', () => {
    const busy = { ...lane([session('a', 'needs', NOW), session('b', 'working', NOW), session('c', 'working', NOW), session('d', 'unread', NOW)], null), id: '/r/busy' } as Lane;
    const failing = { ...lane([session('e', 'unread', NOW)], pr('open', 2)), id: '/r/failing' } as Lane;
    const changes = { ...lane([], pr('changes')), id: '/r/changes' } as Lane;
    const inReview = { ...lane([], pr('open')), id: '/r/review' } as Lane;
    // Needs you counts agents alone; the PRs that need work count under PR needs work.
    expect(inboxCounts([busy, failing, changes, inReview])).toEqual({ needs: 1, working: 2, unread: 2, review: 2, work: 2 });
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
    expect(flag({ asks: true, excerpt: 'Asks you · Want the PR?' })).toBe('Asks you');
    // A question held through the plugin, which what you type doesn't answer.
    expect(flag({ excerpt: 'Asks you · Which color?' })).toBe('Asks you');
    expect(flag({ stalledSince: NOW - 3_600_000, excerpt: 'No activity for {for:1}' })).toBe('Stuck');
    expect(sessionFlag(s({ status: 'error', excerpt: 'API error' }))).toMatchObject({ label: 'Error', tone: 'bad' });
  });

  it('leaves out what the glyph and bay already say', () => {
    expect(flag({})).toBeNull();
    expect(flag({ harness: 'antigravity', excerpt: 'waiting' })).toBeNull();
    for (const status of ['working', 'unread', 'idle', 'stopped'] as const) expect(flag({ status, excerpt: 'Pushed the fix' })).toBeNull();
  });
});

describe('closable', () => {
  const live = (over: Partial<Session>): Session => ({ ...session('claude:c:1', 'idle', NOW), live: true, surface: 'cli', pid: 42, attachCommand: null, ...over }) as Session;

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
  const running = (over: Partial<Session>): Session => ({ ...session('claude:c:1', 'working', NOW), live: true, ...over }) as Session;

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
  const own = { ...session('k', 'idle', NOW), live: false } as Session;
  const visit = { ...own, visitor: { lane: '/r' } } as unknown as Session;
  const lanes = [{ id: '/other', sessions: [visit] }, { id: '/r', sessions: [own] }] as unknown as Lane[];

  it('finds a session where it runs, not where it visits', () => {
    expect(homeOf(lanes, 'k')?.lane.id).toBe('/r');
    expect(homeOf(lanes, 'gone')).toBeNull();
    expect(homeOf(lanes, null)).toBeNull();
  });

  it('picks the running session, else the latest', () => {
    const at = (key: string, live: boolean, t: number) => ({ session: { ...session(key, 'idle', t), live } as Session });
    expect(liveOrLatest([at('a', false, 3), at('b', true, 1), at('c', false, 2)])?.session.key).toBe('b');
    expect(liveOrLatest([at('a', false, 1), at('b', false, 3)])?.session.key).toBe('b');
    expect(liveOrLatest([])).toBeNull();
  });
});

describe('sessionPrs', () => {
  const s = { ...session('k', 'idle', NOW), branch: 'fix' } as Session;
  const item = (number: number, keys: string[]) => ({ number, url: `u/${number}`, sessions: keys.map((key) => ({ key, lane: '/r' })) }) as unknown as PrItem;
  const onBranch = (branch: string, p: Pr | null) => ({ ...lane([s], p), branch }) as Lane;

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
