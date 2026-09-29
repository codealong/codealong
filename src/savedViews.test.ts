import { describe, expect, it } from 'vitest';
import { parseSaved, sameView, suggestName, type SavedView } from './savedViews';

const saved: SavedView = { id: 'a', name: 'Needs me', view: 'sessions', project: 'All projects', device: null, age: 'all', filter: 'needs' };

describe('saved views', () => {
  it('reads what was stored, leaving out what does not read as a view', () => {
    const raw = JSON.stringify([saved, { ...saved, id: 'b', device: 'm1', filter: null }, { id: 'c', name: 'broken' }, null, 7]);
    expect(parseSaved(raw).map((v) => [v.id, v.device, v.filter])).toEqual([
      ['a', null, 'needs'],
      ['b', 'm1', null],
    ]);
    expect(parseSaved(null)).toEqual([]);
    expect(parseSaved('not json')).toEqual([]);
    expect(parseSaved('{"id":"a"}')).toEqual([]);
  });

  it('knows the view a window shows', () => {
    expect(sameView(saved, { ...saved })).toBe(true);
    expect(sameView(saved, { ...saved, device: 'here' })).toBe(false);
    expect(sameView(saved, { ...saved, filter: null })).toBe(false);
    expect(sameView(saved, { ...saved, age: '7d' })).toBe(false);
  });

  it('names a view by what narrows it', () => {
    expect(suggestName(['Sessions', 'Codealong', null, 'Mac mini', undefined, 'Needs you'])).toBe('Sessions · Codealong · Mac mini · Needs you');
  });
});
