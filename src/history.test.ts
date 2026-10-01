import { describe, expect, it } from 'vitest';
import { back, forward, NO_HISTORY, parseHistory, visit, type History } from './history';

const all = () => true;
const seen = (...keys: string[]): History => keys.reduce(visit, NO_HISTORY);
const on = (h: History | null) => (h ? h.keys[h.at] : null);

describe('visit', () => {
  it('lists sessions in the order you looked at them', () => {
    expect(seen('a', 'b', 'c')).toEqual({ keys: ['a', 'b', 'c'], at: 2 });
  });

  it('doesn’t list the one you’re on twice', () => {
    const h = seen('a', 'b');
    expect(visit(h, 'b')).toBe(h);
  });

  it('drops what was ahead, as a browser does', () => {
    const h = back(seen('a', 'b', 'c'), 'c', all)!;
    expect(visit(h, 'd')).toEqual({ keys: ['a', 'b', 'd'], at: 2 });
  });

  it('keeps the latest hundred', () => {
    const h = Array.from({ length: 130 }, (_, i) => `s${i}`).reduce(visit, NO_HISTORY);
    expect(h.keys).toHaveLength(100);
    expect(h.keys[0]).toBe('s30');
    expect(on(h)).toBe('s129');
  });
});

describe('back and forward', () => {
  it('go to the one before and back again', () => {
    const h = seen('a', 'b', 'c');
    const b = back(h, 'c', all);
    expect(on(b)).toBe('b');
    expect(on(back(b!, 'b', all))).toBe('a');
    expect(back(seen('a'), 'a', all)).toBeNull();
    expect(on(forward(b!, 'b', all))).toBe('c');
    expect(forward(h, 'c', all)).toBeNull();
  });

  it('keep a session you only just came to, so Forward returns to it', () => {
    // c is showing but hasn't been there long enough to be listed.
    const b = back(seen('a', 'b'), 'c', all)!;
    expect(on(b)).toBe('b');
    expect(on(forward(b, 'b', all))).toBe('c');
  });

  it('go back to the last session from a view that shows none', () => {
    const h = seen('a', 'b');
    const b = back(h, null, all);
    expect(on(b)).toBe('b');
    expect(on(back(b!, 'b', all))).toBe('a');
    // And on from where the list is.
    expect(on(forward(back(h, 'b', all)!, null, all))).toBe('b');
  });

  it('have nothing ahead of a session that isn’t the one the list is on', () => {
    const h = back(seen('a', 'b', 'c'), 'c', all)!;
    expect(forward(h, 'd', all)).toBeNull();
  });

  it('pass over sessions that can’t be shown, and keep them', () => {
    const h = seen('a', 'b', 'c', 'd');
    const b = back(h, 'd', (k) => k !== 'c' && k !== 'b')!;
    expect(on(b)).toBe('a');
    expect(b.keys).toEqual(['a', 'b', 'c', 'd']);
    expect(on(forward(b, 'a', (k) => k !== 'b'))).toBe('c');
    expect(back(h, 'd', () => false)).toBeNull();
  });

  it('don’t stop on the session already showing', () => {
    // b is gone, so the one before a is a again.
    const h = seen('z', 'a', 'b', 'a');
    expect(on(back(h, 'a', (k) => k !== 'b'))).toBe('z');
    const start = back(back(h, 'a', all)!, 'b', all)!;
    expect(on(start)).toBe('a');
    expect(forward(start, 'a', (k) => k !== 'b')).toBeNull();
  });
});

describe('parseHistory', () => {
  it('reads what was kept', () => {
    const h = back(seen('a', 'b', 'c'), 'c', all)!;
    expect(parseHistory(JSON.stringify(h))).toEqual(h);
  });

  it('starts over from anything else', () => {
    expect(parseHistory(null)).toBe(NO_HISTORY);
    expect(parseHistory('nope')).toBe(NO_HISTORY);
    expect(parseHistory('{"keys":[1,2],"at":0}')).toBe(NO_HISTORY);
    expect(parseHistory('{"keys":[],"at":-1}')).toBe(NO_HISTORY);
  });

  it('puts a place that’s out of range back in it', () => {
    expect(parseHistory('{"keys":["a","b"],"at":7}')).toEqual({ keys: ['a', 'b'], at: 1 });
    expect(parseHistory('{"keys":["a","b"]}')).toEqual({ keys: ['a', 'b'], at: 1 });
  });
});
