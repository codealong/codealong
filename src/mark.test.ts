import { describe, expect, it } from 'vitest';
import { happened, marked, MOMENT_MS, plays } from './mark';
import * as fx from './testing/fixtures';
import type { BarFeed, BarItem, BarRecent } from './types';

const item = (key: string, group: BarItem['group'], hidden = false): BarItem => fx.barItem({ key, group, hidden });
const feed = (items: BarItem[], more: Partial<BarFeed> = {}): BarFeed => fx.barFeed({ items, ...more });
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
    const review = fx.barReview({ key: 'acme/app#7' });
    expect(after(feed([]), feed([], { reviews: [review] }))).toBe('knock');
    expect(after(feed([], { reviews: [review] }), feed([], { reviews: [review] }))).toBeNull();
  });

  it('runs as a session shows for the first time, working', () => {
    expect(after(feed([]), feed([item('new', 'working')]))).toBe('run');
    // One the bar knew, back at work, is a turn starting: no moment.
    expect(after(feed([item('a', 'unread')]), feed([item('a', 'working')]))).toBeNull();
    const recent: BarRecent = { ...item('r', 'working'), group: 'recent' };
    expect(after(feed([], { recent: [recent] }), feed([item('r', 'working')]))).toBeNull();
  });

  it('leaves out sessions in worktrees kept out of sight, as the mark does', () => {
    expect(after(feed([]), feed([item('h', 'unread', true)]))).toBeNull();
    expect(after(feed([]), feed([item('h', 'working', true)]))).toBeNull();
  });

  it('takes another device’s sessions as news, but not what the device brings as it connects again', () => {
    const there = { devices: ['abc'] };
    expect(after(feed([item('@abc:a', 'working')], there), feed([item('@abc:a', 'needs')], there))).toBe('knock');
    // Back after being away: what it shows came with it.
    expect(after(feed([]), feed([item('@abc:a', 'needs'), item('@abc:b', 'working')], there))).toBeNull();
    expect(after(feed([], { devices: ['def'] }), feed([item('@abc:a', 'unread')], { devices: ['abc', 'def'] }))).toBeNull();
    // This device's still counts meanwhile.
    expect(after(feed([]), feed([item('@abc:a', 'needs'), item('u', 'unread')], there))).toBe('ripple');
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
