import { describe, expect, it } from 'vitest';
import { happened, marked, MOMENT_MS, plays } from './mark';
import type { BarFeed, BarItem, BarRecent, BarReview } from './types';

const item = (key: string, group: BarItem['group'], hidden = false): BarItem =>
  ({ key, group, status: group, project: 'Northwind', hidden, title: key, account: 'claude', laneId: key, callsign: key, detail: null, openTarget: '', target: null }) as BarItem;
const feed = (items: BarItem[], more: Partial<BarFeed> = {}): BarFeed =>
  ({ loading: false, theme: 'system', logoStatus: 'idle', perfLog: false, terminal: 'Ghostty', projects: [], items, recent: [], snoozed: [], reviews: [], todos: [], ...more }) as BarFeed;
const after = (was: BarFeed, now: BarFeed) => happened(marked(was), now);

describe('happened', () => {
  it('says nothing when nothing came', () => {
    const f = feed([item('n', 'needs'), item('u', 'unread'), item('w', 'working')]);
    expect(after(f, f)).toBeNull();
    // A pill that goes isn't news.
    expect(after(f, feed([item('w', 'working')]))).toBeNull();
  });

  it('ripples as a turn ends', () => {
    expect(after(feed([item('a', 'working')]), feed([item('a', 'unread')]))).toBe('ripple');
    // Another turn ending, with one unread already.
    expect(after(feed([item('a', 'unread'), item('b', 'working')]), feed([item('a', 'unread'), item('b', 'unread')]))).toBe('ripple');
  });

  it('knocks as a session or a review starts to need you, over anything else', () => {
    expect(after(feed([item('a', 'working'), item('b', 'working')]), feed([item('a', 'needs'), item('b', 'unread')]))).toBe('knock');
    const review = { key: 'acme/app#7' } as BarReview;
    expect(after(feed([]), feed([], { reviews: [review] }))).toBe('knock');
    expect(after(feed([], { reviews: [review] }), feed([], { reviews: [review] }))).toBeNull();
  });

  it('runs as a session shows for the first time, working', () => {
    expect(after(feed([]), feed([item('new', 'working')]))).toBe('run');
    // One the bar knew, back at work, is a turn starting: no moment.
    expect(after(feed([item('a', 'unread')]), feed([item('a', 'working')]))).toBeNull();
    const recent = { ...item('r', 'working'), group: 'recent' } as BarRecent;
    expect(after(feed([], { recent: [recent] }), feed([item('r', 'working')]))).toBeNull();
  });

  it('leaves out sessions in worktrees kept out of sight, as the mark does', () => {
    expect(after(feed([]), feed([item('h', 'unread', true)]))).toBeNull();
    expect(after(feed([]), feed([item('h', 'working', true)]))).toBeNull();
  });
});

describe('plays', () => {
  const at = 1_790_200_000_000;

  it('plays with nothing before it, or once the last has finished and rested', () => {
    expect(plays('run', null, at)).toBe(true);
    expect(plays('run', { kind: 'ripple', at }, at + MOMENT_MS.ripple + 400)).toBe(true);
  });

  it('drops one that comes while another plays, unless it is more urgent', () => {
    expect(plays('ripple', { kind: 'ripple', at }, at + 300)).toBe(false);
    expect(plays('run', { kind: 'ripple', at }, at + MOMENT_MS.ripple + 100)).toBe(false);
    expect(plays('knock', { kind: 'ripple', at }, at + 300)).toBe(true);
    expect(plays('ripple', { kind: 'send', at }, at + 100)).toBe(true);
  });
});
