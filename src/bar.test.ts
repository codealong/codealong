import { describe, expect, it } from 'vitest';
import { barWidth, currentItem, elsewhereWaits, fitGroups, NOTCH, notchClip, notchCount, notchSizes, plain, projectWaits, stepItem, waitingAfter, waitingItems, type Metrics } from './bar';
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
  // A 14-inch MacBook Pro's notch, room for pills 900 wide, and its screen's height.
  const notch = { width: 185, height: 32 };
  const room = 900;
  const screen = 982;
  const sizes = (body: { w: number; h: number }, side: number, screenH = screen) => notchSizes(notch, body, side, room, screenH);

  it('is the notch with a side on each hand while closed', () => {
    expect(sizes({ w: 600, h: 24 }, 60).closed).toEqual({ w: 185 + 2 * NOTCH.wing, h: 32 });
  });

  it('shows its pills as wide as they or the sides need, never narrower than closed', () => {
    // 620 across, but as much wider on each side in whole points: 621, the closed 273 and 174 a side.
    expect(sizes({ w: 600, h: 24 }, 60).pills).toEqual({ w: 621, h: 32 + 24 + NOTCH.pad });
    expect(sizes({ w: 601, h: 24 }, 60).pills.w).toBe(621);
    // Wide sides beside the camera: room for each on its hand.
    expect(sizes({ w: 100, h: 24 }, 200).pills.w).toBe(185 + 2 * (200 + NOTCH.pad));
    // Nothing below yet: at least the closed width.
    expect(sizes({ w: 0, h: 0 }, 0).pills.w).toBe(185 + 2 * NOTCH.wing);
  });

  it('shows a session under the pills, as wide as they are and no narrower than a session reads', () => {
    // Few pills: the session's own width, as much wider than closed on each side in whole points.
    const few = sizes({ w: 300, h: 24 }, 60);
    expect(few.session.w).toBe(561);
    expect((few.session.w - few.pills.w) % 2).toBe(0);
    expect(few.session.h).toBe(few.pills.h + NOTCH.session.height);
    // Many: as wide as the pills.
    const many = sizes({ w: 800, h: 24 }, 60);
    expect(many.session.w).toBe(many.pills.w);
  });

  it('opens a session no taller than the screen has room for, and never too short to read', () => {
    const short = sizes({ w: 300, h: 24 }, 60, 500);
    expect(short.session.h).toBe(500 - NOTCH.session.clear);
    expect(sizes({ w: 300, h: 24 }, 60, 200).session.h).toBe(66 + NOTCH.session.least);
  });

  it('keeps one window whatever shows, as wide as the pills can get and as tall as a session opens', () => {
    const a = sizes({ w: 300, h: 24 }, 60);
    const b = sizes({ w: 700, h: 24 }, 60);
    expect(a.window).toEqual(b.window);
    // Room for the corners and the shadow, whichever reaches further.
    expect(a.window).toEqual({ w: room + 2 * NOTCH.pad + 2 * Math.max(NOTCH.ear.open, NOTCH.shadow.side), h: a.session.h + NOTCH.shadow.bottom });
  });

  it('shows each state through the clip: the notch, a little wider under the pointer, the pills, all of it', () => {
    const s = sizes({ w: 400, h: 24 }, 60);
    const closed = notchClip(s, 'closed');
    expect(s.session.w - 2 * closed.side).toBe(s.closed.w);
    expect(Number.isInteger(closed.side)).toBe(true);
    expect(s.session.h - closed.bottom).toBe(s.closed.h);
    expect(closed.radius).toBe(NOTCH.radius.closed);
    expect(s.session.w - 2 * notchClip(s, 'hover').side).toBe(s.closed.w + 2 * NOTCH.hover);
    const pills = notchClip(s, 'pills');
    expect(s.session.w - 2 * pills.side).toBe(s.pills.w);
    expect(Number.isInteger(pills.side)).toBe(true);
    expect(s.session.h - pills.bottom).toBe(s.pills.h);
    expect(pills.radius).toBe(NOTCH.radius.open);
    expect(notchClip(s, 'session')).toEqual({ side: 0, bottom: 0, radius: NOTCH.radius.session });
  });

  it('shows the most urgent group beside the camera, in the bar’s order', () => {
    expect(notchCount({ needs: 2, reviews: 3, unread: 1, working: 5 })).toEqual({ kind: 'needs', n: 2 });
    expect(notchCount({ needs: 0, reviews: 3, unread: 1, working: 5 })).toEqual({ kind: 'reviews', n: 3 });
    expect(notchCount({ needs: 0, reviews: 0, unread: 1, working: 5 })).toEqual({ kind: 'unread', n: 1 });
    expect(notchCount({ needs: 0, reviews: 0, unread: 0, working: 5 })).toEqual({ kind: 'working', n: 5 });
    expect(notchCount({ needs: 0, reviews: 0, unread: 0, working: 0 })).toBeNull();
  });
});

describe('the session in the notch', () => {
  const item = (key: string, group: BarItem['group'], since: number): BarItem => ({ key, group, status: group === 'needs' ? 'needs' : group, title: key, account: 'claude', laneId: 'l', callsign: 'main', project: 'P', hidden: false, detail: null, since, openTarget: `s|${key}|focus`, target: null, snoozedUntil: null });
  const items = [item('n1', 'needs', 10), item('n2', 'needs', 20), item('u1', 'unread', 30), item('w1', 'working', 5)];
  const keys = (list: BarItem[]) => list.map((i) => i.key);

  it('works through what waits on you, in the bar’s order, and not what’s running', () => {
    expect(keys(waitingItems(items))).toEqual(['n1', 'n2', 'u1']);
    expect(keys(waitingAfter(items, 'n1', new Map()))).toEqual(['n2', 'u1']);
    // From the last, round to the ones skipped; from one that's running, from the top.
    expect(keys(waitingAfter(items, 'u1', new Map()))).toEqual(['n1', 'n2']);
    expect(keys(waitingAfter(items, 'w1', new Map()))).toEqual(['n1', 'n2', 'u1']);
  });

  it('leaves out the ones dealt with until they wait on you anew', () => {
    const handled = new Map([['n2', 20]]);
    expect(keys(waitingAfter(items, 'n1', handled))).toEqual(['u1']);
    const again = items.map((i) => (i.key === 'n2' ? { ...i, since: 40 } : i));
    expect(keys(waitingAfter(again, 'n1', handled))).toEqual(['n2', 'u1']);
  });

  it('opens on the one shown last while it still waits, else the first that waits', () => {
    expect(currentItem(items, null)?.key).toBe('n1');
    expect(currentItem(items, 'u1')?.key).toBe('u1');
    // One that runs now doesn't keep it from what waits.
    expect(currentItem(items, 'w1')?.key).toBe('n1');
    expect(currentItem(items, 'gone')?.key).toBe('n1');
    // Nothing waits: the one shown last, else the bar's first.
    const running = [item('w1', 'working', 5), item('w2', 'working', 6)];
    expect(currentItem(running, 'w2')?.key).toBe('w2');
    expect(currentItem(running, null)?.key).toBe('w1');
    expect(currentItem([], 'w1')).toBeNull();
  });

  it('steps along the bar, stopping at its ends', () => {
    expect(stepItem(items, 'n2', 1)?.key).toBe('u1');
    expect(stepItem(items, 'n2', -1)?.key).toBe('n1');
    expect(stepItem(items, 'n1', -1)?.key).toBe('n1');
    expect(stepItem(items, 'w1', 1)?.key).toBe('w1');
    expect(stepItem(items, 'gone', 1)?.key).toBe('n1');
    expect(stepItem(items, 'gone', -1)?.key).toBe('w1');
    expect(stepItem([], 'n1', 1)).toBeNull();
  });
});
