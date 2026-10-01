import { describe, expect, it } from 'vitest';
import { placePinned, reorderPinned, sameList } from './pins';

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

describe('placePinned', () => {
  it('pins a key at an insertion point among the pins showing', () => {
    const all = ['a', 'b', 'c'];
    expect(placePinned(all, all, 'n', 0)).toEqual(['n', 'a', 'b', 'c']);
    expect(placePinned(all, all, 'n', 1)).toEqual(['a', 'n', 'b', 'c']);
    expect(placePinned(all, all, 'n', 3)).toEqual(['a', 'b', 'c', 'n']);
    expect(placePinned(all, all, 'n', 9)).toEqual(['a', 'b', 'c', 'n']);
  });

  it('keeps pins hidden by the project or filter in their places', () => {
    const all = ['a', 'x', 'b', 'y'];
    expect(placePinned(all, ['a', 'b'], 'n', 1)).toEqual(['a', 'x', 'n', 'b', 'y']);
    // After the last one showing, ahead of the hidden ones that follow it.
    expect(placePinned(all, ['a', 'b'], 'n', 2)).toEqual(['a', 'x', 'b', 'n', 'y']);
  });

  it('puts the first pin showing at the top', () => {
    expect(placePinned([], [], 'n', 0)).toEqual(['n']);
    expect(placePinned(['x', 'y'], [], 'n', 0)).toEqual(['n', 'x', 'y']);
  });

  it('moves a key that is pinned already', () => {
    const all = ['a', 'b', 'c'];
    expect(placePinned(all, all, 'c', 0)).toEqual(['c', 'a', 'b']);
    expect(placePinned(all, all, 'b', 1)).toBe(all);
  });
});

describe('sameList', () => {
  it('compares in order', () => {
    expect(sameList(['a', 'b'], ['a', 'b'])).toBe(true);
    expect(sameList(['a', 'b'], ['b', 'a'])).toBe(false);
    expect(sameList([], ['a'])).toBe(false);
  });
});
