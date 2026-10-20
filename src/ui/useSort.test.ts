import { describe, expect, it } from 'vitest';
import { dropPlace, moved, shiftOf } from './useSort';

describe('dragging a thing to another place in its line', () => {
  // Four things 100 long with gaps of 8: middles at 50, 158, 266 and 374.
  const edges = [0, 108, 216, 324].map((start) => ({ start, end: start + 100 }));

  it('lands after every thing whose middle it has passed', () => {
    expect(dropPlace(edges, 0, 50)).toBe(0);
    expect(dropPlace(edges, 0, 159)).toBe(1);
    expect(dropPlace(edges, 0, 400)).toBe(3);
    expect(dropPlace(edges, 3, 265)).toBe(2);
    expect(dropPlace(edges, 3, 0)).toBe(0);
  });

  it('moves the things it passes one place toward where it came from', () => {
    // The first dragged to the third place: the second and third move back.
    expect([0, 1, 2, 3].map((i) => shiftOf(i, 0, 2, 108))).toEqual([0, -108, -108, 0]);
    // The last dragged to the second place: the second and third move on.
    expect([0, 1, 2, 3].map((i) => shiftOf(i, 3, 1, 108))).toEqual([0, 108, 108, 0]);
    expect([0, 1, 2, 3].map((i) => shiftOf(i, 2, 2, 108))).toEqual([0, 0, 0, 0]);
  });

  it('puts the item where it lands', () => {
    expect(moved(['a', 'b', 'c', 'd'], 0, 2)).toEqual(['b', 'c', 'a', 'd']);
    expect(moved(['a', 'b', 'c', 'd'], 3, 0)).toEqual(['d', 'a', 'b', 'c']);
    // The same account twice in a routing group, on different models, each where it was put.
    const members = [{ account: 'atlas', model: 'opus' }, { account: 'kestrel' }, { account: 'atlas', model: 'sonnet' }];
    expect(moved(members, 2, 0)).toEqual([{ account: 'atlas', model: 'sonnet' }, { account: 'atlas', model: 'opus' }, { account: 'kestrel' }]);
  });
});
