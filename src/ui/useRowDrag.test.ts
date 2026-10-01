import { describe, expect, it } from 'vitest';
import { zoneAt, type Span } from './useRowDrag';

/** Rows 40 tall, one under the other from `top`. */
const rows = (n: number, top = 100): Span[] => Array.from({ length: n }, (_, i) => ({ top: top + i * 40, bottom: top + (i + 1) * 40 }));

describe('zoneAt', () => {
  const pinned = rows(3); // 100–220
  const below: Span = { top: 300, bottom: 340 };

  it('lands a pinned row before the first row whose middle is below the pointer', () => {
    expect(zoneAt(90, pinned, pinned[0], pinned[1], true, true)).toBe(0);
    expect(zoneAt(119, pinned, pinned[0], pinned[1], true, true)).toBe(0);
    expect(zoneAt(121, pinned, pinned[0], pinned[1], true, true)).toBe(1);
    expect(zoneAt(201, pinned, pinned[0], pinned[1], true, true)).toBe(3);
  });

  it('takes drops a little past the last pinned row, then the list has them', () => {
    expect(zoneAt(235, pinned, pinned[0], pinned[0], true, true)).toBe(3);
    expect(zoneAt(240, pinned, pinned[0], pinned[0], true, true)).toBe('list');
    expect(zoneAt(600, pinned, pinned[0], pinned[0], true, true)).toBe('list');
  });

  it('pins a row dragged up among the pinned ones', () => {
    expect(zoneAt(110, pinned, pinned[0], below, false, true)).toBe(0);
    expect(zoneAt(230, pinned, pinned[0], below, false, true)).toBe(3);
    expect(zoneAt(280, pinned, pinned[0], below, false, true)).toBe('list');
  });

  it('pins the row right under the pinned ones only from above its own place', () => {
    const next: Span = { top: 224, bottom: 264 };
    expect(zoneAt(230, pinned, pinned[0], next, false, true)).toBe('list');
    expect(zoneAt(222, pinned, pinned[0], next, false, true)).toBe(3);
  });

  it('with nothing pinned, pins from the top of the list', () => {
    const list = rows(4);
    expect(zoneAt(110, [], list[0], list[2], false, true)).toBe(0);
    expect(zoneAt(125, [], list[0], list[2], false, true)).toBe('list');
    // The first row itself: only once it's above its own place.
    expect(zoneAt(110, [], list[0], list[0], false, true)).toBe('list');
    expect(zoneAt(95, [], list[0], list[0], false, true)).toBe(0);
  });

  it('leaves rows out of an order they cannot join', () => {
    expect(zoneAt(110, pinned, pinned[0], below, false, false)).toBe('list');
    expect(zoneAt(110, [], null, below, false, true)).toBe('list');
  });
});
