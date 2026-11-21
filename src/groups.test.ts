import { describe, expect, it } from 'vitest';
import { GROUP_SHOWN, groupRows, groupsOf, noneLabel, placeIn, reorderGroup, type GroupOptions } from './groups';
import { groupCollapseKey, groupOrdersCodec, isGroupCollapsed } from './groupPrefs';
import * as fx from './testing/fixtures';
import type { Lane } from './types';
import { prPlace, type PrRowItem } from './ui/PrsView';
import { todoPlace } from './ui/TodoView';
import type { TodoItem } from './todos';

/** A row of a made-up list: its key and the worktree it's in. */
type Item = { key: string; lane: Lane | null };
const lane = (id: string, over: Partial<Lane> = {}) => fx.lane({ id, callsign: id.split('/').pop() ?? id, ...over });
const item = (key: string, l: Lane | null): Item => ({ key, lane: l });
const opts = (over: Partial<GroupOptions<Item>> = {}): GroupOptions<Item> => ({ opened: new Set(), whole: false, ...over });
const keysOf = (rows: Item[]) => rows.map((r) => r.key);

describe('groupRows', () => {
  const alpha = lane('/r/alpha', { project: 'Alpha', repo: 'web', repoSlug: 'alpha/web' });
  const beta = lane('/r/beta', { project: 'Beta', repo: 'api', repoSlug: 'beta/api' });

  it('merges non-apart sections into one alphabetical group list with ordering underneath each group, keeping apart sections separate', () => {
    // Beta has the urgent row ('mine'), Alpha only has later rows ('others') — Alpha still sorts
    // before Beta alphabetically so sorting orders rows underneath groups, not the groups themselves.
    const sections = [
      { id: 'mine', rows: [item('b1', beta), item('a1', alpha)] },
      { id: 'others', rows: [item('a2', alpha), item('b2', beta)] },
      { id: 'done', rows: [item('b3', beta), item('a3', alpha)] },
    ];
    const g = groupRows(sections, 'worktree', (r) => placeIn(r.lane, 'worktree'), opts({ apart: (id) => id === 'done' }));
    expect(g.groups.map((x) => [x.key, keysOf(x.rows)])).toEqual([
      ['/r/alpha', ['a1', 'a2']],
      ['/r/beta', ['b1', 'b2']],
    ]);
    expect(g.apart.map((s) => [s.id, s.count, s.groups.map((x) => [x.key, keysOf(x.rows)])])).toEqual([
      [
        'done',
        2,
        [
          ['/r/alpha', ['a3']],
          ['/r/beta', ['b3']],
        ],
      ],
    ]);
    expect(groupsOf(g).map((x) => x.key)).toEqual(['/r/alpha', '/r/beta', '/r/alpha', '/r/beta']);
  });

  it('collects rows with no group into one none group, always last', () => {
    const sections = [
      { id: 'mine', rows: [item('x1', null), item('b1', beta)] },
      { id: 'others', rows: [item('x2', null), item('a1', alpha)] },
    ];
    const g = groupRows(sections, 'worktree', (r) => placeIn(r.lane, 'worktree'), opts());
    expect(g.groups.map((x) => [x.key, x.lane?.id ?? null, keysOf(x.rows)])).toEqual([
      ['/r/alpha', '/r/alpha', ['a1']],
      ['/r/beta', '/r/beta', ['b1']],
      ['', null, ['x1', 'x2']],
    ]);
  });

  it('respects custom saved order first, then sorts new groups alphabetically, keeping none last', () => {
    const gamma = lane('/r/gamma', { project: 'Gamma' });
    const sections = [{ id: 'all', rows: [item('a1', alpha), item('b1', beta), item('g1', gamma), item('x1', null)] }];
    const g = groupRows(sections, 'worktree', (r) => placeIn(r.lane, 'worktree'), opts({ order: ['/r/gamma', '/r/gone'] }));
    expect(g.groups.map((x) => x.key)).toEqual(['/r/gamma', '/r/alpha', '/r/beta', '']);
  });

  it('groups by project, putting chats with no project under none', () => {
    const chat = lane('/chats', { project: 'No project', startsChats: true });
    const sections = [{ id: 'all', rows: [item('b1', beta), item('c1', chat), item('a1', alpha)] }];
    const g = groupRows(sections, 'project', (r) => placeIn(r.lane, 'project'), opts());
    expect(g.groups.map((x) => [x.key, x.project, keysOf(x.rows)])).toEqual([
      ['Alpha', 'Alpha', ['a1']],
      ['Beta', 'Beta', ['b1']],
      ['', null, ['c1']],
    ]);
  });

  it('folds a group’s rows after the first few, never an awake one, and names a repo by its slug', () => {
    const rows = Array.from({ length: GROUP_SHOWN + 2 }, (_, i) => item(`r${i}`, alpha));
    const g = groupRows([{ id: 'all', rows }], 'repo', (r) => placeIn(r.lane, 'repo'), opts({ awake: ([r]) => r.key === `r${GROUP_SHOWN + 1}` }));
    const [one] = groupsOf(g);
    expect([one.key, one.repo, one.lane, one.project]).toEqual(['alpha/web', { name: 'web', slug: 'alpha/web', project: 'Alpha' }, null, null]);
    expect([keysOf(one.rows).slice(GROUP_SHOWN), one.folded]).toEqual([[`r${GROUP_SHOWN + 1}`], 1]);
    expect(keysOf(one.all)).toEqual(keysOf(rows));
  });

  it('keeps apart sections’ and different groupings’ fold keys independent', () => {
    const many = (prefix: string) => Array.from({ length: GROUP_SHOWN + 1 }, (_, i) => item(`${prefix}${i}`, null));
    const sections = [
      { id: 'mine', rows: [...many('m'), item('a1', alpha)] },
      { id: 'done', rows: [...many('d'), item('a2', alpha)] },
    ];
    const g = groupRows(sections, 'worktree', (r) => placeIn(r.lane, 'worktree'), opts({ apart: (id) => id === 'done', opened: new Set(['main/worktree/']) }));
    expect(groupsOf(g).map((x) => [x.fold, x.folded, x.opened])).toEqual([
      ['worktree//r/alpha', 0, false],
      ['main/worktree/', 0, true],
      ['done/worktree//r/alpha', 0, false],
      ['done/worktree/', 1, false],
    ]);
    const byRepo = groupRows([{ id: 'all', rows: many('n') }], 'repo', (r) => placeIn(r.lane, 'repo'), opts({ opened: new Set(['main/worktree/']) }));
    expect(groupsOf(byRepo).map((x) => [x.folded, x.opened])).toEqual([[1, false]]);
  });

  it('returns the right empty-group label for each grouping kind', () => {
    expect(noneLabel('project', 'No worktree here')).toBe('No project');
    expect(noneLabel('repo', 'No worktree here')).toBe('No repo');
    expect(noneLabel('worktree', 'No worktree here')).toBe('No worktree here');
  });
});

describe('reorderGroup', () => {
  it('records full order from alphabetical default and preserves hidden keys across subsequent drags', () => {
    expect(reorderGroup(['a', 'b', 'c'], [], 'c', 0)).toEqual(['c', 'a', 'b']);
    expect(reorderGroup(['a', 'b', 'c'], [], 'a', 3)).toEqual(['b', 'c', 'a']);
    // Hidden keys ('x', 'y') keep their relative slots when visible keys reorder.
    expect(reorderGroup(['a', 'b', 'c'], ['x', 'a', 'y', 'b', 'c'], 'c', 0)).toEqual(['x', 'c', 'y', 'a', 'b']);
    // New visible key ('c') not yet in saved order merges cleanly.
    expect(reorderGroup(['b', 'a', 'c'], ['x', 'b', 'a'], 'c', 0)).toEqual(['x', 'c', 'b', 'a']);
    // No-op move leaves saved unchanged.
    expect(reorderGroup(['a', 'b'], ['a', 'b'], 'a', 0)).toEqual(['a', 'b']);
  });
});

describe('groupPrefs helpers', () => {
  it('parses groupOrders safely and expands collapsed groups during search', () => {
    expect(groupOrdersCodec.parse('{"project":["B","A"],"repo":[1,"r"],"worktree":null}')).toEqual({ project: ['B', 'A'], repo: ['r'], worktree: [] });
    expect(groupOrdersCodec.parse('[]')).toBeUndefined();
    const collapsed = new Set([groupCollapseKey('sessions', 'project/Alpha')]);
    expect(isGroupCollapsed('sessions', 'project/Alpha', collapsed, false)).toBe(true);
    expect(isGroupCollapsed('sessions', 'project/Alpha', collapsed, true)).toBe(false);
    expect(isGroupCollapsed('prs', 'project/Alpha', collapsed, false)).toBe(false);
  });
});

describe('prPlace & todoPlace', () => {
  const pr = fx.prItem({ project: 'Acme', repo: 'web', repoSlug: 'acme/web' });
  const prRow = (l: Lane | null): PrRowItem => ({ kind: 'pr', key: `pr:${pr.key}`, pr, lane: l, sessions: [], places: [], primary: null });

  it('places PRs by worktree, repo or project', () => {
    const held = lane('/r/web-fix', { project: 'Acme', repo: 'web', repoSlug: 'acme/web' });
    expect(prPlace(prRow(held), 'worktree')).toMatchObject({ key: '/r/web-fix', lane: held });
    expect(prPlace(prRow(null), 'worktree')).toEqual({ key: '', lane: null, repo: null, project: null });
    expect(prPlace(prRow(null), 'repo')).toEqual({ key: 'acme/web', lane: null, repo: { name: 'web', slug: 'acme/web', project: 'Acme' }, project: null });
    expect(prPlace(prRow(null), 'project')).toEqual({ key: 'Acme', lane: null, repo: null, project: 'Acme' });
  });

  it('places todos by project even without a worktree, and by worktree or repo via their lane', () => {
    const todo = fx.todo({ id: 't1', project: 'Northwind' });
    const row: TodoItem = { kind: 'todo', key: 't1', todo, labels: [], lane: null, primary: null, sessions: [], places: [], lastActivity: fx.NOW };
    expect(todoPlace(row, 'project')).toEqual({ key: 'Northwind', lane: null, repo: null, project: 'Northwind' });
    expect(todoPlace(row, 'worktree')).toEqual({ key: '', lane: null, repo: null, project: null });
    const bare: TodoItem = { ...row, todo: fx.todo({ id: 't2', project: '' }) };
    expect(todoPlace(bare, 'project')).toEqual({ key: '', lane: null, repo: null, project: null });
  });
});
