// Tests for experimental session themes (src/sessionTheme.ts): curated palette safety, emblem
// color matching, deterministic collision-free assignment, helper inheritance and manual picks.

import { describe, expect, it } from 'vitest';
import {
  assignSessionColors,
  emblemPreferredColor,
  hueDistance,
  nearestPaletteColor,
  SESSION_PALETTE,
  type SessionColorInput,
} from './sessionTheme';

const sess = (key: string, over: Partial<SessionColorInput> = {}): SessionColorInput => ({
  key,
  project: 'Northwind',
  startedAt: 1_000,
  ...over,
});

describe('sessionTheme palette and emblem matching', () => {
  it('avoids semantic amber and alarm-red hue ranges across the curated palette', () => {
    for (const c of SESSION_PALETTE) {
      // Sacred amber (attn) sits around 65–95°, and alarm red (alert) around 18–30°.
      expect(c.hue < 65 || c.hue > 95, `${c.id} hue ${c.hue} intrudes on amber`).toBe(true);
      expect(c.hue < 18 || c.hue > 30, `${c.id} hue ${c.hue} intrudes on alert red`).toBe(true);
    }
  });

  it('matches emoji and named emblems to natural curated families', () => {
    expect(emblemPreferredColor({ name: 'Dinosaur', emoji: '🦕' })).toBe('sage');
    expect(emblemPreferredColor({ name: 'Rocket', emoji: '🚀' })).toBe('indigo');
    expect(emblemPreferredColor({ name: 'Ocean wave', emoji: '🌊' })).toBe('teal');
    expect(emblemPreferredColor({ name: 'Fire', emoji: '🔥' })).toBe('clay');
    expect(emblemPreferredColor({ name: 'Palette', emoji: '🎨' })).toBe('orchid');
    expect(emblemPreferredColor(null)).toBeNull();
  });

  it('snaps arbitrary sampled hues to the closest curated palette color', () => {
    expect(nearestPaletteColor(150)).toBe('sage');
    expect(nearestPaletteColor(280)).toBe('indigo');
    expect(nearestPaletteColor(196)).toBe('teal');
    expect(hueDistance(355, 5)).toBe(10);
  });
});

describe('assignSessionColors', () => {
  it('gives active sessions in the same project collision-free colors even when emblems want the same hue', () => {
    const list: SessionColorInput[] = [
      sess('a', { startedAt: 10, emblem: { name: 'Bug', emoji: '🐛' } }),
      sess('b', { startedAt: 20, emblem: { name: 'Cactus', emoji: '🌵' } }),
      sess('c', { startedAt: 30, emblem: { name: 'Dinosaur', emoji: '🦕' } }),
      sess('d', { startedAt: 40, emblem: { name: 'Test tube', emoji: '🧪' } }),
    ];
    const assigned = assignSessionColors(list);
    const ids = list.map((s) => assigned.get(s.key)!.color.id);
    expect(new Set(ids).size).toBe(list.length);
    // First session claims its emblem's exact match; second gets the closest free neighbor hue.
    expect(assigned.get('a')!.color.id).toBe('sage');
    expect(assigned.get('a')!.source).toBe('emblem');
    expect(assigned.get('b')!.color.id).not.toBe('sage');
  });

  it('assigns all 12 distinct colors before repeating within one project', () => {
    const list = Array.from({ length: SESSION_PALETTE.length }, (_, i) => sess(`s-${i}`, { startedAt: i * 100 }));
    const assigned = assignSessionColors(list);
    const ids = new Set(list.map((s) => assigned.get(s.key)!.color.id));
    expect(ids.size).toBe(SESSION_PALETTE.length);
  });

  it('honors manual color picks and steers other sessions away from them', () => {
    const list = [
      sess('a', { startedAt: 10, emblem: { name: 'Rocket', emoji: '🚀' } }),
      sess('b', { startedAt: 20, emblem: { name: 'Moon', emoji: '🌙' } }),
    ];
    const assigned = assignSessionColors(list, { b: 'indigo' });
    expect(assigned.get('b')!.color.id).toBe('indigo');
    expect(assigned.get('b')!.source).toBe('picked');
    expect(assigned.get('a')!.color.id).not.toBe('indigo');
  });

  it('uses sampled drawn emblem image hues ahead of name fallback', () => {
    const list = [sess('a', { emblem: { name: 'Mystery', picture: 'abc.png' } })];
    const assigned = assignSessionColors(list, {}, { 'abc.png': 'orchid' });
    expect(assigned.get('a')!.color.id).toBe('orchid');
    expect(assigned.get('a')!.source).toBe('emblem');
  });

  it('inherits parent color for helpers while keeping stable colors when a new session arrives', () => {
    const base = [
      sess('parent', { startedAt: 10, emblem: { name: 'Wave', emoji: '🌊' } }),
      sess('helper', { startedAt: 15, helperOf: 'parent' }),
    ];
    const first = assignSessionColors(base);
    expect(first.get('helper')!.color.id).toBe(first.get('parent')!.color.id);
    expect(first.get('helper')!.source).toBe('inherited');

    const withNew = [...base, sess('newer', { startedAt: 99, emblem: { name: 'Wave', emoji: '🌊' } })];
    const second = assignSessionColors(withNew);
    expect(second.get('parent')!.color.id).toBe(first.get('parent')!.color.id);
    expect(second.get('newer')!.color.id).not.toBe(first.get('parent')!.color.id);
  });

  it('does not let archived sessions block active sessions from getting their emblem color', () => {
    const list = [
      sess('old', { startedAt: 1, archived: true, emblem: { name: 'Rocket', emoji: '🚀' } }),
      sess('active', { startedAt: 100, emblem: { name: 'Rocket', emoji: '🚀' } }),
    ];
    const assigned = assignSessionColors(list);
    expect(assigned.get('active')!.color.id).toBe('indigo');
    expect(assigned.get('active')!.source).toBe('emblem');
  });
});
