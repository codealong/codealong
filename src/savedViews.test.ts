import { describe, expect, it } from 'vitest';
import { parseSaved, sameView, suggestName, type SavedView } from './savedViews';

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
