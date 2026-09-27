import { describe, expect, it } from 'vitest';
import { looseEqual } from './memoRow';

describe('looseEqual', () => {
  const lane = { id: 'a' };
  const session = { key: 's' };

  it('sees through fresh wrappers around the same objects', () => {
    const a = { row: { key: 'a', lane, agents: [{ kind: 'agent', lane, session }] }, now: 1 };
    const b = { row: { key: 'a', lane, agents: [{ kind: 'agent', lane, session }] }, now: 1 };
    expect(looseEqual(a, b, 4)).toBe(true);
  });

  it('notices a changed object inside, even an equal copy', () => {
    const a = { row: { lane, agents: [{ session }] } };
    expect(looseEqual(a, { row: { lane, agents: [{ session: { key: 's' } }] } }, 4)).toBe(false);
    expect(looseEqual(a, { row: { lane, agents: [] } }, 4)).toBe(false);
    expect(looseEqual({ now: 1 }, { now: 2 }, 4)).toBe(false);
    expect(looseEqual({ a: 1 }, { a: 1, b: undefined }, 4)).toBe(false);
  });

  it('counts any two functions as equal', () => {
    expect(looseEqual({ onOpen: () => 1 }, { onOpen: () => 2 }, 4)).toBe(true);
  });

  it('stops looking past its depth', () => {
    expect(looseEqual({ x: { y: { z: 1 } } }, { x: { y: { z: 1 } } }, 2)).toBe(false);
    expect(looseEqual({ x: { y: { z: 1 } } }, { x: { y: { z: 1 } } }, 3)).toBe(true);
  });

  it('compares only plain objects and arrays by content', () => {
    expect(looseEqual({ m: new Map() }, { m: new Map() }, 4)).toBe(false);
    expect(looseEqual([1], { 0: 1 }, 4)).toBe(false);
  });
});
