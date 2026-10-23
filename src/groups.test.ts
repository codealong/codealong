import { describe, expect, it } from 'vitest';
import { GROUP_SHOWN, groupRows, groupsOf, placeIn, type GroupOptions } from './groups';
import * as fx from './testing/fixtures';
import type { Lane } from './types';
import { prPlace, type PrRowItem } from './ui/PrsView';

/** A row of a made-up list: its key and the worktree it's in. */
type Item = { key: string; lane: Lane | null };
const lane = (id: string, over: Partial<Lane> = {}) => fx.lane({ id, ...over });
const item = (key: string, l: Lane | null): Item => ({ key, lane: l });
const opts = (over: Partial<GroupOptions<Item>> = {}): GroupOptions<Item> => ({ pinned: [], lanes: [], empty: false, opened: new Set(), whole: false, ...over });
const keysOf = (rows: Item[]) => rows.map((r) => r.key);

describe('groupRows', () => {
  const a = lane('/r/a');
  const b = lane('/r/b');

  it('lists each group once among the sections, under its first row’s heading, the folded sections apart', () => {
    const sections = [
      { id: 'mine', rows: [item('a1', a), item('b1', b)] },
      { id: 'others', rows: [item('a2', a), item('b2', b)] },
      { id: 'done', rows: [item('a3', a)] },
    ];
    const g = groupRows(sections, 'worktree', (r) => placeIn(r.lane, 'worktree'), opts({ apart: (id) => id === 'done' }));
    expect(g.sections.map((s) => [s.id, s.count, s.groups.map((x) => [x.key, keysOf(x.rows)])])).toEqual([
      [
        'mine',
        2,
        [
          ['/r/a', ['a1', 'a2']],
          ['/r/b', ['b1', 'b2']],
        ],
      ],
      ['done', 1, [['/r/a', ['a3']]]],
    ]);
  });

  it('puts the rows with no worktree under a heading of their own, last in each section, never pulled into another', () => {
    const sections = [
      { id: 'mine', rows: [item('x1', null), item('a1', a)] },
      { id: 'others', rows: [item('x2', null), item('b1', b)] },
    ];
    const g = groupRows(sections, 'worktree', (r) => placeIn(r.lane, 'worktree'), opts());
    expect(g.sections.map((s) => [s.id, s.count, s.groups.map((x) => [x.key, x.lane?.id ?? null, keysOf(x.rows)])])).toEqual([
      [
        'mine',
        2,
        [
          ['/r/a', '/r/a', ['a1']],
          ['', null, ['x1']],
        ],
      ],
      [
        'others',
        2,
        [
          ['/r/b', '/r/b', ['b1']],
          ['', null, ['x2']],
        ],
      ],
    ]);
  });

  it('takes the pinned groups out of the sections, but not out of the ones apart', () => {
    const sections = [
      { id: 'mine', rows: [item('a1', a), item('b1', b)] },
      { id: 'done', rows: [item('b2', b)] },
    ];
    const g = groupRows(sections, 'worktree', (r) => placeIn(r.lane, 'worktree'), opts({ pinned: ['/r/b', '/r/gone'], apart: (id) => id === 'done' }));
    expect(g.pinned.map((x) => [x.key, keysOf(x.rows)])).toEqual([['/r/b', ['b1']]]);
    expect(g.sections.map((s) => [s.id, s.groups.map((x) => x.key)])).toEqual([
      ['mine', ['/r/a']],
      ['done', ['/r/b']],
    ]);
    expect(groupsOf(g).map((x) => x.key)).toEqual(['/r/b', '/r/a', '/r/b']);
  });

  it('folds a group’s rows after the first few, never an awake one, and names a repo by its slug', () => {
    const rows = Array.from({ length: GROUP_SHOWN + 2 }, (_, i) => item(`r${i}`, a));
    const g = groupRows([{ id: 'all', rows }], 'repo', (r) => placeIn(r.lane, 'repo'), opts({ awake: ([r]) => r.key === `r${GROUP_SHOWN + 1}` }));
    const [one] = groupsOf(g);
    expect([one.key, one.repo, one.lane]).toEqual(['acme/app', { name: 'app', slug: 'acme/app', project: 'Acme' }, null]);
    expect([keysOf(one.rows).slice(GROUP_SHOWN), one.folded]).toEqual([[`r${GROUP_SHOWN + 1}`], 1]);
  });
});

describe('prPlace', () => {
  const pr = fx.prItem({ repo: 'web', repoSlug: 'acme/web' });
  const row = (l: Lane | null): PrRowItem => ({ kind: 'pr', key: `pr:${pr.key}`, pr, lane: l, sessions: [], places: [] });

  it('puts a PR under the worktree that holds it, or none here; under its repo by its slug either way', () => {
    const held = lane('/r/web-fix', { repo: 'web', repoSlug: 'acme/web' });
    expect(prPlace(row(held), 'worktree')).toMatchObject({ key: '/r/web-fix', lane: held });
    expect(prPlace(row(null), 'worktree')).toEqual({ key: '', lane: null, repo: null });
    expect(prPlace(row(null), 'repo')).toEqual({ key: 'acme/web', lane: null, repo: { name: 'web', slug: 'acme/web', project: 'Acme' } });
    expect(prPlace(row(held), 'repo').key).toBe(prPlace(row(null), 'repo').key);
  });
});
