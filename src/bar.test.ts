import { describe, expect, it } from 'vitest';
import { barWidth, elsewhereWaits, fitGroups, NOTCH, notchClip, notchCount, notchSizes, plain, projectWaits, type Metrics } from './bar';
import { ALL_PROJECTS } from './quick';
import type { BarItem } from './types';

// Which sessions, PRs and todos the bar shows, and their order, are the engine's (barfeed.rs, with
// tests).

const NOW = 1_790_200_000_000;

describe('fitGroups', () => {
  const metrics = (needs: number[], unread: number[], working: number[], reviews = 0, extra: Partial<Metrics> = {}): Metrics => ({
    pills: { needs, unread, working },
    fold: { needs: 30, unread: 30, working: 30 },
    mark: 24,
    project: 0,
    snoozed: 0,
    recent: 0,
    reviews,
    todos: 0,
    gap: 4,
    divider: 1,
    padding: 5,
    ...extra,
  });

  it('measures the bar as its styles lay it out', () => {
    // padding 10 + mark 24 + divider 1 + pills 100 + 50, with 3 gaps between 4 children.
    expect(barWidth(fitGroups(metrics([100, 50], [], []), 10_000), metrics([100, 50], [], []))).toBe(10 + 24 + 1 + 150 + 3 * 4);
    expect(barWidth([], metrics([], [], []))).toBe(10 + 24);
  });

  it('folds nothing when there is room', () => {
    expect(fitGroups(metrics([100], [100], [100]), 10_000)).toEqual([
      { group: 'needs', shown: 1, folded: 0 },
      { group: 'unread', shown: 1, folded: 0 },
      { group: 'working', shown: 1, folded: 0 },
    ]);
  });

  it('folds Working from its last pill back, before touching Your turn', () => {
    const m = metrics([100], [100, 100], [100, 100, 100]);
    const full = barWidth(fitGroups(m, 10_000), m);
    const fits = fitGroups(m, full - 60);
    expect(fits.find((f) => f.group === 'working')).toEqual({ group: 'working', shown: 2, folded: 1 });
    expect(fits.find((f) => f.group === 'unread')).toEqual({ group: 'unread', shown: 2, folded: 0 });
    expect(barWidth(fits, m)).toBeLessThanOrEqual(full - 60);
  });

  it('then folds Your turn, then Needs you, down to one pill each', () => {
    const m = metrics([100, 100], [100, 100], [100, 100], 60);
    const fits = fitGroups(m, 400);
    expect(fits.map((f) => [f.shown, f.folded])).toEqual([
      [2, 0],
      [0, 2],
      [0, 2],
    ]);
    const tight = fitGroups(m, 0);
    expect(tight.map((f) => [f.shown, f.folded])).toEqual([
      [0, 2],
      [0, 2],
      [0, 2],
    ]);
  });

  it('puts the project button beside the mark, and the todos pill beside the reviews', () => {
    const m = metrics([100], [], [], 60, { project: 70, todos: 50 });
    // Mark and project button, a divider, the pill, a divider, reviews and todos: 5 pieces and 2
    // dividers make 7 children with 6 gaps.
    expect(barWidth(fitGroups(m, 10_000), m)).toBe(10 + 24 + 70 + 1 + 100 + 1 + 60 + 50 + 6 * 4);
    const todosOnly = metrics([], [], [], 0, { todos: 50 });
    expect(barWidth(fitGroups(todosOnly, 10_000), todosOnly)).toBe(10 + 24 + 1 + 50 + 2 * 4);
  });

  it('keeps empty groups off the bar', () => {
    expect(barWidth(fitGroups(metrics([], [80], []), 10_000), metrics([], [80], []))).toBe(10 + 24 + 1 + 80 + 2 * 4);
  });
});

describe('plain', () => {
  it('drops code marks and fills in times', () => {
    expect(plain('Stuck on `gcloud auth login` for {for:' + (NOW - 3 * 3_600_000) + '}', NOW)).toBe('Stuck on gcloud auth login for 3h');
  });
});

describe('project counts', () => {
  const item = (key: string, group: BarItem['group'], project: string, hidden = false): BarItem =>
    ({ key, group, status: group, project, hidden, title: key, account: 'claude', laneId: key, callsign: key, detail: null, openTarget: '', target: null }) as BarItem;
  const items = [
    item('n1', 'needs', 'Northwind'),
    item('u1', 'unread', 'Northwind'),
    item('w1', 'working', 'Northwind'),
    item('n2', 'needs', 'Kestrel'),
    item('n3', 'needs', 'Kestrel'),
    item('e1', 'needs', 'Elsewhere'),
    item('e2', 'unread', 'Elsewhere', true),
  ];
  const projects = ['Northwind', 'Kestrel', 'Elsewhere'];

  it('counts what needs you and finished turns per project, as the quick panel does', () => {
    expect(projectWaits(items, projects)).toEqual({
      Northwind: { needs: 1, unread: 1 },
      Kestrel: { needs: 2, unread: 0 },
      // Elsewhere's out-of-sight worktrees count only for Elsewhere itself.
      Elsewhere: { needs: 1, unread: 1 },
      [ALL_PROJECTS]: { needs: 4, unread: 1 },
    });
  });

  it('badges the button with what waits in the projects it hides', () => {
    const waits = projectWaits(items, projects);
    expect(elsewhereWaits(waits, projects, 'Northwind')).toEqual({ needs: 3, unread: 1 });
    expect(elsewhereWaits(waits, projects, 'Kestrel')).toEqual({ needs: 2, unread: 2 });
    expect(elsewhereWaits(waits, projects, ALL_PROJECTS)).toEqual({ needs: 0, unread: 0 });
  });
});

describe('the bar docked in the notch', () => {
  // A 14-inch MacBook Pro's notch, and room for pills 900 wide.
  const notch = { width: 185, height: 32 };
  const room = 900;

  it('is the notch with a side on each hand while closed', () => {
    expect(notchSizes(notch, { w: 600, h: 24 }, 60, room).closed).toEqual({ w: 185 + 2 * NOTCH.wing, h: 32 });
  });

  it('opens as wide as the pills or the sides need, never narrower than closed', () => {
    // 620 across, but as much wider on each side in whole points: 621, the closed 273 and 174 a side.
    expect(notchSizes(notch, { w: 600, h: 24 }, 60, room).open).toEqual({ w: 621, h: 32 + 24 + NOTCH.pad });
    expect(notchSizes(notch, { w: 601, h: 24 }, 60, room).open.w).toBe(621);
    // Wide sides beside the camera: room for each on its hand.
    expect(notchSizes(notch, { w: 100, h: 24 }, 200, room).open.w).toBe(185 + 2 * (200 + NOTCH.pad));
    // Nothing below yet: at least the closed width.
    expect(notchSizes(notch, { w: 0, h: 0 }, 0, room).open.w).toBe(185 + 2 * NOTCH.wing);
  });

  it('keeps one window whatever the pills, as wide as they can get and as tall as it opens', () => {
    const a = notchSizes(notch, { w: 300, h: 24 }, 60, room);
    const b = notchSizes(notch, { w: 700, h: 24 }, 60, room);
    expect(a.window).toEqual(b.window);
    // Room for the corners and the shadow, whichever reaches further.
    expect(a.window).toEqual({ w: room + 2 * NOTCH.pad + 2 * Math.max(NOTCH.ear.open, NOTCH.shadow.side), h: a.open.h + NOTCH.shadow.bottom });
  });

  it('shows the notch and its sides through the clip, a little wider under the pointer, and all of it open', () => {
    const s = notchSizes(notch, { w: 600, h: 24 }, 60, room);
    const closed = notchClip(s, 'closed');
    expect(s.open.w - 2 * closed.side).toBe(s.closed.w);
    expect(Number.isInteger(closed.side)).toBe(true);
    expect(s.open.h - closed.bottom).toBe(s.closed.h);
    expect(closed.radius).toBe(NOTCH.radius.closed);
    expect(s.open.w - 2 * notchClip(s, 'hover').side).toBe(s.closed.w + 2 * NOTCH.hover);
    expect(notchClip(s, 'open')).toEqual({ side: 0, bottom: 0, radius: NOTCH.radius.open });
  });

  it('shows the most urgent group beside the camera, in the bar’s order', () => {
    expect(notchCount({ needs: 2, unread: 1, working: 5 })).toEqual({ group: 'needs', n: 2 });
    expect(notchCount({ needs: 0, unread: 1, working: 5 })).toEqual({ group: 'unread', n: 1 });
    expect(notchCount({ needs: 0, unread: 0, working: 5 })).toEqual({ group: 'working', n: 5 });
    expect(notchCount({ needs: 0, unread: 0, working: 0 })).toBeNull();
  });
});
