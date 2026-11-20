import { beforeEach, describe, expect, it } from 'vitest';
import type { MenuAction, MenuSpec } from './contextMenu';
import { parseSaved, sameView, savedViewMenu, suggestName, type SavedView, type ViewState } from './savedViews';

const saved: SavedView = { id: 'a', name: 'Needs me', view: 'sessions', project: 'All projects', device: [], age: 'all', filter: ['needs'] };

describe('saved views', () => {
  it('reads what was stored, leaving out what does not read as a view', () => {
    const raw = JSON.stringify([saved, { ...saved, id: 'b', device: ['here', 'm1'], filter: [] }, { id: 'c', name: 'broken' }, null, 7]);
    expect(parseSaved(raw).map((v) => [v.id, v.device, v.filter])).toEqual([
      ['a', [], ['needs']],
      ['b', ['here', 'm1'], []],
    ]);
    // Saved when one device and one status were all you could pick.
    const old = JSON.stringify([{ ...saved, device: 'm1', filter: null }, { ...saved, id: 'b', device: null, filter: 'work' }]);
    expect(parseSaved(old).map((v) => [v.device, v.filter])).toEqual([
      [['m1'], []],
      [[], ['work']],
    ]);
    // The sidebar's other picks, saved since there were any; what doesn't read is left out.
    const picked = JSON.stringify([{ ...saved, view: 'prs', facets: { repos: ['o/a'], prLabels: [], agents: 'codex', labels: [3] } }]);
    expect(parseSaved(picked)[0].facets).toEqual({ repos: ['o/a'] });
    expect(parseSaved(JSON.stringify([saved]))[0].facets).toBeUndefined();
    expect(parseSaved(null)).toEqual([]);
    expect(parseSaved('not json')).toEqual([]);
    expect(parseSaved('{"id":"a"}')).toEqual([]);
  });

  it('knows the view a window shows', () => {
    expect(sameView(saved, { ...saved })).toBe(true);
    expect(sameView(saved, { ...saved, device: ['here'] })).toBe(false);
    expect(sameView(saved, { ...saved, filter: [] })).toBe(false);
    expect(sameView({ ...saved, filter: ['needs', 'unread'] }, { ...saved, filter: ['unread', 'needs'] })).toBe(true);
    expect(sameView(saved, { ...saved, age: '7d' })).toBe(false);
    // A view saved before facets were is one with none picked.
    expect(sameView(saved, { ...saved, facets: { agents: [] } })).toBe(true);
    expect(sameView(saved, { ...saved, facets: { agents: ['codex'] } })).toBe(false);
    expect(sameView({ ...saved, facets: { repos: ['a', 'b'] } }, { ...saved, facets: { repos: ['b', 'a'], labels: [] } })).toBe(true);
  });

  it('names a view by what narrows it', () => {
    expect(suggestName(['Sessions', 'Codealong', null, 'Mac mini', undefined, 'Needs you'])).toBe('Sessions · Codealong · Mac mini · Needs you');
  });
});

/** A menu as its labels, a line as -; and one of its items. */
const labels = (spec: MenuSpec): string[] => spec.map((i) => (i === 'separator' ? '-' : i.label));
const item = (spec: MenuSpec, label: string): MenuAction => {
  const found = spec.find((i): i is MenuAction => i !== 'separator' && !('items' in i) && i.label === label);
  if (!found) throw new Error(`No ${label} in ${labels(spec).join(', ')}`);
  return found;
};

describe('a saved view’s menu', () => {
  const now: ViewState = { view: 'prs', project: 'Northwind', device: [], age: '3d', filter: ['needs'] };
  const views: SavedView[] = [
    { id: 'a', name: 'Needs me', view: 'sessions', project: 'All projects', device: [], age: 'all', filter: ['needs'] },
    { id: 'b', name: 'Reviews', view: 'prs', project: 'Atlas', device: [], age: 'all', filter: [] },
  ];
  let kept: SavedView[][] = [];
  beforeEach(() => {
    kept = [];
  });
  const menu = (saved: boolean) => savedViewMenu(views[1], { views, now, saved, keep: (next) => void kept.push(next) });

  it('updates the view to what the window shows, keeping its name and its place, and deletes it', () => {
    expect(labels(menu(false))).toEqual(['Update to this view', '-', 'Delete']);
    item(menu(false), 'Update to this view').run();
    item(menu(false), 'Delete').run();
    expect(kept).toEqual([[views[0], { ...views[1], ...now }], [views[0]]]);
    expect(kept[0][1]).toMatchObject({ id: 'b', name: 'Reviews', project: 'Northwind', age: '3d' });
  });

  it('can’t update to a view that’s saved already', () => {
    expect([item(menu(false), 'Update to this view').enabled, item(menu(true), 'Update to this view').enabled]).toEqual([true, false]);
  });
});
