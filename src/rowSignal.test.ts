import { describe, expect, it, vi } from 'vitest';
import type { HelperCount, RunCount, SideCount } from './boardLists';
import { elapsedShort } from './format';
import { ALL_PROJECTS } from './quick';
import { hasActiveHelpers, hasActiveRuns, hasActiveSides, relatedMenuItems, repoChipParts, stripWorkingFor, todoRowLanes, todoRowSessions, worktreeChipParts } from './rowSignal';
import * as fx from './testing/fixtures';

const glance = (status: 'needs' | 'error' | 'working' | 'unread' | 'idle' | 'stopped', key = 'k') => ({
  key,
  title: 'Helper',
  status,
  since: fx.NOW,
  review: null,
  excerpt: null,
  account: 'claude',
  model: null,
  worktree: null,
  reports: false,
});

describe('worktreeChipParts', () => {
  const disposable = fx.lane({ project: 'Northwind', repo: 'northwind', callsign: 'shimmering-greeting-dusk', origin: 'claude-app', reuse: undefined });
  const multiRepoDisposable = fx.lane({ project: 'Northwind', repo: 'sdk', callsign: 'fix-auth', origin: 'script', reuse: undefined });
  const reusableSameRepo = fx.lane({ project: 'Northwind', repo: 'northwind', callsign: 'kestrel', origin: 'script', reuse: 'open' });
  const reusableMultiRepo = fx.lane({ project: 'Northwind', repo: 'sdk', callsign: 'worker-1', origin: 'script', reuse: 'free' });
  const mainSame = fx.lane({ project: 'Atlas', repo: 'atlas', callsign: 'atlas', origin: 'main', reuse: 'open' });
  const mainOther = fx.lane({ project: 'Northwind', repo: 'sdk', callsign: 'sdk', origin: 'main', reuse: 'free' });

  it('drops disposable worktree names and shows project/repo instead', () => {
    // Under All projects, a repo matching its project uses the project casing so the org icon replaces the folder icon.
    expect(worktreeChipParts(disposable, ALL_PROJECTS, { short: true })).toEqual({
      name: 'Northwind',
      beside: ['northwind'],
      repoPrefix: null,
      mainSuffix: false,
      crossProject: true,
      fallbackIcon: 'folder',
    });
    expect(worktreeChipParts(multiRepoDisposable, ALL_PROJECTS, { short: true })).toEqual({
      name: 'sdk',
      beside: ['sdk'],
      repoPrefix: null,
      mainSuffix: false,
      crossProject: true,
      fallbackIcon: 'folder',
    });
    // Inside a single project, shows the repo name without crossProject.
    expect(worktreeChipParts(disposable, 'Northwind', { short: false })).toEqual({
      name: 'northwind',
      beside: ['northwind'],
      repoPrefix: null,
      mainSuffix: false,
      crossProject: false,
      fallbackIcon: 'folder',
    });
  });

  it('keeps the slot name for reusable worktrees and uses repeat as the fallback icon', () => {
    expect(worktreeChipParts(reusableSameRepo, ALL_PROJECTS, { short: true })).toEqual({
      name: 'kestrel',
      beside: ['kestrel'],
      repoPrefix: null,
      mainSuffix: false,
      crossProject: true,
      fallbackIcon: 'repeat',
    });
    // Two-line multi-repo reusable slot keeps repo prefix; one-line drops it.
    expect(worktreeChipParts(reusableMultiRepo, ALL_PROJECTS, { short: false }).repoPrefix).toBe('sdk');
    expect(worktreeChipParts(reusableMultiRepo, ALL_PROJECTS, { short: true }).repoPrefix).toBeNull();
  });

  it('shows / main only in two-line mode and preserves cross-project casing', () => {
    expect(worktreeChipParts(mainSame, ALL_PROJECTS, { short: true })).toMatchObject({ name: 'Atlas', mainSuffix: false, crossProject: true, fallbackIcon: 'folder' });
    expect(worktreeChipParts(mainSame, ALL_PROJECTS, { short: false })).toMatchObject({ name: 'Atlas', mainSuffix: true, crossProject: true, fallbackIcon: 'folder' });
    expect(worktreeChipParts(mainOther, ALL_PROJECTS, { short: false })).toMatchObject({ name: 'sdk', mainSuffix: true, crossProject: true });
    expect(worktreeChipParts(mainSame, 'Atlas', { short: false })).toMatchObject({ name: 'atlas', mainSuffix: true, crossProject: false });
  });

  it('always names the worktree when alwaysName is set (panels & nested elsewhere helpers)', () => {
    expect(worktreeChipParts(disposable, ALL_PROJECTS, { short: true, alwaysName: true })).toMatchObject({
      name: 'shimmering-greeting-dusk',
      beside: ['shimmering-greeting-dusk'],
      fallbackIcon: 'folder',
    });
  });
});

describe('repoChipParts', () => {
  it('uses project casing under All projects when repo matches project', () => {
    expect(repoChipParts('atlas', 'Atlas', ALL_PROJECTS)).toEqual({ name: 'Atlas', crossProject: true });
    expect(repoChipParts('sdk', 'Northwind', ALL_PROJECTS)).toEqual({ name: 'sdk', crossProject: true });
    expect(repoChipParts('atlas', 'Atlas', 'Atlas')).toEqual({ name: 'atlas', crossProject: false });
  });
});

describe('active helper, run and side chip rules', () => {
  it('hides HelpersChip when all helpers are idle or stopped', () => {
    const idleOnly: HelperCount = { count: 2, working: 0, waiting: 0, each: [glance('idle', 'a'), glance('stopped', 'b')] };
    expect(hasActiveHelpers(idleOnly)).toBe(false);
    expect(hasActiveHelpers(undefined)).toBe(false);
    expect(hasActiveHelpers({ count: 1, working: 1, waiting: 0, each: [glance('working')] })).toBe(true);
    expect(hasActiveHelpers({ count: 1, working: 0, waiting: 1, each: [glance('needs')] })).toBe(true);
    expect(hasActiveHelpers({ count: 1, working: 0, waiting: 0, each: [glance('unread')] })).toBe(true);
  });

  it('hides RunsChip when no headless run is working or waiting', () => {
    const doneOnly: RunCount = { count: 4, working: 0, waiting: 0, each: [glance('idle')] };
    expect(hasActiveRuns(doneOnly)).toBe(false);
    expect(hasActiveRuns({ count: 4, working: 1, waiting: 0, each: [glance('working')] })).toBe(true);
    expect(hasActiveRuns({ count: 4, working: 0, waiting: 1, each: [glance('needs')] })).toBe(true);
  });

  it('hides SidesChip when all side conversations are answered and seen', () => {
    const answered: SideCount = { count: 4, working: 0, waiting: 0, each: [glance('idle')] };
    expect(hasActiveSides(answered, false)).toBe(false);
    expect(hasActiveSides(answered, true)).toBe(true);
    expect(hasActiveSides({ count: 4, working: 1, waiting: 0, each: [glance('working')] }, false)).toBe(true);
  });
});

describe('working duration deduplication', () => {
  it('strips leading {for:...} tokens from working detail', () => {
    expect(stripWorkingFor('{for:12345} · running `git status`')).toBe('running `git status`');
    expect(stripWorkingFor('{for:12345}')).toBeNull();
    expect(stripWorkingFor('Read file')).toBe('Read file');
    expect(stripWorkingFor(null)).toBeNull();
  });

  it('formats compact single-unit elapsed times that fit in the age column', () => {
    const m = 60_000;
    const h = 60 * m;
    const d = 24 * h;
    expect(elapsedShort(fx.NOW - 10_000, fx.NOW)).toBe('<1m');
    expect(elapsedShort(fx.NOW - 14 * m, fx.NOW)).toBe('14m');
    expect(elapsedShort(fx.NOW - (16 * h + 58 * m), fx.NOW)).toBe('16h');
    expect(elapsedShort(fx.NOW - (2 * d + 5 * h), fx.NOW)).toBe('2d');
  });
});

describe('todo row filtering', () => {
  it('deduplicates disposable worktrees in the same repo and skips ones duplicating ProjectTag', () => {
    const a = fx.lane({ id: '/a', project: 'Atlas', repo: 'atlas', callsign: 'wt-1', origin: 'script', reuse: undefined });
    const b = fx.lane({ id: '/b', project: 'Atlas', repo: 'atlas', callsign: 'wt-2', origin: 'script', reuse: undefined });
    const sdk1 = fx.lane({ id: '/s1', project: 'Atlas', repo: 'sdk', callsign: 'wt-3', origin: 'script', reuse: undefined });
    const sdk2 = fx.lane({ id: '/s2', project: 'Atlas', repo: 'sdk', callsign: 'wt-4', origin: 'script', reuse: undefined });
    const slot = fx.lane({ id: '/slot', project: 'Atlas', repo: 'atlas', callsign: 'kestrel', origin: 'script', reuse: 'open' });

    // Under All projects, ProjectTag says Atlas, so disposable atlas worktrees are skipped, sdk is deduplicated to 1, and reusable slot stays.
    expect(todoRowLanes([a, b, sdk1, sdk2, slot], 'Atlas', ALL_PROJECTS).map((l) => l.id)).toEqual(['/s1', '/slot']);
    // Under Atlas itself, no ProjectTag is shown, so disposable atlas deduplicates to 1.
    expect(todoRowLanes([a, b, slot], 'Atlas', 'Atlas').map((l) => l.id)).toEqual(['/a', '/slot']);
    // Under a worktree heading, that worktree is skipped.
    expect(todoRowLanes([slot], 'Atlas', ALL_PROJECTS, '/slot')).toEqual([]);
  });

  it('caps one-line todo session chips to 1 (most urgent first) and two-line to 3', () => {
    const idle = { session: fx.session({ key: '1', status: 'idle' }) };
    const working = { session: fx.session({ key: '2', status: 'working' }) };
    const needs = { session: fx.session({ key: '3', status: 'needs' }) };
    const unread = { session: fx.session({ key: '4', status: 'unread' }) };

    const one = todoRowSessions([idle, working, needs, unread], true);
    expect(one.shown.map((x) => x.session.key)).toEqual(['3']);
    expect(one.rest).toHaveLength(3);

    const two = todoRowSessions([idle, working, needs, unread], false);
    expect(two.shown.map((x) => x.session.key)).toEqual(['1', '2', '3']);
    expect(two.rest.map((x) => x.session.key)).toEqual(['4']);
  });
});

describe('relatedMenuItems', () => {
  it('offers show/hide toggles for finished helpers, headless runs and answered side conversations', () => {
    const toggle = vi.fn();
    const helpers: HelperCount = { count: 2, working: 1, waiting: 0, each: [glance('working', 'h1'), glance('idle', 'h2')] };
    const runs: RunCount = { count: 3, working: 0, waiting: 0, each: [glance('idle', 'r1'), glance('idle', 'r2'), glance('idle', 'r3')] };
    const sides: SideCount = { count: 1, working: 0, waiting: 0, each: [glance('idle', 's1')] };

    const items = relatedMenuItems('boss', { helpers, runs, sides }, new Set(), toggle);
    expect(items.map((i) => (i !== 'separator' ? i.label : '-'))).toEqual(['Show all 2 helpers', 'Show 3 headless runs', 'Show side conversation']);

    // When expanded, offers Hide labels and triggers the right keys.
    const expanded = relatedMenuItems('boss', { helpers, runs, sides }, new Set(['boss', 'runs:boss', 'sides:boss']), toggle);
    expect(expanded.map((i) => (i !== 'separator' ? i.label : '-'))).toEqual(['Hide finished helpers', 'Hide headless runs', 'Hide answered side conversations']);
    for (const item of expanded) {
      if (item !== 'separator' && 'run' in item) item.run();
    }
    expect(toggle.mock.calls.map((c) => c[0])).toEqual(['boss', 'runs:boss', 'sides:boss']);

    // When all helpers are already active and not expanded, nothing is hidden so no toggle is needed.
    const activeOnly: HelperCount = { count: 1, working: 1, waiting: 0, each: [glance('working', 'h1')] };
    expect(relatedMenuItems('boss', { helpers: activeOnly }, new Set(), toggle)).toEqual([]);
  });
});
