import { describe, expect, it } from 'vitest';
import { aimAfter, aimAfterChange, aimedAt, aimedAtNothing, aimedByScroll, clickDoes, type Aim } from './aim';

/** The aim after moves over rows, each [row, x, y]. */
const after = (moves: [string | null, number, number][], from: Aim | null = null): Aim | null => moves.reduce<Aim | null>((aim, [key, x, y]) => aimAfter(aim, key, x, y).aim, from);

describe('aim', () => {
  it('does the row the pointer was moved onto', () => {
    const aim = after([
      ['a', 10, 10],
      ['b', 10, 40],
    ]);
    expect(clickDoes(aim, 'b')).toBe(true);
    expect(clickDoes(aim, 'a')).toBe(false);
  });

  it('only selects a row that came under a pointer that stayed put', () => {
    // The list changed and row c slid under the pointer, which the browser reports as a mouse
    // event where the pointer already was.
    const aim = after([
      ['b', 10, 40],
      ['c', 10, 40],
    ]);
    expect(aimAfter(after([['b', 10, 40]]), 'c', 10, 40).moved).toBe(false);
    expect(clickDoes(aim, 'c')).toBe(false);
    expect(clickDoes(aim, 'b')).toBe(true);
  });

  it('does a row once a click has selected it', () => {
    const aim = after([['b', 10, 40]]);
    expect(clickDoes(aim, 'c')).toBe(false);
    const selected = aimedAt('c', 10, 40);
    expect(clickDoes(selected, 'c')).toBe(true);
    // And a row coming under the pointer after that is selected first in turn.
    expect(clickDoes(aimAfter(selected, 'd', 10, 40).aim, 'd')).toBe(false);
  });

  it('does nothing on a click before the pointer has moved', () => {
    expect(clickDoes(null, 'a')).toBe(false);
  });

  it('starts on no row when the list is shown afresh, until the pointer moves', () => {
    const shown = aimedAtNothing(after([['a', 10, 10]]));
    expect(clickDoes(shown, 'a')).toBe(false);
    expect(clickDoes(aimAfter(shown, 'a', 10, 10).aim, 'a')).toBe(false);
    expect(clickDoes(aimAfter(shown, 'a', 11, 10).aim, 'a')).toBe(true);
    expect(aimedAtNothing(null)).toBeNull();
  });

  it('takes a move off every row as aiming at none', () => {
    expect(clickDoes(after([['a', 10, 10], [null, 10, 70]]), 'a')).toBe(false);
  });

  it('does whichever row a scroll brought under the pointer', () => {
    const scrolled = aimedByScroll(after([['a', 10, 10]]));
    expect(clickDoes(scrolled, 'f')).toBe(true);
    // The event the browser sends as the rows scroll under the pointer counts as a move onto f.
    const next = aimAfter(scrolled, 'f', 10, 10);
    expect(next.moved).toBe(true);
    expect(clickDoes(next.aim, 'f')).toBe(true);
    expect(clickDoes(next.aim, 'g')).toBe(false);
  });

  it('forgets a scroll once the list changes by itself, but not the row aimed at', () => {
    const scrolled = aimAfterChange(aimedByScroll(after([['a', 10, 10]])));
    expect(clickDoes(scrolled, 'f')).toBe(false);
    expect(aimAfter(scrolled, 'f', 10, 10).moved).toBe(false);
    const aim = after([['a', 10, 10]]);
    expect(aimAfterChange(aim)).toBe(aim);
    expect(aimAfterChange(null)).toBeNull();
  });
});
