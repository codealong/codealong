import { describe, expect, it } from 'vitest';
import {
  ageListingPr,
  archivedIn,
  bayRowsOf,
  currentProject,
  deviceCountsOf,
  folded,
  inPinOrder,
  laneInQuery,
  pinnedLaneRowsOf,
  prIn,
  prInDone,
  prInFilter,
  prInQuery,
  projectActions,
  projectsOf,
  prRowOf,
  prRowsOf,
  recentSessionCount,
  sessionIn,
  sessionIndexOf,
  sessionInFilter,
  sessionInQuery,
  sessionItemsOf,
  shownSessionsOf,
  unarchived,
} from './boardLists';
import { needsOn } from './devices';
import { bayOf } from './lanes';
import { ALL_PROJECTS } from './quick';
import * as fx from './testing/fixtures';
import { prSections } from './ui/PrsView';
import type { Lane, Pr, ProjectSummary, Session } from './types';
import type { SessionItem } from './ui/SessionsView';

const NOW = fx.NOW;
const DAY = 86_400_000;

const session = (key: string, over: Partial<Session> = {}): Session => fx.session({ key, ...over });
/** A worktree whose bay and glyph follow from its sessions and PR, as the engine's would. */
const lane = (id: string, sessions: Session[] = [], over: Partial<Lane> = {}): Lane => {
  const [bay, glyph] = bayOf(sessions, over.pr ?? null);
  return fx.lane({ id, sessions, bay, glyph, ...over });
};
const item = (l: Lane, s: Session): SessionItem => ({ kind: 'agent', key: `${l.id}::${s.key}`, lane: l, session: s });
const project = (name: string): ProjectSummary => ({ name, lanes: 1, repos: [] });

describe('projectsOf', () => {
  it('lists this device’s projects as they are when the other devices add none', () => {
    const here = [project('Acme'), project('Elsewhere')];
    expect(projectsOf(here, [lane('@b1:/r/app', [], { project: 'Acme' })])).toBe(here);
  });

  it('adds a project only another device has before Elsewhere, counting its worktrees, and skips hidden ones', () => {
    const there = [lane('@b1:/r/x', [], { project: 'Side' }), lane('@b1:/r/y', [], { project: 'Side' }), lane('@b1:/r/z', [], { project: 'Gone', hidden: true })];
    expect(projectsOf([project('Acme'), project('Elsewhere')], there).map((p) => [p.name, p.lanes])).toEqual([
      ['Acme', 1],
      ['Side', 2],
      ['Elsewhere', 1],
    ]);
    expect(projectsOf([project('Acme')], there).map((p) => p.name)).toEqual(['Acme', 'Side']);
  });
});

describe('currentProject', () => {
  const projects = [project('Elsewhere'), project('Acme'), project('Beta')];
  it('keeps the one picked while it’s listed, and All projects', () => {
    expect(currentProject('Beta', projects, false)).toBe('Beta');
    expect(currentProject(ALL_PROJECTS, projects, false)).toBe(ALL_PROJECTS);
  });
  it('falls back to the first project but Elsewhere, else all of them', () => {
    expect(currentProject('Gone', projects, false)).toBe('Acme');
    expect(currentProject(null, projects, false)).toBe('Acme');
    expect(currentProject('Gone', [project('Elsewhere')], false)).toBe(ALL_PROJECTS);
  });
  it('holds the one picked until the board is read', () => {
    expect(currentProject('Beta', [], true)).toBe('Beta');
    expect(currentProject(null, [], true)).toBe(ALL_PROJECTS);
  });
});

describe('searching', () => {
  const s = session('claude:work:s1', { title: 'Fix the login', account: 'work', excerpt: 'Pushed a branch' });
  const l = lane('/r/app/feature', [s], { branch: 'feat/login', pr: fx.pr({ number: 42, title: 'Login page' }), headline: 'waiting on CI' });

  it('finds a worktree by its name, branch, headline, path, PR and its sessions’ accounts and titles, in any case', () => {
    for (const q of ['', 'FEATURE', 'feat/log', 'waiting on', '/r/app', '#42', 'login page', 'work fix']) expect(laneInQuery(l, q)).toBe(true);
    expect(laneInQuery(l, 'nothing like it')).toBe(false);
  });

  it('finds a session by its title, account and excerpt, then its worktree and PR', () => {
    const r = item(l, s);
    for (const q of ['', 'fix the', 'WORK', 'pushed a', 'feature', 'feat/login', '#42 login']) expect(sessionInQuery(r, q)).toBe(true);
    // The headline is the worktree's, which the session's row doesn't show.
    expect(sessionInQuery(r, 'waiting on')).toBe(false);
  });

  it('finds a PR by its number with or without #, title, author, branch and repo', () => {
    const p = fx.prItem({ number: 42, title: 'Login page', author: 'sam', head: 'feat/login', repoSlug: 'acme/web' });
    for (const q of ['', '42', '#42', 'LOGIN', 'sam', 'feat/', 'acme/web']) expect(prInQuery(p, q)).toBe(true);
    expect(prInQuery(p, 'docs')).toBe(false);
  });
});

describe('the status filters on a session', () => {
  const at = (status: Session['status'], pr: Pr | null = null) => {
    const s = session('claude:claude:a', { status });
    return item(lane('/r/a', [s], { pr }), s);
  };

  it('puts errors with what needs you, and takes Working and Your turn as they are', () => {
    expect(sessionIn(at('needs'), 'needs')).toBe(true);
    expect(sessionIn(at('error'), 'needs')).toBe(true);
    expect(sessionIn(at('working'), 'working')).toBe(true);
    expect(sessionIn(at('unread'), 'unread')).toBe(true);
    expect(sessionIn(at('idle'), 'working')).toBe(false);
  });

  it('goes by its worktree’s PR for In review and PR needs work, and by the bay otherwise', () => {
    expect(sessionIn(at('idle', fx.pr({ state: 'open' })), 'review')).toBe(true);
    expect(sessionIn(at('idle', fx.pr({ state: 'merged' })), 'review')).toBe(false);
    expect(sessionIn(at('idle', fx.pr({ state: 'closed' })), 'review')).toBe(false);
    expect(sessionIn(at('idle', fx.pr({ checks: fx.checks({ failed: 1 }) })), 'work')).toBe(true);
    expect(sessionIn(at('idle'), 'work')).toBe(false);
    expect(sessionIn(at('idle', fx.pr({ state: 'merged' })), 'merged')).toBe(true);
  });

  it('lets a session through when any filter takes it, and every session with none', () => {
    expect(sessionInFilter(at('working'), [])).toBe(true);
    expect(sessionInFilter(at('working'), ['needs', 'working'])).toBe(true);
    expect(sessionInFilter(at('working'), ['needs', 'unread'])).toBe(false);
  });
});

describe('the status filters on a PR', () => {
  const row = (statuses: [Session['status'], boolean][], over: Parameters<typeof fx.prItem>[0] = {}) => {
    const l = lane('/r/a');
    const sessions = statuses.map(([status, visitor], i) => ({ lane: l, session: session(`claude:claude:s${i}`, { status, visitor: visitor ? 'claude:claude:x' : null }) }));
    return { kind: 'pr' as const, key: 'pr:x', pr: fx.prItem(over), lane: null, sessions, places: [] };
  };

  it('goes by the sessions that worked on it, leaving out visitors from other worktrees', () => {
    expect(prIn(row([['working', false]]), 'working')).toBe(true);
    expect(prIn(row([['error', false]]), 'needs')).toBe(true);
    expect(prIn(row([['working', true]]), 'working')).toBe(false);
    expect(prIn(row([['unread', false]]), 'needs')).toBe(false);
  });

  it('is in review while open, needs work with failing checks, and is in no bay of its own', () => {
    expect(prIn(row([]), 'review')).toBe(true);
    expect(prIn(row([], { state: 'merged' }), 'review')).toBe(false);
    expect(prIn(row([], { checks: fx.checks({ failed: 2 }) }), 'work')).toBe(true);
    expect(prIn(row([]), 'idle')).toBe(false);
    expect(prInFilter(row([]), [])).toBe(true);
    expect(prInFilter(row([]), ['idle', 'review'])).toBe(true);
  });
});

describe('the Sessions view’s lists', () => {
  const old = session('claude:claude:old', { lastActivity: NOW - 5 * DAY });
  const live = session('claude:claude:live', { lastActivity: NOW - 60_000, status: 'working' });
  const gone = session('claude:claude:gone', { archived: true, lastActivity: NOW - 2 * DAY });
  const gone2 = session('claude:claude:gone2', { archived: true, lastActivity: NOW - DAY });
  const visiting = session('claude:claude:v', { visitor: 'claude:claude:live' });
  const a = lane('/r/a', [old, live, gone, visiting]);
  const b = lane('/r/b', [gone2], { project: 'Other' });

  it('leaves archived sessions out of every worktree, and a worktree without any as it was', () => {
    const plain = lane('/r/c', [live]);
    const [x, y] = unarchived([a, plain]);
    expect(x.sessions.map((s) => s.key)).toEqual(['claude:claude:old', 'claude:claude:live', 'claude:claude:v']);
    expect(y).toBe(plain);
  });

  it('gives back the same copy of a worktree that is the same object, so its row can skip rendering, and a new one for a worktree that changed', () => {
    const [first] = unarchived([a]);
    expect(unarchived([a])[0]).toBe(first);
    const changed = { ...a, sessions: [...a.sessions, session('claude:claude:new')] };
    const [next] = unarchived([changed]);
    expect(next).not.toBe(first);
    expect(next.sessions.map((s) => s.key)).toEqual(['claude:claude:old', 'claude:claude:live', 'claude:claude:v', 'claude:claude:new']);
  });

  it('lists the project’s archived sessions, the latest first, without visitors', () => {
    expect(archivedIn([a, b], 'Acme').map((r) => r.key)).toEqual(['/r/a::claude:claude:gone']);
    expect(archivedIn([a, b], ALL_PROJECTS).map((r) => r.key)).toEqual(['/r/b::claude:claude:gone2', '/r/a::claude:claude:gone']);
  });

  it('has each session once in the worktree it runs in, keyed by both', () => {
    expect(sessionItemsOf([lane('/r/a', [old, visiting])]).map((r) => r.key)).toEqual(['/r/a::claude:claude:old']);
  });

  it('shows sessions active since the cutoff and pinned ones, then the search and the filters', () => {
    const items = sessionItemsOf([lane('/r/a', [old, live])]);
    const cutoff = NOW - DAY;
    expect(shownSessionsOf(items, cutoff, new Set(), '', []).map((r) => r.session.key)).toEqual(['claude:claude:live']);
    expect(shownSessionsOf(items, cutoff, new Set(['claude:claude:old']), '', []).map((r) => r.session.key)).toEqual(['claude:claude:old', 'claude:claude:live']);
    expect(shownSessionsOf(items, 0, new Set(), '', ['working']).map((r) => r.session.key)).toEqual(['claude:claude:live']);
    expect(shownSessionsOf(items, 0, new Set(), 'nothing', [])).toEqual([]);
    expect(recentSessionCount(items, cutoff, new Set())).toBe(1);
    expect(recentSessionCount(items, 0, new Set())).toBe(2);
  });

  it('puts the pinned rows in the pins’ order, leaving out pins not shown', () => {
    const rows = [{ k: 'a' }, { k: 'b' }, { k: 'c' }];
    expect(inPinOrder(rows, ['c', 'x', 'a'], (r) => r.k)).toEqual([{ k: 'c' }, { k: 'a' }]);
  });
});

describe('the board’s lists', () => {
  const needs = lane('/r/needs', [session('claude:claude:n', { status: 'needs' })], { callsign: 'needs-it' });
  const idle = lane('/r/idle', [session('claude:claude:i')], { callsign: 'quiet' });
  const parked = lane('/r/parked', [], { parked: true });

  it('pins worktrees in the pins’ order, as the search and the filter show them', () => {
    expect(pinnedLaneRowsOf([needs, idle], ['/r/idle', '/r/needs'], '', [], '').map((r) => r.key)).toEqual(['/r/idle', '/r/needs']);
    expect(pinnedLaneRowsOf([needs, idle], ['/r/idle', '/r/needs'], 'quiet', [], '').map((r) => r.key)).toEqual(['/r/idle']);
    expect(pinnedLaneRowsOf([needs, idle], ['/r/idle', '/r/needs'], '', ['needs'], '').map((r) => r.key)).toEqual(['/r/needs']);
    expect(pinnedLaneRowsOf([needs], ['/r/gone'], '', [], '')).toEqual([]);
  });

  it('puts every other worktree in its bay, leaving pinned ones out', () => {
    const bays = bayRowsOf([needs, idle, parked], '', new Set(['/r/idle']), [], '');
    expect(bays.map((b) => [b.bay, b.rows.map((r) => r.key)])).toEqual([
      ['needs', ['/r/needs']],
      ['idle', ['/r/parked']],
    ]);
  });

  it('folds parked worktrees away when asked to', () => {
    const bays = bayRowsOf([idle, parked], '', new Set(), [], '');
    expect(folded(bays, true).map((b) => b.rows.map((r) => r.key))).toEqual([['/r/idle']]);
    expect(folded(bays, false).map((b) => b.rows.map((r) => r.key).sort())).toEqual([['/r/idle', '/r/parked']]);
  });
});

describe('the PRs view’s lists', () => {
  const s = session('claude:claude:s');
  const holder = lane('/r/feature', [s]);
  const laneById = new Map([[holder.id, holder]]);
  const index = sessionIndexOf([holder]);

  it('indexes each session by its worktree and key', () => {
    expect([...index.keys()]).toEqual(['/r/feature::claude:claude:s']);
  });

  it('makes a PR’s row with the worktree that holds it and the sessions that worked on it, as they’re listed', () => {
    const pr = fx.prItem({ lanes: ['/r/feature', '/r/gone'], sessions: [{ lane: '/r/feature', key: 'claude:claude:s' }, { lane: '/r/gone', key: 'x' }] });
    const row = prRowOf(pr, laneById, index);
    expect(row.key).toBe('pr:acme/app#7');
    expect(row.lane).toBe(holder);
    expect(row.sessions).toEqual([{ lane: holder, session: s }]);
    expect(row.places.map((p) => [p.lane.id, p.holds])).toEqual([['/r/feature', true]]);
  });

  it('shows PRs updated since the cutoff and pinned ones, then the search and the filters', () => {
    const fresh = fx.prItem({ number: 1, title: 'Fresh', updatedAt: NOW });
    const stale = fx.prItem({ number: 2, title: 'Stale', updatedAt: NOW - 5 * DAY, state: 'merged' });
    const cutoff = NOW - DAY;
    expect(prRowsOf([fresh, stale], cutoff, new Set(), '', [], laneById, index).map((r) => r.pr.number)).toEqual([1]);
    expect(prRowsOf([fresh, stale], cutoff, new Set([stale.key]), '', [], laneById, index).map((r) => r.pr.number)).toEqual([1, 2]);
    expect(prRowsOf([fresh, stale], 0, new Set(), 'stale', [], laneById, index).map((r) => r.pr.number)).toEqual([2]);
    expect(prRowsOf([fresh, stale], 0, new Set(), '', ['review'], laneById, index).map((r) => r.pr.number)).toEqual([1]);
  });

  it('lists merged and closed PRs under Merged & closed, which is folded, unless they’re pinned', () => {
    const merged = fx.prItem({ number: 1, state: 'merged' });
    expect(prInDone(merged, new Set())).toBe(true);
    expect(prInDone(fx.prItem({ number: 2, state: 'closed' }), new Set())).toBe(true);
    expect(prInDone(merged, new Set([merged.key]))).toBe(false);
    for (const state of ['open', 'draft', 'approved', 'changes'] as const) expect(prInDone(fx.prItem({ state }), new Set())).toBe(false);
    // The section the view lists it in says the same.
    for (const state of ['open', 'draft', 'approved', 'changes', 'merged', 'closed'] as const) {
      const pr = fx.prItem({ state });
      expect(prSections([prRowOf(pr, laneById, index)])[0].id === 'done').toBe(prInDone(pr, new Set()));
    }
  });
});

describe('ageListingPr', () => {
  const ages = [
    { id: '1d', ms: DAY },
    { id: '3d', ms: 3 * DAY },
    { id: '7d', ms: 7 * DAY },
    { id: 'all', ms: 0 },
  ] as const;
  const old = fx.prItem({ number: 4, updatedAt: NOW - 5 * DAY });

  it('widens the age filter as little as lists a PR asked to be shown', () => {
    expect(ageListingPr(old, new Set(), ages, '1d', NOW)).toBe('7d');
    expect(ageListingPr(old, new Set(), ages, '3d', NOW)).toBe('7d');
    expect(ageListingPr(fx.prItem({ updatedAt: NOW - 10 * DAY }), new Set(), ages, '1d', NOW)).toBe('all');
    // What it widens to lists it, by the rule the PRs view goes by.
    expect(prRowsOf([old], NOW - 7 * DAY, new Set(), '', [], new Map(), sessionIndexOf([])).map((r) => r.pr.number)).toEqual([4]);
  });

  it('leaves the filter alone when it lists the PR already', () => {
    expect(ageListingPr(old, new Set(), ages, '7d', NOW)).toBeNull();
    expect(ageListingPr(old, new Set(), ages, 'all', NOW)).toBeNull();
    expect(ageListingPr(fx.prItem({ updatedAt: NOW - DAY / 2 }), new Set(), ages, '1d', NOW)).toBeNull();
    // A pinned PR is listed whatever its age.
    expect(ageListingPr(old, new Set([old.key]), ages, '1d', NOW)).toBeNull();
  });
});

describe('counts', () => {
  it('says what needs you and what’s your turn in each project, and in all of them', () => {
    const lanes = [lane('/r/a', [session('claude:claude:a', { status: 'needs', lastActivity: NOW })]), lane('/r/b', [session('claude:claude:b', { status: 'unread', lastActivity: NOW })], { project: 'Other' })];
    expect(projectActions(lanes, [project('Acme'), project('Other')], 0, new Set(), '')).toEqual({
      Acme: { needs: 1, unread: 0 },
      Other: { needs: 0, unread: 1 },
      [ALL_PROJECTS]: { needs: 1, unread: 1 },
    });
  });

  it('counts each device’s sessions, this one first', () => {
    const lanes = [lane('/r/a', [session('claude:claude:a', { status: 'needs' })]), lane('@b1:/r/a', [session('@b1:claude:claude:b', { status: 'error' }), session('@b1:claude:claude:c', { status: 'needs' })])];
    expect([...deviceCountsOf(lanes, [{ short: 'b1' }, { short: 'c2' }], needsOn)]).toEqual([
      [null, 1],
      ['b1', 2],
      ['c2', 0],
    ]);
  });
});
