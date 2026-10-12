import { describe, expect, it } from 'vitest';
import { chordKeys } from './menus';
import { nextTip, shortcutCaps, shownNow, TIP_PLACES, TIPS, tipShows, type Tip, type TipPart, type TipShortcuts } from './tips';

const ON = { quick: 'Option+Cmd+/', waiting: 'Option+Cmd+.', improve: 'Option+Cmd+,' } satisfies TipShortcuts;

/** A tip's words as they read, a cap's keys run together. */
function read(says: TipPart[]): string {
  return says.map((p) => (typeof p === 'string' ? p : 'chord' in p ? chordKeys(p.chord).join('') : 'shortcut' in p ? shortcutCaps(ON[p.shortcut]).join('') : p.code)).join('');
}

describe('the tips', () => {
  it('each have an id of their own, kept in storage', () => {
    const ids = TIPS.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) expect(id).toMatch(/^[a-z]+(-[a-z]+)*$/);
  });

  it('give every place several to come round', () => {
    for (const place of TIP_PLACES) expect(TIPS.filter((t) => t.places.includes(place)).length, place).toBeGreaterThanOrEqual(5);
  });

  it('name keys the chord tables have, each a sign or a key on a cap', () => {
    for (const t of TIPS) {
      for (const p of t.says) {
        if (typeof p === 'string' || !('chord' in p)) continue;
        expect(p.chord, t.id).not.toBe('');
        for (const cap of chordKeys(p.chord)) expect(cap.length, `${t.id}: ${p.chord}`).toBeLessThanOrEqual(3);
      }
    }
  });

  it('read as one or two short sentences', () => {
    for (const t of TIPS) {
      const text = read(t.says);
      expect(text.length, `${t.id}: ${text}`).toBeLessThanOrEqual(150);
      expect(text, t.id).toMatch(/\.$/);
      expect(text, t.id).not.toMatch(/\s{2}|^\s|\s$/);
      expect(text.split(/[.:] (?=[A-Z⌃⌥⇧⌘])/).length, `${t.id}: ${text}`).toBeLessThanOrEqual(3);
      const first = t.says[0];
      if (typeof first === 'string') expect(first, t.id).toMatch(/^[A-Z]/);
    }
  });
});

describe('tipShows', () => {
  const tip: Tip = { id: 'q', places: ['board'], says: [{ shortcut: 'quick' }, ' opens it.'] };
  it('leaves out a tip about a shortcut that is off or unreadable', () => {
    expect(tipShows(tip, ON)).toBe(true);
    expect(tipShows(tip, {})).toBe(false);
    expect(tipShows(tip, { quick: '' })).toBe(false);
    expect(tipShows(tip, { quick: 'Cmd+K+J' })).toBe(false);
  });
  it('shows the rest whatever the shortcuts', () => {
    expect(tipShows({ id: 'p', places: ['board'], says: ['Plain words.'] }, {})).toBe(true);
  });
});

describe('nextTip', () => {
  const tips: Tip[] = [
    { id: 'a', places: ['board'], says: ['A.'] },
    { id: 'b', places: ['board', 'quick'], says: ['B.'] },
    { id: 'c', places: ['quick'], says: ['C.'] },
    { id: 'd', places: ['board'], says: [{ shortcut: 'waiting' }, ' d.'] },
  ];
  const at = (place: 'board' | 'quick', shown: string[], cuts: TipShortcuts = ON) => nextTip(place, shown, cuts, tips)?.id ?? null;

  it("goes in the list's order while some haven't shown", () => {
    expect(at('board', [])).toBe('a');
    expect(at('board', ['a'])).toBe('b');
    expect(at('board', ['a', 'b'])).toBe('d');
  });
  it('then comes round to the one shown longest ago', () => {
    expect(at('board', ['b', 'a', 'd'])).toBe('b');
    expect(at('board', ['d', 'b', 'a'])).toBe('d');
  });
  it('counts tips shown at other places', () => {
    expect(at('quick', ['b'])).toBe('c');
  });
  it('passes over a tip whose shortcut is off', () => {
    expect(at('board', ['a', 'b'], {})).toBe('a');
  });
  it('has none for a place without tips', () => {
    expect(at('quick', [], ON)).toBe('b');
    expect(nextTip('pr', [], ON, tips)).toBeNull();
  });
});

describe('shownNow', () => {
  it('moves the tip shown to the end, and forgets tips that are gone', () => {
    const tips: Tip[] = ['a', 'b', 'c'].map((id) => ({ id, places: ['board'], says: [] }));
    expect(shownNow([], 'a', tips)).toEqual(['a']);
    expect(shownNow(['a', 'b', 'c'], 'a', tips)).toEqual(['b', 'c', 'a']);
    expect(shownNow(['gone', 'b'], 'c', tips)).toEqual(['b', 'c']);
  });
});

describe('shortcutCaps', () => {
  it('puts each key of a shortcut on a cap of its own', () => {
    expect(shortcutCaps('Option+Cmd+/')).toEqual(['⌥', '⌘', '/']);
    expect(shortcutCaps('Option+Shift+Space')).toEqual(['⌥', '⇧', 'Space']);
    expect(shortcutCaps('')).toEqual([]);
    expect(shortcutCaps(undefined)).toEqual([]);
  });
});
