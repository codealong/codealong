import { describe, expect, it } from 'vitest';
import { nextAfter, pickTo, pickToggle, selectHow } from './picks';

describe('picking several rows', () => {
  const order = ['a', 'b', 'c', 'd', 'e'];

  it('⇧ picks the rows from the anchor, which stays put', () => {
    const p = pickTo(order, null, 'b', 'd');
    expect(p).toEqual({ focus: 'd', anchor: 'b', keys: ['b', 'c', 'd'] });
    // Back up past the anchor: the range turns around it.
    expect(pickTo(order, p, 'd', 'a')).toEqual({ focus: 'a', anchor: 'b', keys: ['a', 'b'] });
    // Nothing selected before: the row alone.
    expect(pickTo(order, null, null, 'c').keys).toEqual(['c']);
  });

  it('⌘ adds a row or leaves it out, and the keyboard stays on a picked row', () => {
    const p = pickToggle(order, null, 'b', 'd');
    expect(p).toEqual({ focus: 'd', anchor: 'd', keys: ['b', 'd'] });
    const q = pickToggle(order, p, 'd', 'a');
    expect(q.keys).toEqual(['b', 'd', 'a']);
    // Leaving out another row keeps the keyboard where it is.
    expect(pickToggle(order, q, 'a', 'd')).toEqual({ focus: 'a', anchor: 'a', keys: ['b', 'a'] });
    // Leaving out its own row moves it to the next picked row, else the one before.
    expect(pickToggle(order, { focus: 'b', anchor: 'b', keys: ['a', 'b', 'd'] }, 'b', 'b').focus).toBe('d');
    expect(pickToggle(order, { focus: 'd', anchor: 'd', keys: ['a', 'b', 'd'] }, 'd', 'd').focus).toBe('b');
    // The last one can't be left out: something stays selected.
    expect(pickToggle(order, null, 'c', 'c').keys).toEqual(['c']);
  });

  it('goes by the keys held, ⌘ before ⇧', () => {
    expect(selectHow({ metaKey: false, shiftKey: false })).toBe('one');
    expect(selectHow({ metaKey: true, shiftKey: false })).toBe('toggle');
    expect(selectHow({ metaKey: false, shiftKey: true })).toBe('range');
    expect(selectHow({ metaKey: true, shiftKey: true })).toBe('toggle');
  });

  it('selects the row after the ones that left, else the one before them', () => {
    expect(nextAfter(order, new Set(['b', 'c']))).toBe('d');
    // Picked apart: after the last of them, else before the first.
    expect(nextAfter(order, new Set(['a', 'c']))).toBe('d');
    expect(nextAfter(order, new Set(['b', 'e']))).toBe('a');
    expect(nextAfter(order, new Set(['d', 'e']))).toBe('c');
    expect(nextAfter(order, new Set(order))).toBeNull();
  });
});
