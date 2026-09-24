import { describe, expect, it } from 'vitest';
import { reorderPinned, sameList } from './pins';

describe('reorderPinned', () => {
  it('moves a pin to an insertion point', () => {
    const all = ['a', 'b', 'c'];
    expect(reorderPinned(all, all, 'a', 2)).toEqual(['b', 'a', 'c']);
    expect(reorderPinned(all, all, 'c', 0)).toEqual(['c', 'a', 'b']);
    expect(reorderPinned(all, all, 'b', 3)).toEqual(['a', 'c', 'b']);
  });

  it('leaves the list alone for a move to where it already is', () => {
    const all = ['a', 'b', 'c'];
    expect(reorderPinned(all, all, 'a', -1)).toBe(all);
    expect(reorderPinned(all, all, 'b', 1)).toBe(all);
    expect(reorderPinned(all, all, 'b', 2)).toBe(all);
    expect(reorderPinned(all, all, 'c', 9)).toBe(all);
  });

  it('keeps pins hidden by the project or filter in their places', () => {
    expect(reorderPinned(['a', 'x', 'b', 'c'], ['a', 'b', 'c'], 'c', 0)).toEqual(['c', 'x', 'a', 'b']);
  });

  it('works on worktree paths the same way', () => {
    const all = ['/r/app3', '/r/main', '/r/quarry'];
    expect(reorderPinned(all, ['/r/app3', '/r/quarry'], '/r/quarry', 0)).toEqual(['/r/quarry', '/r/main', '/r/app3']);
  });
});

describe('sameList', () => {
  it('compares in order', () => {
    expect(sameList(['a', 'b'], ['a', 'b'])).toBe(true);
    expect(sameList(['a', 'b'], ['b', 'a'])).toBe(false);
    expect(sameList([], ['a'])).toBe(false);
  });
});
