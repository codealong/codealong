import { describe, expect, it } from 'vitest';
import { looseEqual, stoodIn } from './memoRow';

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

describe('stoodIn', () => {
  const stand = (key: string, field?: string) => Object.assign(() => undefined, { path: field ? `${key}.${field}` : key });
  const path = (f: unknown) => (f as { path?: string }).path;

  it('stands in for callbacks, and for callbacks in props passed together', () => {
    const lane = { id: 'a' };
    const props = { row: { lane }, now: 1, onOpen: () => 1, drag: { pinIndex: 2, onPointerDown: () => 2, extraClass: 'is-drag-source' } };
    const out = stoodIn(props, [], stand);
    expect(path(out.onOpen)).toBe('onOpen');
    const drag = out.drag as typeof props.drag;
    expect(path(drag.onPointerDown)).toBe('drag.onPointerDown');
    expect(drag.pinIndex).toBe(2);
    expect(drag.extraClass).toBe('is-drag-source');
    // What holds no callback is passed as it is, so the row's own data keeps its identity.
    expect(out.row).toBe(props.row);
    expect(out.now).toBe(1);
  });

  it('leaves alone the functions a row calls while it renders', () => {
    const domId = () => 'row-a';
    expect(stoodIn({ domId }, ['domId'], stand).domId).toBe(domId);
  });

  it('gives two renders of the same row props that compare equal', () => {
    const lane = { id: 'a' };
    const cache = new Map<string, () => undefined>();
    const stable = (key: string, field?: string) => {
      const p = field ? `${key}.${field}` : key;
      if (!cache.has(p)) cache.set(p, () => undefined);
      return cache.get(p)!;
    };
    const render = () => stoodIn({ lane, drag: { pinIndex: 0, onPointerDown: () => {}, extraClass: undefined } }, [], stable);
    const a = render();
    const b = render();
    expect(looseEqual(a, b, 4)).toBe(true);
    expect((a.drag as { onPointerDown: unknown }).onPointerDown).toBe((b.drag as { onPointerDown: unknown }).onPointerDown);
  });
});
