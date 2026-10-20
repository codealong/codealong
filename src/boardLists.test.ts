import { describe, expect, it } from 'vitest';
import { projectBoard, type BoardColumn, type BoardDoc } from './boards/model';
import { memberTest, subjectsOf } from './boards/rules';
import {
  ageListingChain,
  ageListingPr,
  ageListingSession,
  archivedIn,
  bayRowsOf,
  chainOf,
  currentProject,
  deviceCountsOf,
  folded,
  helperCounts,
  HELPERS_SHOWN_MAX,
  inPinOrder,
  kanbanColumns,
  laneInQuery,
  pinnedLaneRowsOf,
  placeHelpers,
  revealing,
  toggleShown,
  sessionItemOf,
  showingHelper,
  sideCounts,
  prIn,
  prInDone,
  prInFilter,
  prInQuery,
  prCardItem,
  projectActions,
  projectsOf,
  prRowOf,
  prRowsOf,
  prsRead,
  recentSessionCount,
  runCounts,
  sessionIn,
  sessionIndexOf,
  sessionInFilter,
  sessionInQuery,
  sessionItemsOf,
  saidLinesOf,
  shownSessionsOf,
  todoInQuery,
  unarchived,
  agentOptions,
  facetsIn,
  facetsOn,
  onlyPick,
  prFacetOptions,
  prInFacets,
  sessionInFacets,
  todoInFacets,
  todoLabelOptions,
  togglePick,
  type Facets,
} from './boardLists';
import { needsOn } from './devices';
import { bayOf, runsShownKey, sessionHomes } from './lanes';
import { ALL_PROJECTS } from './quick';
import { todoItem, type TodoItem } from './todos';
import * as fx from './testing/fixtures';
import { prSections } from './ui/PrsView';
import type { Board, Lane, Pr, PrItem, ProjectSummary, Session, SideGlance, TextHit, Todo } from './types';
import { familiesOf, helpersUnder, sessionSections, type SessionItem } from './ui/SessionsView';

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

  it('finds a PR by its number with or without #, title, author, branch, repo and labels', () => {
    const labels = [{ name: 'needs-copy', color: 'fbca04' }, { name: 'dependencies', color: '0366d6', description: 'Pull requests that update a dependency file' }];
    const p = prRowOf(fx.prItem({ number: 42, title: 'Login page', author: 'sam', head: 'feat/login', repoSlug: 'acme/web', labels }), new Map(), sessionIndexOf([]));
    for (const q of ['', '42', '#42', 'LOGIN', 'sam', 'feat/', 'acme/web', 'page login', 'Dependencies', 'needs-copy login']) expect(prInQuery(p, q)).toBe(true);
    expect(prInQuery(p, 'docs')).toBe(false);
    // A label's description isn't searched: only what the row shows.
    expect(prInQuery(p, 'update')).toBe(false);
  });

  it('finds by every word in any order, as the quick panel does, a short one among others only whole', () => {
    const r = item(l, s);
    for (const q of ['login fix', 'branch pushed', 'work  login']) expect(sessionInQuery(r, q)).toBe(true);
    expect(laneInQuery(l, 'ci waiting')).toBe(true);
    // The "a" of "Pushed a branch" counts whole, so "fix a" finds it, but "fix g" isn't a word there.
    expect(sessionInQuery(r, 'fix a')).toBe(true);
    expect(sessionInQuery(r, 'fix g')).toBe(false);
    expect(sessionInQuery(r, 'login nothing')).toBe(false);
  });

  it('finds a todo by its title, description, labels and what it’s linked to', () => {
    const t = todoItem(fx.todo({ id: 't1', title: 'Ship the login', description: 'Before Friday', labels: ['l1'], worktrees: [l.id], sessions: [s.key] }), new Map([[l.id, l]]), new Map([[s.key, { lane: l, session: s }]]), new Map([['l1', { id: 'l1', name: 'Bug', color: 'red' }]]));
    for (const q of ['', 'bug', 'friday ship', 'feat/login', 'work fix']) expect(todoInQuery(t, q)).toBe(true);
    expect(todoInQuery(t, 'nothing like it')).toBe(false);
  });
});

describe('searching what was said', () => {
  const s = session('claude:work:s1', { title: 'Fix the login', account: 'work' });
  const quiet = session('claude:work:s2', { title: 'Tidy the docs', account: 'work' });
  const l = lane('/r/app/feature', [s, quiet], { pr: fx.pr({ number: 42, title: 'Login page' }) });
  const hit = (key: string, snippet: string, you = false): TextHit => ({ key, snippet, you, at: NOW, score: 4 });
  const said = new Map([[quiet.key, hit(quiet.key, 'The webhook retries twice, then gives up.')]]);

  it('finds a session, its worktree, its PR and its todo by what was said in it', () => {
    expect(sessionInQuery(item(l, quiet), 'webhook retries', said)).toBe(true);
    expect(sessionInQuery(item(l, s), 'webhook retries', said)).toBe(false);
    expect(laneInQuery(l, 'webhook retries', said)).toBe(true);
    expect(laneInQuery(lane('/r/other', [s]), 'webhook retries', said)).toBe(false);
    const pr = fx.prItem({ number: 42, title: 'Login page', lanes: [l.id], sessions: [{ lane: l.id, key: quiet.key }] });
    expect(prInQuery(prRowOf(pr, new Map([[l.id, l]]), sessionIndexOf([l])), 'webhook retries', said)).toBe(true);
    expect(prInQuery(prRowOf({ ...pr, sessions: [] }, new Map([[l.id, l]]), sessionIndexOf([l])), 'webhook retries', said)).toBe(false);
    const t = todoItem(fx.todo({ id: 't1', title: 'Ship it', sessions: [quiet.key] }), new Map([[l.id, l]]), new Map([[quiet.key, { lane: l, session: quiet }]]), new Map());
    expect(todoInQuery(t, 'webhook retries', said)).toBe(true);
    expect(shownSessionsOf([item(l, s), item(l, quiet)], 0, new Set(), 'webhook retries', [], said).map((r) => r.session.key)).toEqual([quiet.key]);
    expect(archivedIn([lane('/r/a', [{ ...quiet, archived: true }])], ALL_PROJECTS, 'webhook retries', said).map((r) => r.session.key)).toEqual([quiet.key]);
  });

  it('gives a line only to the sessions found by what was said and not by what describes them', () => {
    const lines = saidLinesOf([item(l, s), item(l, quiet)], 'webhook retries', said);
    expect([...lines.keys()]).toEqual([quiet.key]);
    expect(lines.get(quiet.key)).toEqual({ text: 'The webhook retries twice, then gives up.', you: false, marks: [[4, 11], [12, 19]] });
    // Both said "fix", but it's in s's title, so s's row needs no line to say why it's listed.
    const fix = new Map([
      [s.key, hit(s.key, 'You asked to fix it first.', true)],
      [quiet.key, hit(quiet.key, 'A fix for the docs.')],
    ]);
    expect([...saidLinesOf([item(l, s), item(l, quiet)], 'fix', fix).keys()]).toEqual([quiet.key]);
    expect(saidLinesOf([item(l, quiet)], '', said).size).toBe(0);
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

describe('the sidebar’s facets', () => {
  const agent = (account: string, n: number) => {
    const s = session(`${account}:${account}:s${n}`, { account });
    return item(lane(`/r/${n}`, [s]), s);
  };
  const pr = (repoSlug: string, number: number, labels: string[] = [], over: Parameters<typeof fx.prItem>[0] = {}) => prRowOf(fx.prItem({ repoSlug, number, labels: labels.map((name) => ({ name, color: 'ff0000' })), ...over }), new Map(), new Map());
  const todo = (id: string, labels: string[], over: Partial<Todo> = {}) => todoItem(fx.todo({ id, labels, ...over }), new Map(), new Map(), new Map());

  it('picks and unpicks, and a chip’s double-click shows only its pick, or everything again', () => {
    let f: Facets = {};
    f = togglePick(f, 'repos', 'o/a');
    f = togglePick(f, 'repos', 'o/b');
    expect(f.repos).toEqual(['o/a', 'o/b']);
    expect(togglePick(f, 'repos', 'o/a').repos).toEqual(['o/b']);
    expect(onlyPick(f, 'repos', 'o/b').repos).toEqual(['o/b']);
    expect(onlyPick({ repos: ['o/b'] }, 'repos', 'o/b').repos).toEqual([]);
  });

  it('offers each view its own, and counts as narrowing only the picks of the view showing', () => {
    expect(facetsIn('sessions')).toEqual(['agents']);
    expect(facetsIn('prs')).toEqual(['repos', 'prLabels', 'authors']);
    expect(facetsIn('todo')).toEqual(['labels']);
    expect(facetsIn('kanban')).toEqual(['labels']);
    expect(facetsIn('worktrees')).toEqual([]);
    expect(facetsOn({ agents: ['codex'], repos: [], labels: ['l1'] }, 'sessions')).toEqual(['agents']);
    expect(facetsOn({ agents: ['codex'], repos: [] }, 'prs')).toEqual([]);
  });

  it('lets a row through with any of a facet’s picks, when every facet holds', () => {
    expect(sessionInFacets(agent('codex', 1), {})).toBe(true);
    expect(sessionInFacets(agent('codex', 1), { agents: ['claude', 'codex'] })).toBe(true);
    expect(sessionInFacets(agent('codex', 1), { agents: ['claude'] })).toBe(false);
    expect(prInFacets(pr('o/a', 1, ['bug']), { repos: ['o/a'], prLabels: ['bug', 'docs'] })).toBe(true);
    expect(prInFacets(pr('o/a', 1, ['bug']), { repos: ['o/a'], prLabels: ['docs'] })).toBe(false);
    expect(prInFacets(pr('o/a', 1), { repos: ['o/b'] })).toBe(false);
    expect(prInFacets(pr('o/a', 1, [], { author: 'ana' }), { authors: ['ana', 'bo'] })).toBe(true);
    expect(prInFacets(pr('o/a', 1, [], { author: 'ana' }), { authors: ['bo'] })).toBe(false);
    expect(prInFacets(pr('o/a', 1, [], { author: null }), { authors: ['bo'] })).toBe(false);
    expect(todoInFacets(todo('a', ['l1', 'l2']), { labels: ['l2'] })).toBe(true);
    expect(todoInFacets(todo('a', []), { labels: ['l2'] })).toBe(false);
  });

  it('counts each option, the most first, keeping one picked that has none left', () => {
    const rows = [agent('codex', 1), agent('claude', 2), agent('codex', 3)];
    expect(agentOptions(rows, { agents: ['cursor'] }).map((o) => [o.id, o.count])).toEqual([
      ['codex', 2],
      ['claude', 1],
      ['cursor', 0],
    ]);
  });

  it('counts a PR facet’s open PRs as the other facet narrows them, naming repos as GitHub does', () => {
    const rows = [pr('o/app', 1, ['bug']), pr('o/app', 2), pr('o/web', 3, ['bug', 'docs']), pr('o/web', 4, ['bug'], { state: 'merged' })];
    const all = prFacetOptions(rows, {});
    expect(all.repos.map((o) => [o.id, o.label, o.count])).toEqual([
      ['o/app', 'app', 2],
      ['o/web', 'web', 1],
    ]);
    expect(all.prLabels.map((o) => [o.id, o.count, o.color])).toEqual([
      ['bug', 2, 'ff0000'],
      ['docs', 1, 'ff0000'],
    ]);
    // A repo picked narrows the labels' counts, not its own.
    const picked = prFacetOptions(rows, { repos: ['o/web'] });
    expect(picked.repos.map((o) => o.count)).toEqual([2, 1]);
    expect(picked.prLabels.map((o) => [o.id, o.count])).toEqual([
      ['bug', 1],
      ['docs', 1],
    ]);
  });

  it('counts open PRs by who opened them, yours first as you', () => {
    const rows = [
      pr('o/app', 1, ['bug'], { author: 'ana' }),
      pr('o/app', 2, [], { author: 'ana' }),
      pr('o/web', 3, [], { author: 'me', mine: true }),
      pr('o/web', 4, [], { author: 'bo', state: 'merged' }),
      pr('o/web', 5, [], { author: null }),
    ];
    expect(prFacetOptions(rows, {}).authors.map((o) => [o.id, o.label, o.count])).toEqual([
      ['me', 'you', 1],
      ['ana', 'ana', 2],
    ]);
    // The other facets narrow its counts, and a pick with none left stays to unpick.
    expect(prFacetOptions(rows, { prLabels: ['bug'], authors: ['bo'] }).authors.map((o) => [o.id, o.count])).toEqual([
      ['ana', 1],
      ['bo', 0],
    ]);
    // Picked, it narrows the others' counts.
    expect(prFacetOptions(rows, { authors: ['me'] }).repos.map((o) => [o.id, o.count])).toEqual([['o/web', 1]]);
  });

  it('lists todos’ labels in your order, counting the ones not done', () => {
    const labels = [
      { id: 'l1', name: 'billing', color: 'red' as const },
      { id: 'l2', name: 'docs', color: 'teal' as const },
      { id: 'l3', name: 'unused', color: 'gray' as const },
    ];
    const rows = [todo('a', ['l2']), todo('b', ['l1', 'l2']), todo('c', ['l1'], { status: 'done' })];
    expect(todoLabelOptions(rows, labels, {}).map((o) => [o.id, o.label, o.count])).toEqual([
      ['l1', 'billing', 1],
      ['l2', 'docs', 2],
    ]);
    expect(todoLabelOptions(rows, labels, { labels: ['l3'] }).map((o) => o.id)).toEqual(['l1', 'l2', 'l3']);
    // One picked and since deleted can still be unpicked.
    expect(todoLabelOptions(rows, labels, { labels: ['gone'] }).map((o) => [o.id, o.label])).toEqual([
      ['l1', 'billing'],
      ['l2', 'docs'],
      ['gone', 'Deleted label'],
    ]);
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

  it('lists the archived sessions a search finds, as Sessions searches the rest', () => {
    const named = { ...gone, title: 'Fix the login flake' };
    const lanes = [lane('/r/a', [old, named]), b];
    expect(archivedIn(lanes, ALL_PROJECTS, 'LOGIN').map((r) => r.key)).toEqual(['/r/a::claude:claude:gone']);
    expect(archivedIn(lanes, ALL_PROJECTS, 'checkout')).toEqual([]);
    expect(archivedIn(lanes, ALL_PROJECTS, '').map((r) => r.key)).toEqual(['/r/b::claude:claude:gone2', '/r/a::claude:claude:gone']);
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

  describe('helpers', () => {
    const boss = session('claude:claude:boss', { status: 'idle' });
    const helper = (key: string, status: Session['status'], helps: Session['helper'] = { parent: boss.key }) => session(key, { status, helper: helps });
    const busy = helper('codex:codex:busy', 'working');
    const done = helper('codex:codex:done', 'idle');
    const asks = helper('codex:codex:asks', 'needs');
    const review = helper('claude:claude:review', 'idle', { pr: 'acme/app#7' });
    const reviewAsks = helper('claude:claude:review2', 'error', { pr: 'acme/app#7' });
    const keysOf = (rows: { session: Session }[]) => rows.map((r) => r.session.key);

    it('lists a helper with the session it works for while it works or waits on you, and counts the rest there', () => {
      const items = sessionItemsOf([lane('/r/a', [boss, busy, done, asks])]);
      expect(keysOf(placeHelpers(items, false))).toEqual([boss.key, busy.key, asks.key]);
      const c = helperCounts(items).get(boss.key);
      expect(c).toMatchObject({ count: 3, working: 1, waiting: 1 });
      // Each for its chip, the most urgent first, as a lane lists its sessions.
      expect(c?.each.map((g) => [g.key, g.status])).toEqual([
        [asks.key, 'needs'],
        [busy.key, 'working'],
        [done.key, 'idle'],
      ]);
      // A review says which PR.
      const reviewOfMine = helper('claude:claude:rv', 'idle', { parent: boss.key, pr: 'acme/app#204' });
      expect(helperCounts(sessionItemsOf([lane('/r/a', [boss, reviewOfMine])])).get(boss.key)?.each[0].review).toBe('#204');
    });

    it('lists every helper of a session you asked to see them all of, finished ones too', () => {
      const items = sessionItemsOf([lane('/r/a', [boss, busy, done, asks])]);
      expect(keysOf(placeHelpers(items, false, new Set([boss.key])))).toEqual([boss.key, busy.key, done.key, asks.key]);
      // Not where the session itself isn't listed.
      expect(keysOf(placeHelpers(sessionItemsOf([lane('/r/a', [done])]), false, new Set([boss.key])))).toEqual([]);
      // One the age or status filter left out, which its chip counts, comes from all of them.
      const filtered = sessionItemsOf([lane('/r/a', [boss, busy])]);
      const all = sessionItemsOf([lane('/r/a', [boss, busy, done, asks])]);
      expect(keysOf(placeHelpers(filtered, false, new Set([boss.key]), all)).sort()).toEqual([boss.key, busy.key, done.key, asks.key].sort());
      expect(keysOf(placeHelpers(filtered, false, new Set(), all))).toEqual([boss.key, busy.key]);
      // The chip's double-click turns it on and off, keeping the latest.
      expect(toggleShown(['a'], 'b')).toEqual(['a', 'b']);
      expect(toggleShown(['a', 'b'], 'a')).toEqual(['b']);
      const full = Array.from({ length: HELPERS_SHOWN_MAX }, (_, i) => `s${i}`);
      expect(toggleShown(full, 'new')).toEqual([...full.slice(1), 'new']);
    });

    it('says on its chip’s card who runs each and where, if not in its session’s worktree', () => {
      const reports = helper('codex:codex:reports', 'working', { parent: boss.key, report: true });
      const away = { ...done, model: 'gpt-5.5', account: 'codex' };
      const items = sessionItemsOf([lane('/r/a', [boss, reports]), lane('/r/b', [away], { callsign: 'elsewhere' })]);
      const [a, b] = helperCounts(items).get(boss.key)?.each ?? [];
      expect([a.key, a.worktree, a.reports, a.model]).toEqual([reports.key, null, true, null]);
      expect([b.key, b.worktree, b.reports, b.model, b.account]).toEqual([away.key, 'elsewhere', false, 'gpt-5.5', 'codex']);
      expect(a.excerpt).toBe(reports.excerpt);
    });

    it('lists a helper its card shows under its session, unless it’s listed there anyway', () => {
      const items = sessionItemsOf([lane('/r/a', [boss, busy, done, asks])]);
      const row = (s: Session) => items.find((r) => r.session.key === s.key)!;
      const listed = placeHelpers(items, false);
      expect(showingHelper(['x'], listed, row(done))).toEqual(['x', boss.key]);
      // Listed, at work or waiting on you, or its session's helpers all listed: as it was.
      const shown = ['x'];
      expect(showingHelper(shown, listed, row(busy))).toBe(shown);
      expect(showingHelper(shown, listed, row(asks))).toBe(shown);
      const all = [boss.key];
      expect(showingHelper(all, listed, row(done))).toBe(all);
      // A search that finds a finished one lists it, but opening one Codealong runs clears the
      // search, which would leave it out: its session is added all the same.
      const searched = placeHelpers(items, true);
      expect(keysOf(searched)).toContain(done.key);
      expect(keysOf(placeHelpers(items, false, new Set(showingHelper([], searched, row(done)))))).toContain(done.key);
      // A search that finds only its session leaves an active one out, which its card still
      // shows: a click lists it, the search still there.
      for (const status of ['working', 'needs', 'error', 'unread'] as const) {
        const parent = { ...boss, title: 'Implement feature' };
        const kid = { ...helper('codex:codex:kid', status), title: 'Review changes' };
        const both = sessionItemsOf([lane('/r/a', [parent, kid])]);
        const found = placeHelpers(shownSessionsOf(both, 0, new Set(), 'Implement feature', []), true, new Set(), both);
        expect(keysOf(found)).toEqual([parent.key]);
        const after = showingHelper([], found, both[1]);
        expect(keysOf(placeHelpers(shownSessionsOf(both, 0, new Set(), 'Implement feature', []), true, new Set(after), both))).toEqual([parent.key, kid.key]);
      }
      // A row of its own made as the list makes them.
      expect(sessionItemOf({ lane: row(done).lane, session: done })).toEqual(row(done));
    });

    it('lists a helper’s own helpers under it in turn, and all of them when it’s asked to', () => {
      const kid = helper('codex:codex:kid', 'working');
      const grandkid = (status: Session['status']) => helper('claude:claude:grandkid', status, { parent: kid.key });
      expect(keysOf(placeHelpers(sessionItemsOf([lane('/r/a', [boss, kid, grandkid('working')])]), false))).toEqual([boss.key, kid.key, 'claude:claude:grandkid']);
      const items = sessionItemsOf([lane('/r/a', [boss, kid, grandkid('idle')])]);
      expect(keysOf(placeHelpers(items, false))).toEqual([boss.key, kid.key]);
      expect(keysOf(placeHelpers(items, false, new Set([kid.key])))).toEqual([boss.key, kid.key, 'claude:claude:grandkid']);
      // Not under a helper that isn't listed itself.
      expect(keysOf(placeHelpers(sessionItemsOf([lane('/r/a', [boss, { ...kid, status: 'idle' }, grandkid('idle')])]), false, new Set([kid.key])))).toEqual([boss.key]);
    });

    it('puts a pinned session’s helpers under it among the pinned ones', () => {
      const rows = placeHelpers(sessionItemsOf([lane('/r/a', [boss, busy, asks, session('claude:claude:other')])]), false);
      const under = helpersUnder(rows, new Set([boss.key]));
      expect([...under.keys()]).toEqual([boss.key]);
      expect(under.get(boss.key)?.map((r) => [r.session.key, r.nested])).toEqual([
        [asks.key, true],
        [busy.key, true],
      ]);
      expect(helpersUnder(rows, new Set(['claude:claude:other'])).size).toBe(0);
      // Pinned too, a helper is listed once, as a pinned one, not under its session as well.
      expect(helpersUnder(rows, new Set([boss.key, asks.key])).get(boss.key)?.map((r) => r.session.key)).toEqual([busy.key]);
      // A pinned helper's own helpers go under it.
      const kid = helper('codex:codex:kid', 'working');
      const grandkid = helper('claude:claude:grandkid', 'working', { parent: kid.key });
      const family = placeHelpers(sessionItemsOf([lane('/r/a', [boss, kid, grandkid])]), false);
      expect(helpersUnder(family, new Set([kid.key])).get(kid.key)?.map((r) => r.session.key)).toEqual([grandkid.key]);
      expect(helpersUnder(family, new Set([kid.key])).has(boss.key)).toBe(false);
      // Session and helper both pinned: the helper's helper under it, and nothing twice.
      const both = helpersUnder(family, new Set([boss.key, kid.key]));
      expect([...both].map(([k, rs]) => [k, rs.map((r) => r.session.key)])).toEqual([[kid.key, [grandkid.key]]]);
    });

    it('leaves a helper out while it doesn’t need you when its session isn’t listed, and a review always, unless a search finds it', () => {
      const items = sessionItemsOf([lane('/r/a', [busy, done, asks, review, reviewAsks])]);
      expect(keysOf(placeHelpers(items, false))).toEqual([asks.key, reviewAsks.key]);
      expect(keysOf(placeHelpers(items, true))).toEqual(keysOf(items));
      // Nothing to place: the same list back.
      const plain = sessionItemsOf([lane('/r/a', [boss])]);
      expect(placeHelpers(plain, false)).toBe(plain);
      // Archived helpers aren't counted.
      expect(helperCounts(sessionItemsOf([lane('/r/a', [boss, { ...done, archived: true }])])).size).toBe(0);
    });
  });

  describe('headless runs', () => {
    const boss = session('claude:claude2:boss', { status: 'working' });
    const run = (key: string, status: Session['status'], over: Partial<Session> = {}) => session(key, { status, headless: true, origin: { key: boss.key, how: 'started' }, account: 'claude2', model: 'claude-opus-5-5', ...over });
    const busy = run('claude:claude2:busy', 'working');
    const busy2 = run('claude:claude2:busy2', 'working');
    const ended = run('claude:claude2:ended', 'idle');
    const asks = run('claude:claude2:asks', 'needs');
    const keysOf = (rows: { session: Session }[]) => rows.map((r) => r.session.key);

    it('counts the runs a session started on its row rather than listing them, at work or done', () => {
      const items = sessionItemsOf([lane('/r/a', [boss, busy, busy2, ended])]);
      expect(keysOf(placeHelpers(items, false))).toEqual([boss.key]);
      const c = runCounts(items).get(boss.key);
      expect(c).toMatchObject({ count: 3, working: 2, waiting: 0 });
      // Each for its chip's card, the ones at work first, saying who runs it on which model.
      expect(c?.each.map((g) => [g.key, g.status, g.account, g.model])).toEqual([
        [busy.key, 'working', 'claude2', 'claude-opus-5-5'],
        [busy2.key, 'working', 'claude2', 'claude-opus-5-5'],
        [ended.key, 'idle', 'claude2', 'claude-opus-5-5'],
      ]);
      // Not on its helpers' chip, and not the archived ones.
      expect(helperCounts(items).size).toBe(0);
      expect(runCounts(sessionItemsOf([lane('/r/a', [boss, { ...ended, archived: true }])])).size).toBe(0);
    });

    it('lists one that waits on you, all of them where you asked, under it, and the ones a search finds', () => {
      const items = sessionItemsOf([lane('/r/a', [boss, busy, ended, asks])]);
      expect(keysOf(placeHelpers(items, false))).toEqual([boss.key, asks.key]);
      // Listing its helpers doesn't list its runs; asking for its runs does.
      expect(keysOf(placeHelpers(items, false, new Set([boss.key])))).toEqual([boss.key, asks.key]);
      const all = placeHelpers(items, false, new Set([runsShownKey(boss.key)]));
      expect(keysOf(all)).toEqual([boss.key, busy.key, ended.key, asks.key]);
      expect(keysOf(placeHelpers(items, true))).toEqual(keysOf(items));
      // Listed, they go under it.
      const rows = sessionSections(all, 'status', NOW).flatMap((s) => s.rows);
      expect(rows.map((r) => [r.session.key, !!r.nested])).toEqual([
        [boss.key, false],
        [asks.key, true],
        [busy.key, true],
        [ended.key, true],
      ]);
    });

    it('lists a run its chip’s card shows under its session, its runs’ key added', () => {
      const items = sessionItemsOf([lane('/r/a', [boss, busy, ended, asks])]);
      const row = (s: Session) => items.find((r) => r.session.key === s.key)!;
      const listed = placeHelpers(items, false);
      expect(showingHelper(['x'], listed, row(ended))).toEqual(['x', runsShownKey(boss.key)]);
      expect(showingHelper(['x'], listed, row(busy))).toEqual(['x', runsShownKey(boss.key)]);
      // One that waits on you is listed anyway; and with its runs listed, nothing to add.
      const shown = ['x'];
      expect(showingHelper(shown, listed, row(asks))).toBe(shown);
      const all = [runsShownKey(boss.key)];
      expect(showingHelper(all, listed, row(ended))).toBe(all);
    });

    it('keeps what lists a run’s helper when its card lists the run, however many are kept', () => {
      const helper = session('codex:codex:helper', { status: 'idle', helper: { parent: boss.key } });
      const deep = run('claude:claude2:deep', 'idle', { origin: { key: helper.key, how: 'started' } });
      const items = sessionItemsOf([lane('/r/a', [boss, helper, deep])]);
      const row = (s: Session) => items.find((r) => r.session.key === s.key)!;
      // Its helper listed (every helper of the session shown), the oldest of a full list.
      const others = Array.from({ length: HELPERS_SHOWN_MAX - 1 }, (_, i) => `other${i}`);
      const shown = [boss.key, ...others];
      const listed = placeHelpers(items, false, new Set(shown));
      expect(keysOf(listed)).toEqual([boss.key, helper.key]);
      const next = showingHelper(shown, listed, row(deep), items);
      expect(next).toHaveLength(HELPERS_SHOWN_MAX);
      expect(next.slice(-2)).toEqual([boss.key, runsShownKey(helper.key)]);
      expect(keysOf(placeHelpers(items, false, new Set(next)))).toEqual([boss.key, helper.key, deep.key]);
    });

    it('lists a run or a finished helper a window asks to show, once the search clears', () => {
      const done = session('codex:codex:done', { status: 'idle', helper: { parent: boss.key } });
      const items = sessionItemsOf([lane('/r/a', [boss, busy, ended, asks, done])]);
      const row = (s: Session) => items.find((r) => r.session.key === s.key)!;
      // Its session's runs listed, not its helpers; and its helpers, not its runs.
      expect(revealing(['x'], items, row(busy))).toEqual(['x', runsShownKey(boss.key)]);
      expect(revealing([], items, row(done))).toEqual([boss.key]);
      expect(keysOf(placeHelpers(items, false, new Set(revealing([], items, row(ended)))))).toContain(ended.key);
      // Listed anyway: as it was.
      const shown = ['x'];
      expect(revealing(shown, items, row(asks))).toBe(shown);
      expect(revealing(shown, items, row(boss))).toBe(shown);
      const all = [runsShownKey(boss.key)];
      expect(revealing(all, items, row(busy))).toBe(all);
    });

    it('lists a run a window asks to show when its session is older than the age filter', () => {
      const ages = [
        { id: '1d', ms: DAY },
        { id: '3d', ms: 3 * DAY },
        { id: 'all', ms: 0 },
      ] as const;
      const old = { ...boss, lastActivity: NOW - 2 * DAY };
      const recent = { ...ended, lastActivity: NOW - 60_000 };
      const items = sessionItemsOf([lane('/r/a', [old, recent])]);
      const row = items.find((r) => r.session.key === recent.key)!;
      const chain = chainOf(row, items).map((r) => r.session.key);
      expect(chain).toEqual([recent.key, old.key]);
      // The run alone is let through already: its session needs the filter wider.
      expect(ageListingSession(recent, new Set(), ages, '1d', NOW)).toBeNull();
      const wider = ageListingChain(chainOf(row, items).map((r) => r.session), new Set(), ages, '1d', NOW);
      expect(wider).toBe('3d');
      const shown = new Set(revealing([], items, row));
      expect(keysOf(placeHelpers(shownSessionsOf(items, NOW - 3 * DAY, new Set(), '', []), false, shown, items))).toContain(recent.key);
      expect(ageListingChain([recent, old], new Set(), ages, '3d', NOW)).toBeNull();
    });

    it('lists a run of a finished helper a window asks to show, with the helper', () => {
      const helper = session('codex:codex:helper', { status: 'idle', helper: { parent: boss.key } });
      const run2 = run('claude:claude2:deep', 'idle', { origin: { key: helper.key, how: 'started' } });
      const items = sessionItemsOf([lane('/r/a', [boss, helper, run2])]);
      const row = items.find((r) => r.session.key === run2.key)!;
      expect(keysOf(placeHelpers(items, false))).toEqual([boss.key]);
      const shown = revealing([], items, row);
      expect(shown).toEqual([runsShownKey(helper.key), boss.key]);
      expect(keysOf(placeHelpers(items, false, new Set(shown)))).toEqual([boss.key, helper.key, run2.key]);
      // With as many kept as there can be, the ones it needs stay, its runs' among them, and the
      // oldest others go.
      const others = Array.from({ length: HELPERS_SHOWN_MAX - 1 }, (_, i) => `other${i}`);
      const full = revealing([runsShownKey(helper.key), ...others], items, row);
      expect(full).toHaveLength(HELPERS_SHOWN_MAX);
      expect(full.slice(-2)).toEqual([runsShownKey(helper.key), boss.key]);
      expect(full[0]).toBe('other1');
      expect(keysOf(placeHelpers(items, false, new Set(full)))).toEqual([boss.key, helper.key, run2.key]);
    });

    it('lists a run no session on the board started as a session of its own', () => {
      const lone = run('claude:claude2:lone', 'working', { origin: undefined });
      const gone = run('claude:claude2:gone', 'idle', { origin: { key: 'claude:claude2:closed', how: 'started' } });
      const items = sessionItemsOf([lane('/r/a', [boss, lone, gone])]);
      expect(keysOf(placeHelpers(items, false))).toEqual([boss.key, lone.key, gone.key]);
      expect(runCounts(items).get(boss.key)).toBeUndefined();
    });
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

describe('the Kanban view’s columns', () => {
  /** A board's columns, by id, with the keys of their cards: a project's own board by default. */
  const cols = (todoRows: TodoItem[], todos: Todo[], sessions: SessionItem[], prs: PrItem[], doneSince: number, board: BoardDoc = projectBoard('Acme')) => {
    const subjects = subjectsOf({ todoItems: todoRows, todos, sessionItems: sessions, lanes: [], prs });
    const member = memberTest(board, subjects, NOW);
    return Object.fromEntries(kanbanColumns({ todoRows, todos, sessions, prs, doneSince, board, subjects, member, now: NOW }).map((c) => [c.id, c.cards.map((r) => r.key)]));
  };
  const todoOf = (t: Todo, lanes: Lane[] = []) => todoItem(t, new Map(lanes.map((l) => [l.id, l])), sessionHomes(lanes), new Map());

  it('puts todos where the Todo view lists them: Ready then Waiting under To do, In review by its mark or its PR', () => {
    const open = lane('/r/open', [], { pr: fx.pr({ state: 'open' }) });
    const draft = lane('/r/draft', [], { pr: fx.pr({ state: 'draft' }) });
    const merged = lane('/r/merged', [], { pr: fx.pr({ state: 'merged' }) });
    const lanes = [open, draft, merged];
    const todos = [
      fx.todo({ id: 'wait', dependsOn: ['ready'] }),
      fx.todo({ id: 'ready' }),
      fx.todo({ id: 'doing', status: 'in-progress' }),
      fx.todo({ id: 'marked', status: 'in-progress', review: { note: '', since: NOW } }),
      fx.todo({ id: 'pr', status: 'in-progress', worktrees: ['/r/open'] }),
      fx.todo({ id: 'drafting', status: 'in-progress', worktrees: ['/r/draft'] }),
      fx.todo({ id: 'landed', status: 'in-progress', worktrees: ['/r/merged'] }),
      fx.todo({ id: 'done', status: 'done', doneAt: NOW - DAY }),
      fx.todo({ id: 'long-done', status: 'done', doneAt: NOW - 30 * DAY }),
    ];
    expect(cols(todos.map((t) => todoOf(t, lanes)), todos, [], [], NOW - 7 * DAY)).toEqual({
      todo: ['todo:ready', 'todo:wait'],
      progress: ['todo:doing', 'todo:drafting'],
      review: ['todo:marked', 'todo:pr', 'todo:landed'],
      done: ['todo:done'],
    });
  });

  it('gives a session on no todo a card by its PR and how its turn ended', () => {
    const s = (key: string, over: Partial<Session> = {}) => session(key, over);
    const reviewing = lane('/r/a', [s('claude:claude:pr')], { pr: fx.pr({ state: 'changes' }) });
    const mergedIdle = lane('/r/b', [s('claude:claude:merged', { live: true })], { pr: fx.pr({ state: 'merged' }) });
    const mergedBusy = lane('/r/c', [s('claude:claude:busy', { status: 'working' })], { pr: fx.pr({ state: 'merged' }) });
    const landed = lane('/r/d', [s('claude:claude:landed', { status: 'unread', insight: { landing: 'landed' } }), s('claude:claude:mine', { live: true }), s('claude:claude:helper', { helper: { parent: 'claude:claude:mine' }, status: 'working' })]);
    // One that ended and that you've seen has gone on, unless its PR waits on its reviewers.
    const gone = lane('/r/e', [s('claude:claude:gone'), s('claude:claude:landed-long-ago', { insight: { landing: 'landed' } })]);
    // A session still at work on a done todo has a card of its own; one on an open todo is on its card.
    const doneOn = lane('/r/f', [s('claude:claude:after', { status: 'working' })]);
    const lanes = [reviewing, mergedIdle, mergedBusy, landed, gone, doneOn];
    const todos = [fx.todo({ id: 'open', status: 'in-progress', sessions: ['claude:claude:mine'] }), fx.todo({ id: 'old', status: 'done', doneAt: NOW - 30 * DAY, sessions: ['claude:claude:after'] })];
    const out = cols(todos.map((t) => todoOf(t, lanes)), todos, sessionItemsOf(lanes), [], NOW - 7 * DAY);
    expect(out.review).toEqual(['/r/a::claude:claude:pr']);
    expect(out.done.sort()).toEqual(['/r/b::claude:claude:merged', '/r/d::claude:claude:landed']);
    // A helper at work is its session's.
    expect(out.progress.sort()).toEqual(['/r/c::claude:claude:busy', '/r/f::claude:claude:after', 'todo:open']);
  });

  it('puts what waits on you first in a column, a todo by its most urgent session, the rest by your last prompt', () => {
    const waitedLong = session('claude:claude:a', { status: 'needs', statusSince: NOW - 9000 });
    const waitedLess = session('claude:claude:b', { status: 'needs', statusSince: NOW - 1000 });
    const yourTurn = session('claude:claude:c', { status: 'unread', statusSince: NOW - 5000 });
    const working = session('claude:claude:d', { status: 'working', statusSince: NOW - 5000 });
    const snoozed = session('claude:claude:e', { status: 'needs', statusSince: NOW - 99_000, snoozedUntil: NOW + 60_000, lastPromptAt: NOW - 50_000 });
    const quiet = session('claude:claude:f', { lastPromptAt: NOW - 100 });
    const l = lane('/r/app', [waitedLong, waitedLess, yourTurn, working, snoozed, quiet]);
    const todo = fx.todo({ id: 't', status: 'in-progress', sessions: [quiet.key, waitedLess.key], updatedAt: NOW - 70_000 });
    const lonely = fx.todo({ id: 'lonely', status: 'in-progress', updatedAt: NOW - 20_000 });
    const items = sessionItemsOf([l]);
    const out = cols([todoOf(todo, [l]), todoOf(lonely, [l])], [todo, lonely], items, [], 0);
    expect(out.progress).toEqual(['/r/app::claude:claude:a', 'todo:t', '/r/app::claude:claude:c', '/r/app::claude:claude:d', 'todo:lonely', '/r/app::claude:claude:e']);
  });

  it('holds on a board what its rule takes in and what was added, less what was taken off', () => {
    const l = lane('/r/app', [session('claude:claude:in', { status: 'working' }), session('claude:claude:hand'), session('claude:claude:out', { status: 'working' })]);
    const todos = [fx.todo({ id: 'rel', labels: ['l-rel'] }), fx.todo({ id: 'other' }), fx.todo({ id: 'off', labels: ['l-rel'] })];
    const rows = todos.map((t) => todoOf(t, [l]));
    const board: BoardDoc = { id: 'b-1', name: 'Release', project: 'Acme', rule: { labels: ['l-rel'] }, added: ['session:claude:claude:hand'], removed: ['todo:off'] };
    // A session added by hand has a card though its turn ended and you've seen it.
    expect(cols(rows, todos, sessionItemsOf([l]), [], 0, board)).toEqual({ todo: ['todo:rel'], progress: ['/r/app::claude:claude:hand'], review: [], done: [] });
    const hand: BoardDoc = { ...board, rule: null, added: ['todo:other', 'worktree:/r/app'] };
    // A worktree on the board brings its sessions worth a card, but not one taken off.
    expect(cols(rows, todos, sessionItemsOf([l]), [], 0, hand)).toEqual({ todo: ['todo:other'], progress: ['/r/app::claude:claude:in', '/r/app::claude:claude:out'], review: [], done: [] });
    expect(cols(rows, todos, sessionItemsOf([l]), [], 0, { ...hand, removed: ['session:claude:claude:out'] }).progress).toEqual(['/r/app::claude:claude:in']);
    // Nor on a project's own board, which holds every worktree.
    expect(cols(rows, todos, sessionItemsOf([l]), [], 0, { ...projectBoard('Acme'), removed: ['session:claude:claude:in'] }).progress).toEqual(['/r/app::claude:claude:out']);
  });

  it('puts cards in a board’s own columns: the first whose rule takes it, else where it was put, else the first without a rule', () => {
    const todos = [fx.todo({ id: 'bug', labels: ['l-bug'] }), fx.todo({ id: 'put' }), fx.todo({ id: 'rest' }), fx.todo({ id: 'w', dependsOn: ['rest'] })];
    const rows = todos.map((t) => todoOf(t));
    const columns: BoardColumn[] = [
      { id: 'bugs', name: 'Bugs', rule: { labels: ['l-bug'] }, sort: 'move' },
      { id: 'inbox', name: 'Inbox', sort: 'hand' },
      { id: 'later', name: 'Later', sort: 'hand' },
    ];
    const board: BoardDoc = { id: 'b-1', name: 'Mine', project: 'Acme', rule: {}, added: [], removed: [], columns, placed: { 'todo:put': 'later', 'todo:bug': 'later' }, order: ['todo:rest'] };
    // The board's own order, what waits after the rest.
    expect(cols(rows, todos, [], [], 0, board)).toEqual({ bugs: ['todo:bug'], inbox: ['todo:rest', 'todo:w'], later: ['todo:put'] });
    // With every column ruled, what none takes goes in No column.
    const ruled: BoardDoc = { ...board, columns: [columns[0]] };
    expect(cols(rows, todos, [], [], 0, ruled)).toEqual({ bugs: ['todo:bug'], '': ['todo:put', 'todo:rest', 'todo:w'] });
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

  describe('how far they’ve come from GitHub', () => {
    const own = lane('/r/feature', [], { repoSlug: 'acme/app', branch: 'feature' });
    const searched = lane('/r/web', [], { repoSlug: 'acme/web' });
    const github = { key: 'github', title: 'Signed out of GitHub', text: 'Signed out of GitHub' };
    const of = (lanes: Lane[], over: Partial<Board>) => fx.board({ lanes, ...over });

    it('waits on GitHub’s first poll for a repo only the search for your reviews reads', () => {
      expect(prsRead(of([searched], { githubPolled: false }), 'Acme')).toBe('coming');
      expect(prsRead(of([searched], { githubPolled: true }), 'Acme')).toBe('read');
    });

    it('waits on a repo read in full until its read comes, whether or not the poll has ended', () => {
      expect(prsRead(of([own], { githubPolled: true, githubCurrent: { 'acme/app': null } }), 'Acme')).toBe('coming');
      expect(prsRead(of([own], { githubPolled: true, githubCurrent: { 'acme/app': NOW } }), 'Acme')).toBe('read');
      // Read before the poll that brings the search has ended: its PRs are in.
      expect(prsRead(of([own], { githubPolled: false, githubCurrent: { 'acme/app': NOW } }), 'Acme')).toBe('read');
      // Any of the project's repos not in yet holds the rest.
      expect(prsRead(of([own, searched], { githubPolled: true, githubCurrent: { 'acme/app': NOW } }), 'Acme')).toBe('read');
      expect(prsRead(of([own, searched], { githubPolled: false, githubCurrent: { 'acme/app': NOW } }), 'Acme')).toBe('coming');
    });

    it('goes by the project shown’s repos, every project’s for all of them', () => {
      const other = lane('/r/other', [], { project: 'Other', repoSlug: 'acme/other', branch: 'feature' });
      const board = of([own, other], { githubPolled: true, githubCurrent: { 'acme/app': NOW, 'acme/other': null } });
      expect(prsRead(board, 'Acme')).toBe('read');
      expect(prsRead(board, 'Other')).toBe('coming');
      expect(prsRead(board, ALL_PROJECTS)).toBe('coming');
    });

    it('says GitHub is failing rather than waiting while its notice is up', () => {
      expect(prsRead(of([searched], { githubPolled: false, notices: [github] }), 'Acme')).toBe('failing');
      expect(prsRead(of([searched], { githubPolled: true, notices: [github] }), 'Acme')).toBe('read');
      expect(prsRead(of([searched], { githubPolled: false, notices: [{ ...github, key: 'config' }] }), 'Acme')).toBe('coming');
    });

    it('waits on nothing for a project with no repo on GitHub', () => {
      expect(prsRead(of([], { githubPolled: false }), 'Acme')).toBe('read');
      expect(prsRead(of([lane('/r/local', [], { repoSlug: null })], { githubPolled: false }), 'Acme')).toBe('read');
    });
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

describe('ageListingSession', () => {
  const ages = [
    { id: '1d', ms: DAY },
    { id: '3d', ms: 3 * DAY },
    { id: '7d', ms: 7 * DAY },
    { id: 'all', ms: 0 },
  ] as const;
  const l = lane('/r/a');
  const old = session('claude:claude:old', { lastActivity: NOW - 5 * DAY });

  it('widens the age filter as little as lists a session asked to be shown', () => {
    expect(ageListingSession(old, new Set(), ages, '1d', NOW)).toBe('7d');
    expect(ageListingSession(old, new Set(), ages, '3d', NOW)).toBe('7d');
    expect(ageListingSession(session('claude:claude:older', { lastActivity: NOW - 10 * DAY }), new Set(), ages, '1d', NOW)).toBe('all');
    // What it widens to lists it, by the rule the Sessions view goes by.
    expect(shownSessionsOf([item(l, old)], NOW - 3 * DAY, new Set(), '', [])).toEqual([]);
    expect(shownSessionsOf([item(l, old)], NOW - 7 * DAY, new Set(), '', []).map((r) => r.session.key)).toEqual([old.key]);
  });

  it('leaves the filter alone when it lists the session already', () => {
    expect(ageListingSession(old, new Set(), ages, '7d', NOW)).toBeNull();
    expect(ageListingSession(old, new Set(), ages, 'all', NOW)).toBeNull();
    expect(ageListingSession(session('claude:claude:new', { lastActivity: NOW - DAY / 2 }), new Set(), ages, '1d', NOW)).toBeNull();
    // A pinned session is listed whatever its age.
    expect(ageListingSession(old, new Set([old.key]), ages, '1d', NOW)).toBeNull();
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

describe('sideCounts', () => {
  const glance = (id: string, over: Partial<SideGlance> = {}): SideGlance => ({ id, question: `q ${id}`, asked: 1, running: false, open: false, startedAt: NOW - 1000, lastAt: NOW - 500, ...over });

  it('counts each session’s side conversations as its helpers are, the one answering first and at work', () => {
    const counts = sideCounts({ a: [glance('new'), glance('mid', { running: true, open: true, asked: 3 }), glance('old', { question: '' })], b: [] });
    const a = counts.get('a')!;
    expect([a.count, a.working, a.waiting]).toEqual([3, 1, 0]);
    expect(a.each.map((g) => [g.key, g.title, g.status])).toEqual([
      ['mid', 'q mid', 'working'],
      ['new', 'q new', 'idle'],
      ['old', 'A side conversation', 'idle'],
    ]);
    // A session with none has no chip.
    expect(counts.has('b')).toBe(false);
    expect(sideCounts(undefined).size).toBe(0);
  });
});

describe('familiesOf', () => {
  it('takes a section’s rows a session at a time, with the rows nested under it', () => {
    const l = lane('l');
    const row = (key: string, nested = false): SessionItem => ({ ...item(l, session(key)), nested });
    const fams = familiesOf([row('a'), row('a1', true), row('a2', true), row('b'), row('c'), row('c1', true)]);
    expect(fams.map((f) => [f.head.session.key, f.kids.map((k) => k.session.key)])).toEqual([
      ['a', ['a1', 'a2']],
      ['b', []],
      ['c', ['c1']],
    ]);
  });
});

describe('prCardItem', () => {
  it('finds where a PR lives among the worktrees a window has, as the PRs view does', () => {
    const s = session('claude:me:s1');
    const holds = lane('/r/atlas', [s]);
    const visited = lane('/r/kestrel', [session('claude:me:s2')]);
    const pr = fx.prItem({ number: 7, lanes: [holds.id], sessions: [{ lane: holds.id, key: s.key }] });
    const item = prCardItem(pr, [visited, holds]);
    expect(item.pr).toBe(pr);
    expect(item.lane?.id).toBe(holds.id);
    expect(item.sessions.map((x) => x.session.key)).toEqual([s.key]);
    expect(item.places.map((p) => [p.lane.id, p.holds, p.sessions.length])).toEqual([[holds.id, true, 1]]);
  });
});
