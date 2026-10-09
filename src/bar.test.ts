import { describe, expect, it } from 'vitest';
import { barWidth, currentRow, elsewhereWaits, fitGroups, NOTCH, notchClip, notchDragged, notchRows, notchShut, notchSizes, plain, projectWaits, ROW_READS, waitingRows, type Metrics, type NotchRow } from './bar';
import { inQueueOrder, placeOf } from './queue';
import { ALL_PROJECTS, QUEUE_READS, queueRows } from './quick';
import * as fx from './testing/fixtures';
import { SESSION_READS } from './useWaitingQueue';
import type { BarItem, BarReview } from './types';

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
  const item = (key: string, group: BarItem['group'], project: string, hidden = false): BarItem => fx.barItem({ key, group, project, hidden });
  const items = [
    item('n1', 'needs', 'Acme'),
    item('u1', 'unread', 'Acme'),
    item('w1', 'working', 'Acme'),
    item('n2', 'needs', 'Globex'),
    item('n3', 'needs', 'Globex'),
    item('e1', 'needs', 'Elsewhere'),
    item('e2', 'unread', 'Elsewhere', true),
  ];
  const projects = ['Acme', 'Globex', 'Elsewhere'];

  it('counts what needs you and finished turns per project, as the quick panel does', () => {
    expect(projectWaits(items, projects)).toEqual({
      Acme: { needs: 1, unread: 1 },
      Globex: { needs: 2, unread: 0 },
      // Elsewhere's out-of-sight worktrees count only for Elsewhere itself.
      Elsewhere: { needs: 1, unread: 1 },
      [ALL_PROJECTS]: { needs: 4, unread: 1 },
    });
  });

  it('badges the button with what waits in the projects it hides', () => {
    const waits = projectWaits(items, projects);
    expect(elsewhereWaits(waits, projects, 'Acme')).toEqual({ needs: 3, unread: 1 });
    expect(elsewhereWaits(waits, projects, 'Globex')).toEqual({ needs: 2, unread: 2 });
    expect(elsewhereWaits(waits, projects, ALL_PROJECTS)).toEqual({ needs: 0, unread: 0 });
  });
});

describe('the bar docked in the notch', () => {
  // A 14-inch MacBook Pro's notch and screen, and room for pills 900 wide.
  const notch = { width: 185, height: 32 };
  const room = 900;
  const screen = { w: 1512, h: 982 };
  const sizes = (body: { w: number; h: number }, side: number, on = screen, want: { w: number; h: number } | null = null) => notchSizes(notch, body, side, room, on, want);

  it('is the notch with a side on each hand while closed, each as wide as what shows there needs', () => {
    expect(sizes({ w: 600, h: 24 }, 60).closed).toEqual({ w: 185 + 2 * NOTCH.wing, h: 32 });
    expect(notchSizes(notch, { w: 600, h: 24 }, 60, room, screen, null, 1, 30).closed.w).toBe(185 + 2 * NOTCH.wing);
    // Two counts side by side: as much wider on both hands, in whole points.
    expect(notchSizes(notch, { w: 600, h: 24 }, 60, room, screen, null, 1, 62.4).closed.w).toBe(185 + 2 * 63);
  });

  it('shows its pills as wide as they or the sides need, never narrower than closed', () => {
    // 620 across, but as much wider on each side in whole points: 621, the closed 317 and 152 a side.
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
    expect(few.session.w).toBe(681);
    expect((few.session.w - few.pills.w) % 2).toBe(0);
    expect(few.session.h).toBe(few.pills.h + NOTCH.session.height);
    // Many: as wide as the pills.
    const many = sizes({ w: 800, h: 24 }, 60);
    expect(many.session.w).toBe(many.pills.w);
  });

  it('opens a session no taller than the screen has room for, and never too short to read', () => {
    const short = sizes({ w: 300, h: 24 }, 60, { w: 1512, h: 500 });
    expect(short.session.h).toBe(500 - NOTCH.session.clear);
    expect(sizes({ w: 300, h: 24 }, 60, { w: 1512, h: 200 }).session.h).toBe(66 + NOTCH.session.least);
  });

  it('takes the size you drag the session to, within what there’s room for', () => {
    const body = { w: 300, h: 24 };
    const dragged = sizes(body, 60, screen, { w: 800, h: 700 });
    expect(dragged.session).toEqual({ w: 801, h: 66 + 700 });
    // No narrower than a session reads or than the pills, no shorter than one reads.
    expect(sizes(body, 60, screen, { w: 100, h: 50 }).session).toEqual({ w: 681, h: 66 + NOTCH.session.least });
    expect(sizes({ w: 800, h: 24 }, 60, screen, { w: 100, h: 520 }).session.w).toBe(821);
    // No bigger than the biggest it gets, which leaves some of the screen clear.
    const far = sizes(body, 60, screen, { w: 5000, h: 5000 });
    expect(far.session).toEqual(far.full);
    expect(far.full).toEqual({ w: 1041, h: 982 - NOTCH.session.clear });
    // A small screen has less room.
    expect(sizes(body, 60, { w: 1000, h: 700 }, { w: 5000, h: 5000 }).session).toEqual({ w: 905, h: 700 - NOTCH.session.clear });
  });

  it('opens a zoomed session as much bigger, within the screen, and leaves a dragged one as it was', () => {
    const body = { w: 300, h: 24 };
    const zoomed = notchSizes(notch, body, 60, room, screen, null, 1.25);
    // 850 across, widened to whole points a side; 650 under the pills.
    expect(zoomed.session).toEqual({ w: 851, h: 66 + 650 });
    // The window stays as it was, so the zoom resizes only the shape inside it.
    expect(zoomed.full).toEqual(sizes(body, 60).full);
    expect(zoomed.window).toEqual(sizes(body, 60).window);
    // Past the most it gets, it grows to the zoomed usual size, within what the screen leaves clear.
    const big = notchSizes(notch, body, 60, room, screen, null, 2);
    expect(big.session.w).toBe(1361);
    expect(big.full.w).toBe(1361);
    expect(notchSizes(notch, body, 60, room, { w: 1200, h: 982 }, null, 2).session.w).toBe(1105);
    expect(notchSizes(notch, body, 60, room, { w: 1512, h: 700 }, null, 2).session.h).toBe(700 - NOTCH.session.clear);
    // A size you dragged it to stays, if it's no narrower than the zoomed usual width.
    expect(notchSizes(notch, body, 60, room, screen, { w: 900, h: 500 }, 1.25).session).toEqual({ w: 901, h: 66 + 500 });
  });

  it('pulls a corner out as far on the other side, the notch staying centered', () => {
    const from = { w: 641, h: 520 };
    expect(notchDragged(from, 'right', 30, 40)).toEqual({ w: 701, h: 560 });
    expect(notchDragged(from, 'left', -30, 40)).toEqual({ w: 701, h: 560 });
    expect(notchDragged(from, 'left', 30, -40)).toEqual({ w: 581, h: 480 });
    // The bottom edge only makes it taller.
    expect(notchDragged(from, 'bottom', 30, 40)).toEqual({ w: 641, h: 560 });
  });

  it('keeps one window whatever shows, as big as a session can be dragged', () => {
    const a = sizes({ w: 300, h: 24 }, 60);
    const b = sizes({ w: 700, h: 24 }, 60, screen, { w: 900, h: 300 });
    expect(a.window).toEqual(b.window);
    // Room for the corners and the shadow, whichever reaches further.
    expect(a.window).toEqual({ w: a.full.w + 2 * Math.max(NOTCH.ear.open, NOTCH.shadow.side), h: a.full.h + NOTCH.shadow.bottom });
    // Never narrower than the pills can get.
    expect(sizes({ w: 300, h: 24 }, 60, { w: 800, h: 700 }).full.w).toBeGreaterThanOrEqual(room + 2 * NOTCH.pad);
  });

  it('shows each state through the clip: the notch, a little wider under the pointer, the pills, the session', () => {
    const s = sizes({ w: 400, h: 24 }, 60);
    const closed = notchClip(s, 'closed');
    expect(s.full.w - 2 * closed.side).toBe(s.closed.w);
    expect(Number.isInteger(closed.side)).toBe(true);
    expect(s.full.h - closed.bottom).toBe(s.closed.h);
    expect(closed.radius).toBe(NOTCH.radius.closed);
    expect(s.full.w - 2 * notchClip(s, 'hover').side).toBe(s.closed.w + 2 * NOTCH.hover);
    const pills = notchClip(s, 'pills');
    expect(s.full.w - 2 * pills.side).toBe(s.pills.w);
    expect(Number.isInteger(pills.side)).toBe(true);
    expect(s.full.h - pills.bottom).toBe(s.pills.h);
    expect(pills.radius).toBe(NOTCH.radius.open);
    const session = notchClip(s, 'session');
    expect(s.full.w - 2 * session.side).toBe(s.session.w);
    expect(Number.isInteger(session.side)).toBe(true);
    expect(s.full.h - session.bottom).toBe(s.session.h);
    expect(session.radius).toBe(NOTCH.radius.session);
  });

  it('counts what waits on you beside the camera while closed, else what runs', () => {
    expect(notchShut({ needs: 3, reviews: 0, unread: 7, working: 1 })).toEqual(['needs', 'unread']);
    expect(notchShut({ needs: 2, reviews: 3, unread: 1, working: 5 })).toEqual(['needs', 'reviews', 'unread']);
    expect(notchShut({ needs: 0, reviews: 3, unread: 0, working: 5 })).toEqual(['reviews']);
    expect(notchShut({ needs: 0, reviews: 0, unread: 0, working: 5 })).toEqual(['working']);
    expect(notchShut({ needs: 0, reviews: 0, unread: 0, working: 0 })).toEqual([]);
  });
});

describe('what the notch works through', () => {
  const item = (key: string, group: BarItem['group'], since: number): BarItem => ({ key, group, status: group === 'needs' ? 'needs' : group, title: key, account: 'claude', laneId: 'l', callsign: 'main', project: 'P', hidden: false, detail: null, since, openTarget: `s|${key}|focus`, target: null, snoozedUntil: null });
  const review = (number: number, since: number): BarReview => ({ key: `o/r#${number}`, number, title: `PR ${number}`, repo: 'r', repoSlug: 'o/r', url: `https://github.com/o/r/pull/${number}`, project: 'P', requestedBy: 'ana', since, updatedAt: since });
  const items = [item('n1', 'needs', 10), item('n2', 'needs', 20), item('u1', 'unread', 30), item('w1', 'working', 5)];
  const reviews = [review(7, 15), review(8, 25)];
  const rows = notchRows(items, reviews);
  const keys = (list: NotchRow[]) => list.map((r) => r.key);

  it('lists the sessions that need you, then the reviews asked of you, then the rest, in the bar’s order', () => {
    expect(keys(rows)).toEqual(['n1', 'n2', 'o/r#7', 'o/r#8', 'u1', 'w1']);
    expect(rows.map((r) => r.kind)).toEqual(['session', 'session', 'review', 'review', 'session', 'session']);
    expect(keys(notchRows(items, []))).toEqual(['n1', 'n2', 'u1', 'w1']);
  });

  it('reads a row by its session or its PR', () => {
    expect(rows.map(ROW_READS.sinceOf)).toEqual([10, 20, 15, 25, 30, 5]);
    expect(ROW_READS.titleOf(rows[2])).toBe('#7 PR 7');
    expect(ROW_READS.titleOf(rows[0])).toBe('n1');
  });

  // How it goes from one to the next, and along the bar, is queue.ts's (queue.test.ts).
  it('works through what waits on you, reviews included, and not what’s running', () => {
    expect(keys(waitingRows(rows))).toEqual(['n1', 'n2', 'o/r#7', 'o/r#8', 'u1']);
  });

  it('works through it in the order the bar lists it, which is the queue’s', () => {
    const waiting = waitingRows(notchRows([item('n1', 'needs', 10), item('u1', 'unread', 30), item('u2', 'unread', 40)], reviews));
    expect(keys(inQueueOrder(waiting, ROW_READS))).toEqual(keys(waiting));
  });

  it('places what waits in the queue as the waiting window and Sessions do', () => {
    const session = fx.session({ key: 'claude:claude:u', status: 'unread', statusSince: 30 });
    const row = queueRows(fx.board({ lanes: [fx.lane({ sessions: [session] })] }), ALL_PROJECTS)[0];
    const pill = notchRows([fx.barItem({ key: session.key, group: 'unread', status: 'unread', since: 30 })], [])[0];
    expect(placeOf(pill, ROW_READS)).toEqual({ key: session.key, waits: 'unread', since: 30 });
    expect(placeOf(row, QUEUE_READS)).toEqual(placeOf(pill, ROW_READS));
    expect(placeOf({ session }, SESSION_READS)).toEqual(placeOf(pill, ROW_READS));
    const pr = fx.prItem({ key: 'acme/app#7', reviewRequested: true, state: 'open', requestedAt: 15, project: 'Acme' });
    const reviewRow = queueRows(fx.board({ prs: [pr] }), ALL_PROJECTS)[0];
    expect(placeOf(reviewRow, QUEUE_READS)).toEqual(placeOf(notchRows([], [fx.barReview({ key: pr.key, since: 15 })])[0], ROW_READS));
  });

  it('opens on the one shown last while it still waits, else the first that waits', () => {
    expect(currentRow(rows, null)?.key).toBe('n1');
    expect(currentRow(rows, 'u1')?.key).toBe('u1');
    expect(currentRow(rows, 'o/r#8')?.key).toBe('o/r#8');
    // One that runs now doesn't keep it from what waits.
    expect(currentRow(rows, 'w1')?.key).toBe('n1');
    expect(currentRow(rows, 'gone')?.key).toBe('n1');
    // Only reviews wait: the first of them.
    expect(currentRow(notchRows([item('w1', 'working', 5)], reviews), null)?.key).toBe('o/r#7');
    // Only finished turns: the one that has waited longest, whatever order they come in.
    expect(currentRow(notchRows([item('u2', 'unread', 40), item('u1', 'unread', 30)], []), null)?.key).toBe('u1');
    // Nothing waits: the one shown last, else the bar's first.
    const running = notchRows([item('w1', 'working', 5), item('w2', 'working', 6)], []);
    expect(currentRow(running, 'w2')?.key).toBe('w2');
    expect(currentRow(running, null)?.key).toBe('w1');
    expect(currentRow([], 'w1')).toBeNull();
  });
});
