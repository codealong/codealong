import { describe, expect, it } from 'vitest';
import type { TerminalFrame } from '../../types';
import { apply, emptyGrid, linkAt } from './grid';

function frame(over: Partial<TerminalFrame>): TerminalFrame {
  return { id: 't', seq: 1, cols: 10, rows: 3, full: true, lines: [], looks: [{}], cursor: null, fg: 0x111111, bg: 0xeeeeee, scroll: { total: 3, offset: 0, len: 3 }, mouse: false, ...over };
}

describe('a terminal screen kept from frames', () => {
  it('starts from a whole frame and then takes only the rows that changed', () => {
    const g = emptyGrid();
    expect(apply(g, frame({ full: false, seq: 4 }))).toBe('missed');
    const first = apply(g, frame({ lines: [{ y: 0, runs: [{ x: 0, l: 0, c: ['h', 'i'] }] }] }));
    expect(first).toEqual({ all: true, rows: [0], cursorMoved: false });
    const next = apply(g, frame({ seq: 2, full: false, looks: [{ f: 0xff0000 }], lines: [{ y: 2, runs: [{ x: 3, l: 0, c: ['x'] }] }], cursor: { x: 4, y: 2, shape: 'block', blink: false, password: false } }));
    expect(next).toEqual({ all: false, rows: [2], cursorMoved: true });
    expect(g.lines[0][0].cells).toEqual(['h', 'i']);
    expect(g.lines[2][0]).toEqual({ x: 3, look: { f: 0xff0000 }, cells: ['x'] });
  });

  it('asks for the whole screen after a missed frame', () => {
    const g = emptyGrid();
    apply(g, frame({}));
    expect(apply(g, frame({ seq: 3, full: false }))).toBe('missed');
  });

  it('draws everything again when the size or the colors change', () => {
    const g = emptyGrid();
    apply(g, frame({}));
    expect(apply(g, frame({ seq: 2, full: false, cols: 12 }))).toMatchObject({ all: true });
    expect(apply(g, frame({ seq: 3, full: false, cols: 12, bg: 0 }))).toMatchObject({ all: true });
  });
});

describe('a link under the pointer', () => {
  it('finds the address a column is in, without the punctuation after it', () => {
    const runs = [{ x: 0, look: {}, cells: [...'see https://example.com/a?b=1.'] }];
    expect(linkAt(runs, 40, 10)).toBe('https://example.com/a?b=1');
    expect(linkAt(runs, 40, 1)).toBeNull();
    expect(linkAt(runs, 40, 29)).toBeNull();
  });

  it('counts a wide character as the two cells it takes', () => {
    const runs = [{ x: 0, look: {}, cells: ['字', '', ' ', ...'http://x.io'] }];
    expect(linkAt(runs, 20, 3)).toBe('http://x.io');
  });
});
