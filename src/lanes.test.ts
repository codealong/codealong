import { describe, expect, it } from 'vitest';
import { ageFiltered, bayOf, branchLabel, buildBays, bySession, closable, connectionLostWords, cutOffWords, exitsItsOwnWay, filterByAge, foldsOf, foldWords, helperLine, helperName, helperStands, homeOf, inboxCounts, laneWaits, limitWords, liveOrLatest, narrowed, relaned, rowItems, runsShownKey, seenable, sessionFlag, sessionHeadline, sessionPrs, sidesListed, sidesShownKey, stoppable, unattended, waitsInBackground, withoutAccount, workingFor, worktreeRow } from './lanes';
import * as fx from './testing/fixtures';
import type { Lane, Pr, PrItem, Session, SideGlance } from './types';

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
    expect(bays[0].rows[0].lines.map((a) => a.key)).toEqual(['/r/busy::a', '/r/busy::b', '/r/busy::c']);
    // What you can select: a worktree's lines, and with none of its agents in those states, the
    // one that last worked there, so every line is a session.
    expect(bays.flatMap((g) => g.rows.flatMap(rowItems)).map((r) => r.key)).toEqual(['/r/busy::a', '/r/busy::b', '/r/busy::c', '/r/quiet::e']);
  });

  it('gives a worktree with no agent at work a line for the one that last worked there', () => {
    const older = { ...session('older', 'idle', NOW - DAY), account: 'claude' };
    const latest = { ...session('latest', 'stopped', NOW - 60_000), account: 'claude2' };
    const gone = { ...session('gone', 'idle', NOW), archived: true };
    expect(worktreeRow({ ...lane([older, latest, gone], null), id: '/r/idle' }).lines.map((a) => a.key)).toEqual(['/r/idle::latest']);
    // With none of its own, the one that made it or worked in it.
    const guest = { ...session('guest', 'idle', NOW - DAY), visitor: 'made this worktree' };
    expect(worktreeRow({ ...lane([guest], null), id: '/r/made' }).lines.map((a) => a.session.key)).toEqual(['guest']);
    // No agent at all: the worktree is what you select.
    const empty = worktreeRow({ ...lane([], null), id: '/r/empty' });
    expect([empty.lines, rowItems(empty).map((r) => r.key)]).toEqual([[], ['/r/empty']]);
  });

  it('orders by status, then by when you last sent a prompt, not by activity', () => {
    // `old` is busier right now, but you asked `recent` more recently.
    const old = session('old', 'working', NOW, NOW - 3_600_000);
    const recent = session('recent', 'working', NOW - 300_000, NOW - 600_000);
    const done = session('done', 'unread', NOW - 60_000, NOW - 60_000);
    const one = { ...lane([old, done, recent], null), id: '/r/one', lastActivity: NOW };
    expect(buildBays([one])[0].rows[0].lines.map((a) => a.session.key)).toEqual(['recent', 'old', 'done']);
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
    expect(worktreeRow(l).lines.map((a) => a.session.key)).toEqual(['current']);
    expect(relaned(l, l.sessions).bay).toBe('idle');
    expect(inboxCounts([l]).unread).toBe(0);
    expect(narrowed(l, ['unread'], '')).toBeNull();
  });
});

describe('a helper', () => {
  const boss = session('boss', 'idle', NOW - 60_000);
  const helper = (status: Session['status']) => ({ ...session('rev', status, NOW, NOW), helper: { parent: 'boss' } });

  it('stays out of its worktree’s row, status and counts while it doesn’t need you, as board.rs has it', () => {
    const busy = helper('working');
    const l = { ...lane([boss], null), sessions: [boss, busy] };
    expect(unattended(busy)).toBe(true);
    expect(worktreeRow(l).lines.map((a) => a.session.key)).toEqual(['boss']);
    expect(relaned(l, l.sessions).bay).toBe('idle');
    expect(inboxCounts([l]).working).toBe(0);
    expect(narrowed(l, ['working'], '')).toBeNull();
  });

  it('has a line in its session’s chip’s tooltip saying where it stands since when', () => {
    const g = (status: Session['status'], review: string | null = null) => ({ key: 'k', title: 'Review the diff', status, since: NOW - 3 * 60_000, review });
    expect(helperLine(g('working'), NOW)).toBe('Review the diff · working for 3m');
    expect(helperLine(g('needs'), NOW)).toBe('Review the diff · waits on you, 3m ago');
    expect(helperLine(g('error'), NOW)).toBe('Review the diff · stopped with an error 3m ago');
    expect(helperLine(g('idle', '#204'), NOW)).toBe('Review of #204 · done 3m ago');
    expect(helperLine(g('unread'), NOW)).toBe('Review the diff · done 3m ago');
    // Its card says each part on a line of its own.
    expect([helperName(g('idle', '#204')), helperStands(g('working'), NOW)]).toEqual(['Review of #204', 'working for 3m']);
  });

  it('is in Needs you like any session once it waits on you', () => {
    for (const st of ['needs', 'error'] as const) {
      const asks = helper(st);
      const l = { ...lane([boss], null), sessions: [boss, asks] };
      expect(unattended(asks)).toBe(false);
      expect(relaned(l, l.sessions).bay).toBe('needs');
      expect(worktreeRow(l).lines.map((a) => a.session.key)).toEqual(['rev']);
      expect(inboxCounts([l]).needs).toBe(1);
    }
  });
});

describe('a headless run', () => {
  const boss = session('boss', 'idle', NOW - 60_000);
  const run = (status: Session['status']) => ({ ...session('run', status, NOW, NOW), headless: true, origin: { key: 'boss', how: 'started' as const } });

  it('stays out of its worktree’s row, status and counts while it doesn’t need you, and works for the session it came from', () => {
    for (const st of ['working', 'idle'] as const) {
      const r = run(st);
      const l = { ...lane([boss], null), sessions: [boss, r] };
      expect(unattended(r)).toBe(true);
      expect(worktreeRow(l).lines.map((a) => a.session.key)).toEqual(['boss']);
      expect(relaned(l, l.sessions).bay).toBe('idle');
      expect(inboxCounts([l]).working).toBe(0);
    }
    expect(workingFor(run('working'))).toBe('boss');
    // Where it came from goes for a run alone: a session you forked isn't working for another.
    expect(workingFor({ ...run('working'), headless: false })).toBeUndefined();
  });

  it('is in Needs you like any session once it waits on a call to allow', () => {
    const asks = run('needs');
    const l = { ...lane([boss], null), sessions: [boss, asks] };
    expect(unattended(asks)).toBe(false);
    expect(relaned(l, l.sessions).bay).toBe('needs');
    expect(inboxCounts([l]).needs).toBe(1);
  });
});

describe('an archived session', () => {
  // Archived while it waited on you: still listed, but out of its worktree's status (board.rs).
  const gone = { ...session('gone', 'needs', NOW), archived: true };
  const current = session('current', 'working', NOW - 60_000);
  const l = { ...lane([current], null), id: '/r/archived', sessions: [current, gone] };

  it('stays out of the status filters and their counts, as it does the row and the bay', () => {
    expect(worktreeRow(l).lines.map((a) => a.session.key)).toEqual(['current']);
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

  it('says a session holding another session’s message holds it, and whose', () => {
    expect(sessionHeadline(s({ excerpt: 'Held message from “Review 1021”' }))).toEqual(['`claude2` holds a message for you · Fix the test', 'from “Review 1021”']);
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
    expect(flag({ excerpt: 'Held message from “Review 1021”' })).toBe('Message');
    expect(flag({ stalledSince: NOW - 3_600_000, excerpt: 'No activity for {for:1}' })).toBe('Stuck');
    expect(sessionFlag(s({ status: 'error', excerpt: 'API error' }))).toMatchObject({ label: 'Error', tone: 'bad' });
  });

  it('says a lost connection ended its turn, and whether Codealong carries it on', () => {
    const lost = s({ status: 'unread', excerpt: "API Error: Can't reach the API server — check your internet or DNS (ENOTFOUND)", connectionLost: { at: NOW - 60_000, continues: true } });
    expect(sessionFlag(lost)).toMatchObject({ label: 'Network error', tone: 'warn' });
    expect(connectionLostWords({ at: NOW, continues: true })).toBe('Its last turn ended because it couldn’t reach its API: the Mac slept, or the network dropped. Codealong tells it to carry on once the API can be reached again.');
    expect(connectionLostWords({ at: NOW })).toBe('Its last turn ended because it couldn’t reach its API: the Mac slept, or the network dropped. Send it something to carry on.');
    // Carried on, it's working: nothing to flag.
    expect(sessionFlag({ ...lost, status: 'working', excerpt: null })).toBeNull();
  });

  it('says a turn was cut off, as news until seen and as a flag after', () => {
    const cut = s({ status: 'error', cutOff: NOW - 60_000, excerpt: 'running `yarn test`' });
    expect(sessionFlag(cut)).toMatchObject({ label: 'Cut off', tone: 'bad' });
    expect(sessionHeadline(cut)).toEqual(['`claude2` was cut off · Fix the test', 'running `yarn test`']);
    expect(sessionFlag({ ...cut, status: 'stopped' })).toMatchObject({ label: 'Cut off' });
    expect(cutOffWords()).toBe('Its last turn was cut off partway: what ran it ended (its terminal closed, Codealong or the Mac restarted, or it was killed). Resume it, or send it something, to carry on.');
    expect(cutOffWords(true)).toBe('Its last turn was cut off partway: what ran it ended (its terminal closed, Codealong or the Mac restarted, or it was killed).');
    // Resumed, it's working: nothing to flag.
    expect(sessionFlag({ ...cut, status: 'working' })).toBeNull();
    // Read and left as it is, it's marked seen as a finished turn is; another error isn't.
    expect(seenable(cut)).toBe(true);
    expect(seenable({ ...cut, cutOff: undefined })).toBe(false);
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

describe('exitsItsOwnWay', () => {
  // A Claude Code session started with --worktree, idle at its prompt in a terminal here.
  const own = fx.session({ live: true, pid: 42, ownWorktree: '/r/app/.claude/worktrees/brave-otter' });

  it('is a running terminal session holding its own worktree, with nothing under way', () => {
    expect(exitsItsOwnWay(own)).toBe(true);
    expect(exitsItsOwnWay({ ...own, status: 'needs', asks: true })).toBe(true);
    expect(exitsItsOwnWay({ ...own, ownWorktree: undefined })).toBe(false);
  });

  it('is not while what it types would be taken for something else, as idle.rs has it', () => {
    expect(exitsItsOwnWay({ ...own, status: 'working' })).toBe(false);
    expect(exitsItsOwnWay({ ...own, background: 'npm run dev' })).toBe(false);
    expect(exitsItsOwnWay({ ...own, status: 'needs' })).toBe(false);
  });

  it('is not for one without a terminal of its own here', () => {
    expect(exitsItsOwnWay({ ...own, attachCommand: 'claude attach 5' })).toBe(false);
    expect(exitsItsOwnWay({ ...own, surface: 'app' })).toBe(false);
    expect(exitsItsOwnWay({ ...own, live: false })).toBe(false);
    expect(exitsItsOwnWay({ ...own, key: '@mini:claude:claude:s1' })).toBe(false);
  });
});

describe('waitsInBackground', () => {
  // A turn that ended with `gh pr checks --watch` left running waits on it, and is drawn so.
  const waiting = fx.session({ status: 'working', background: 'gh pr checks 1151 --watch' });
  const busy = fx.session({ key: 'b', status: 'working' });

  it('is a working session with a command left running in the background', () => {
    expect(waitsInBackground(waiting)).toBe(true);
    expect(waitsInBackground({ ...waiting, background: '' })).toBe(true);
    expect(waitsInBackground(busy)).toBe(false);
    expect(waitsInBackground({ ...waiting, status: 'idle' })).toBe(false);
  });

  it('is a worktree whose working sessions all wait', () => {
    expect(laneWaits(fx.lane({ glyph: 'working', sessions: [waiting] }))).toBe(true);
    expect(laneWaits(fx.lane({ glyph: 'working', sessions: [waiting, busy] }))).toBe(false);
    expect(laneWaits(fx.lane({ glyph: 'needs', sessions: [waiting] }))).toBe(false);
  });
});

describe('a session’s side conversations under it', () => {
  const glance = (id: string, running: boolean): SideGlance => ({ id, question: id, asked: 1, running, open: running, startedAt: NOW, lastAt: NOW });
  const list = [glance('answering', true), glance('answered', false)];

  it('lists the one answering, as a helper at work is, and every one where you asked', () => {
    expect(sidesListed(list, 'a', new Set()).map((g) => g.id)).toEqual(['answering']);
    expect(sidesListed(list, 'a', new Set([sidesShownKey('a')])).map((g) => g.id)).toEqual(['answering', 'answered']);
    // Asking for another session's, or for this one's helpers, lists no more of them.
    expect(sidesListed(list, 'a', new Set([sidesShownKey('b'), 'a'])).map((g) => g.id)).toEqual(['answering']);
    expect(sidesListed(undefined, 'a', new Set([sidesShownKey('a')]))).toEqual([]);
  });
});

describe('folding what’s listed in full under a session', () => {
  const count = (count: number, working = 0, waiting = 0) => ({ count, working, waiting });
  const counts = {
    helpers: (k: string) => (k === 'boss' ? count(9, 1, 1) : k === 'helper' ? count(2) : undefined),
    runs: (k: string) => (k === 'boss' ? count(40, 3, 1) : undefined),
    sides: (k: string) => (k === 'boss' ? count(6, 1) : undefined),
  };
  const members = [
    { key: 'boss', title: 'The boss' },
    { key: 'helper', title: 'Review PR 7' },
  ];

  it('offers a way back from each kind listed in full, saying how many leave the list', () => {
    const shown = new Set(['boss', runsShownKey('boss'), sidesShownKey('boss'), 'helper']);
    const folds = foldsOf(members, shown, counts);
    expect(folds.map((f) => [f.kind, f.key, f.hides])).toEqual([
      ['helpers', 'boss', 7],
      ['runs', 'boss', 39],
      ['sides', 'boss', 5],
      ['helpers', 'helper', 2],
    ]);
    expect(folds.map((f) => foldWords(f, f.key === 'boss'))).toEqual(['Hide 7 finished helpers', 'Hide 39 headless runs', 'Hide 5 answered side conversations', 'Hide 2 finished helpers of Review PR 7']);
  });

  it('offers none where nothing is listed in full, or there are none of the kind', () => {
    expect(foldsOf(members, new Set(), counts)).toEqual([]);
    expect(foldsOf(members, new Set([sidesShownKey('helper'), runsShownKey('helper')]), counts)).toEqual([]);
  });

  it('says what it does when nothing would leave the list', () => {
    expect(foldWords({ kind: 'helpers', key: 'boss', title: 'The boss', hides: 0 }, true)).toBe('List only the helpers at work');
    expect(foldWords({ kind: 'sides', key: 'boss', title: 'The boss', hides: 1 }, true)).toBe('Hide 1 answered side conversation');
  });
});
