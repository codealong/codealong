import { describe, expect, it } from 'vitest';
import type { FacetId, FacetOption } from './boardLists';
import type { BoardDoc } from './boards/model';
import { VIEWS, VIEWS_SANS_BOARDS } from './mainViews';
import { ageAppliesIn, listFiltersOf, pickNamesOf, sideBoardsOf, sideFacetsOf, sideFiltersOf, sideStatusesOf, sideViewsOf, viewSuggestionOf, type FiltersOn, type ViewCounts } from './sideLists';

const counts: ViewCounts = { sessions: 12, todos: 4, schedules: 2, prs: 5, prsComing: false, requested: 0 };
const NO_OPTIONS: Record<FacetId, FacetOption[]> = { agents: [], sessionLabels: [], repos: [], prLabels: [], authors: [], labels: [] };
const statuses = sideStatusesOf({ needs: 3, unread: 2, working: 1, review: 0, work: 4 });

const board = (over: Partial<BoardDoc>): BoardDoc => ({ id: 'b1', name: 'Launch', project: 'Northwind', rule: null, added: [], removed: [], ...over });

/** The filters on, with none on but what a test says. */
const on = (over: Partial<FiltersOn> = {}): FiltersOn => ({
  devices: [],
  deviceName: (id) => (id === 'here' ? 'This device' : id === 'm1' ? 'Kestrel' : null),
  ageApplies: true,
  age: 'all',
  statuses: [],
  sideStatuses: statuses,
  facets: [],
  ...over,
});

describe('the sidebar’s views', () => {
  it('lists the Board view once, for Kanban and Canvas both, with the keys of the layout a click on it shows', () => {
    const views = sideViewsOf(VIEWS, 'canvas', counts);
    expect(views.map((v) => [v.id, v.label, v.count])).toEqual([
      ['sessions', 'Sessions', 12],
      ['prs', 'PRs', 5],
      ['todo', 'Todo', 4],
      ['schedules', 'Schedules', 2],
      ['board', 'Board', ''],
    ]);
    expect(views[4].icon).toBe('layout-kanban');
    expect(views[4].kbd).toEqual(['⇧', '⌘', 'B']);
    expect(sideViewsOf(VIEWS, 'kanban', counts)[4].kbd).toEqual(['⇧', '⌘', 'K']);
    expect(views[0].kbd).toEqual(['⇧', '⌘', 'S']);
  });

  it('has no Board view while boards are off', () => {
    expect(sideViewsOf(VIEWS_SANS_BOARDS, 'kanban', counts).map((v) => v.id)).toEqual(['sessions', 'prs', 'todo', 'schedules']);
  });

  it('counts no PRs while GitHub’s are on their way and none is listed, and badges the ones that want your review', () => {
    const prs = (c: Partial<ViewCounts>) => sideViewsOf(VIEWS, 'kanban', { ...counts, ...c })[1];
    expect(prs({ prs: 0, prsComing: true }).count).toBe('');
    expect(prs({ prs: 3, prsComing: true }).count).toBe(3);
    expect(prs({ prs: 0 }).count).toBe(0);
    expect([prs({}).badge, prs({}).badgeTitle]).toEqual([undefined, '0 want your review']);
    expect([prs({ requested: 2 }).badge, prs({ requested: 2 }).badgeTitle]).toEqual([2, '2 want your review']);
    // Only the PRs view has a badge.
    expect(sideViewsOf(VIEWS, 'kanban', { ...counts, requested: 2 })[0].badge).toBeUndefined();
  });
});

describe('the sidebar’s boards', () => {
  const boards = [board({ id: 'b1', name: 'Launch' }), board({ id: 'b2', name: 'Atlas work', project: 'Atlas', rule: { kinds: ['todo'], labels: ['l1'] } })];
  const has = { current: 'Northwind', labels: [{ id: 'l1', name: 'bug', color: 'red' as const }], shown: 'b1', showing: true, count: 7, layoutShown: 'kanban' as const };

  it('says a board’s project where it isn’t the one shown, and what it takes in', () => {
    const list = sideBoardsOf(boards, has);
    expect(list.map((b) => [b.id, b.name, b.project, b.title])).toEqual([
      ['b1', 'Launch', undefined, 'Only what’s added'],
      ['b2', 'Atlas work', 'Atlas', 'In Atlas: Todos, labelled bug'],
    ]);
  });

  it('gives the board shown the view’s keys, and its count while the Board view shows', () => {
    expect(sideBoardsOf(boards, has).map((b) => [b.count, b.kbd])).toEqual([
      [7, ['⇧', '⌘', 'K']],
      [undefined, undefined],
    ]);
    // Another view showing: its keys still go back to it, with no count.
    expect(sideBoardsOf(boards, { ...has, showing: false, layoutShown: 'canvas' }).map((b) => [b.count, b.kbd])).toEqual([
      [undefined, ['⇧', '⌘', 'B']],
      [undefined, undefined],
    ]);
    // No board to show: none has the keys.
    expect(sideBoardsOf(boards, { ...has, shown: undefined }).every((b) => b.count === undefined && b.kbd === undefined)).toBe(true);
  });
});

describe('what narrows a view', () => {
  it('lists the statuses with their counts', () => {
    expect(statuses.map((s) => [s.id, s.label, s.count])).toEqual([
      ['needs', 'Needs you', 3],
      ['unread', 'Your turn', 2],
      ['working', 'Working', 1],
      ['review', 'In review', 0],
      ['work', 'PR needs work', 4],
    ]);
  });

  it('goes by age in every view but Todo, the canvas and Schedules', () => {
    expect(VIEWS.filter((v) => ageAppliesIn(v.id)).map((v) => v.id)).toEqual(['sessions', 'prs', 'kanban']);
  });

  it('offers each view its facets, with the picks among them by name', () => {
    const options = { ...NO_OPTIONS, agents: [{ id: 'codex', label: 'Codex', count: 2 }], repos: [{ id: 'northwind/app', label: 'app', count: 1 }] };
    const sessions = sideFacetsOf('sessions', options, { agents: ['codex', 'gone'], repos: ['northwind/app'] });
    expect(sessions.map((f) => [f.id, f.label, f.picked])).toEqual([
      ['agents', 'Agents', ['codex', 'gone']],
      ['sessionLabels', 'Labels', []],
    ]);
    expect(sessions[0].options).toBe(options.agents);
    // A pick the options don't list is said by its id.
    expect(pickNamesOf(sessions)).toEqual(['Codex', 'gone']);
    expect(pickNamesOf(sideFacetsOf('prs', options, { agents: ['codex'], repos: ['northwind/app'] }))).toEqual(['app']);
    expect(sideFacetsOf('schedules', options, { agents: ['codex'] })).toEqual([]);
  });

  it('says what narrows the view in words, for the rail’s badge', () => {
    expect(sideFiltersOf(on(), [])).toEqual([]);
    expect(sideFiltersOf(on({ age: '3d', statuses: ['needs', 'work'] }), ['Codex'])).toEqual(['last 3 days', 'Needs you', 'PR needs work', 'Codex']);
    // Where the view doesn't go by age, the age isn't said.
    expect(sideFiltersOf(on({ age: '3d', ageApplies: false }), [])).toEqual([]);
  });

  it('lists the same filters as chips, each with the way to take it off', () => {
    const cleared: string[] = [];
    const clear = { device: (id: string) => void cleared.push(`device ${id}`), age: () => void cleared.push('age'), status: (f: string) => void cleared.push(`status ${f}`), facet: (facet: string, pick: string) => void cleared.push(`${facet} ${pick}`) };
    const facets = sideFacetsOf('sessions', { ...NO_OPTIONS, agents: [{ id: 'codex', label: 'Codex', count: 2 }] }, { agents: ['codex'] });
    const chips = listFiltersOf(on({ devices: ['here', 'm1', 'm9'], age: '1d', statuses: ['unread'], facets }), clear);
    expect(chips.map((c) => [c.key, c.label, c.name])).toEqual([
      ['device:here', 'This device', 'what runs on This device'],
      ['device:m1', 'Kestrel', 'what runs on Kestrel'],
      // A device that's gone is said by its id.
      ['device:m9', 'm9', 'what runs on m9'],
      ['age', 'Last 24 hours', 'active in the last 24 hours'],
      ['status:unread', 'Your turn', 'your turn'],
      ['agents:codex', 'Codex', 'Agents: Codex'],
    ]);
    for (const c of chips) c.clear();
    expect(cleared).toEqual(['device here', 'device m1', 'device m9', 'age', 'status unread', 'agents codex']);
    // No chip for the age where the view doesn't go by it.
    expect(listFiltersOf(on({ age: '1d', ageApplies: false }), clear)).toEqual([]);
  });

  it('offers a name for a view as it’s saved: the view, then what narrows it', () => {
    expect(viewSuggestionOf('sessions', 'All projects', on(), [])).toBe('Sessions');
    expect(viewSuggestionOf('prs', 'Northwind', on({ devices: ['here', 'm1'], statuses: ['needs', 'unread'], age: '7d' }), ['app', 'bug'])).toBe('PRs · Northwind · This device + Kestrel · Needs you + Your turn · app + bug · 7d');
    // A device that's gone has no name to say, and Todo doesn't go by age.
    expect(viewSuggestionOf('todo', 'Northwind', on({ devices: ['m9'], age: '7d', ageApplies: false }), [])).toBe('Todo · Northwind');
  });
});
