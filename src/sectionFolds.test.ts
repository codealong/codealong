import { describe, expect, it } from 'vitest';
import { hasSessionSections, isSectionFolded, PERSISTED_SECTION_FOLDS, sectionOfRow, sectionOfSession, toggleFoldList, unfoldList, visibleSessionRows, type SessionSectionParts } from './sectionFolds';
import { lane as makeLane, NOW, session as makeSession } from './testing/fixtures';
import { groupSessions, sessionSections, type SessionItem } from './ui/SessionsView';

function item(id: string, over: Parameters<typeof makeSession>[0] = {}, laneId = 'lane-1'): SessionItem {
  const lane = makeLane({ id: laneId, callsign: laneId });
  const session = makeSession({ key: id, title: id, lastActivity: NOW - 60_000, statusSince: NOW - 60_000, ...over });
  return { kind: 'agent', key: `${lane.id}::${session.key}`, lane, session };
}

describe('sectionFolds', () => {
  it('folds active sections only when not searching, and keeps Archived under showArchived', () => {
    const folded = new Set(['working', 'pinned']);
    expect(isSectionFolded('working', folded, false, false)).toBe(true);
    expect(isSectionFolded('working', folded, false, true)).toBe(false);
    expect(isSectionFolded('needs', folded, false, false)).toBe(false);
    // Archived starts folded and stays folded during search unless opened.
    expect(isSectionFolded('archived', folded, false, false)).toBe(true);
    expect(isSectionFolded('archived', folded, false, true)).toBe(true);
    expect(isSectionFolded('archived', folded, true, false)).toBe(false);
  });

  it('persists only ambient sections, never urgent bays or rotating days', () => {
    for (const id of ['orchestrators', 'pinned', 'pinnedGroups', 'working', 'snoozed']) {
      expect(PERSISTED_SECTION_FOLDS.has(id)).toBe(true);
    }
    for (const id of ['needs', 'unread', 'listening', 'today', 'yesterday', 'archived']) {
      expect(PERSISTED_SECTION_FOLDS.has(id)).toBe(false);
    }
  });

  it('toggles and unfolds lists without duplicates', () => {
    expect(toggleFoldList([], 'working')).toEqual(['working']);
    expect(toggleFoldList(['working', 'snoozed'], 'working')).toEqual(['snoozed']);
    expect(unfoldList(['working', 'snoozed'], 'working')).toEqual(['snoozed']);
    expect(unfoldList(['snoozed'], 'working')).toEqual(['snoozed']);
  });

  it('omits rows from folded sections in visibleSessionRows while keeping hasSessionSections true', () => {
    const orch = item('orch', { orchestrator: { schedule: 's1' }, status: 'idle' });
    const pin = item('pin', { status: 'idle' });
    const pinHelper = item('pin-h', { status: 'working', helper: { parent: 'pin' } });
    const needs = item('needs', { status: 'needs' });
    const work = item('work', { status: 'working' });
    const arch = item('arch', { archived: true, status: 'idle' });

    const sections = sessionSections([needs, work], 'status', NOW);
    const parts: SessionSectionParts = {
      orchestrators: [orch],
      listening: [],
      pinned: [pin],
      topHelpers: new Map([['pin', [pinHelper]]]),
      sections,
      groups: null,
      archived: [arch],
    };

    const allOpen = visibleSessionRows(parts, (id) => id === 'archived');
    expect(allOpen.map((r) => r.key)).toEqual([orch.key, pin.key, pinHelper.key, needs.key, work.key]);

    const folded = new Set(['orchestrators', 'pinned', 'working']);
    const someFolded = visibleSessionRows(parts, (id) => isSectionFolded(id, folded, false, false));
    expect(someFolded.map((r) => r.key)).toEqual([needs.key]);

    // Folding every active section leaves no visible rows, yet still reports sections exist.
    const allFolded = visibleSessionRows(parts, () => true);
    expect(allFolded).toEqual([]);
    expect(hasSessionSections(parts, false)).toBe(true);

    // Opening archived adds archived rows at the end.
    const withArchived = visibleSessionRows(parts, (id) => isSectionFolded(id, folded, true, false));
    expect(withArchived.map((r) => r.key)).toEqual([needs.key, arch.key]);
  });

  it('finds the section of any row, both flat and grouped', () => {
    const orch = item('orch', { orchestrator: { schedule: 's1' }, status: 'idle' });
    const pin = item('pin', { status: 'idle' });
    const needs = item('needs', { status: 'needs' }, 'lane-a');
    const work = item('work', { status: 'working' }, 'lane-b');
    const arch = item('arch', { archived: true, status: 'idle' });
    const sections = sessionSections([needs, work], 'status', NOW);

    const flat: SessionSectionParts = {
      orchestrators: [orch],
      listening: [],
      pinned: [pin],
      topHelpers: new Map(),
      sections,
      groups: null,
      archived: [arch],
    };
    expect(sectionOfRow(orch.key, flat)).toBe('orchestrators');
    expect(sectionOfRow(pin.key, flat)).toBe('pinned');
    expect(sectionOfRow(needs.key, flat)).toBe('needs');
    expect(sectionOfRow(work.key, flat)).toBe('working');
    expect(sectionOfRow(arch.key, flat)).toBe('archived');
    expect(sectionOfRow('missing', flat)).toBeNull();

    const groups = groupSessions(sections, 'worktree', {
      pinned: ['lane-a'],
      lanes: [needs.lane, work.lane],
      empty: false,
      opened: new Set(),
      whole: false,
    });
    const grouped: SessionSectionParts = { ...flat, groups };
    expect(sectionOfRow(needs.key, grouped)).toBe('pinnedGroups');
    expect(sectionOfRow('lane-a', grouped)).toBe('pinnedGroups');
    expect(sectionOfRow(work.key, grouped)).toBe('working');
    expect(sectionOfRow('lane-b', grouped)).toBe('working');
  });

  it('determines sectionOfSession for reveal even before the row is listed', () => {
    const opts = { pinnedKeys: new Set(['pin']), sort: 'status' as const, started: 'apart' as const, group: 'none' as const, groupPins: [] as string[], now: NOW };
    const pin = item('pin', { status: 'idle' });
    const helperOfPin = item('pin-h', { status: 'working', helper: { parent: 'pin' } });
    const work = item('work', { status: 'working' });
    const arch = item('arch', { archived: true });
    const orch = item('orch', { orchestrator: { schedule: 's1' } });
    const all = [pin, helperOfPin, work, arch, orch];

    expect(sectionOfSession(arch, all, opts)).toBe('archived');
    expect(sectionOfSession(orch, all, opts)).toBe('orchestrators');
    expect(sectionOfSession(pin, all, opts)).toBe('pinned');
    expect(sectionOfSession(helperOfPin, all, opts)).toBe('pinned');
    expect(sectionOfSession(work, all, opts)).toBe('working');
  });
});
