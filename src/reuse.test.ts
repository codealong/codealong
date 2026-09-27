import { describe, expect, it } from 'vitest';
import { reuse } from './reuse';

describe('reuse', () => {
  it('returns the previous object when nothing changed', () => {
    const prev = { lanes: [{ id: 'a', n: 1, tags: ['x'] }], at: 1 };
    const next = JSON.parse(JSON.stringify(prev));
    expect(reuse(prev, next)).toBe(prev);
  });

  it('keeps unchanged items and replaces changed ones', () => {
    const prev = { lanes: [{ id: 'a', n: 1 }, { id: 'b', n: 2 }], at: 1 };
    const next = { lanes: [{ id: 'a', n: 1 }, { id: 'b', n: 3 }], at: 1 };
    const out = reuse(prev, next);
    expect(out).toEqual(next);
    expect(out).not.toBe(prev);
    expect(out.lanes[0]).toBe(prev.lanes[0]);
    expect(out.lanes[1]).not.toBe(prev.lanes[1]);
  });

  it('matches moved items by key or id', () => {
    const prev = { list: [{ key: 'a', v: 1 }, { key: 'b', v: 2 }] };
    const next = { list: [{ key: 'b', v: 2 }, { key: 'a', v: 1 }] };
    const out = reuse(prev, next);
    expect(out).toEqual(next);
    expect(out.list[0]).toBe(prev.list[1]);
    expect(out.list[1]).toBe(prev.list[0]);
    expect(out.list).not.toBe(prev.list);
  });

  it('notices added, removed and changed fields', () => {
    const prev: Record<string, unknown> = { a: 1, b: 2 };
    expect(reuse(prev, { a: 1 })).toEqual({ a: 1 });
    expect(reuse(prev, { a: 1, b: 2, c: 3 })).toEqual({ a: 1, b: 2, c: 3 });
    expect(reuse(prev, { a: 1, b: null })).toEqual({ a: 1, b: null });
    expect(reuse({ a: [1, 2] }, { a: [1] })).toEqual({ a: [1] });
    expect(reuse({ a: [1] }, { a: { 0: 1 } })).toEqual({ a: { 0: 1 } });
  });
});
