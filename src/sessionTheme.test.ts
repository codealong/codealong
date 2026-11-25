// Tests for experimental session themes (src/sessionTheme.ts): curated multi-color chords, emblem
// mood matching, deterministic collision-free assignment, helper inheritance and manual picks.

import { describe, expect, it } from 'vitest';
import {
  assignSessionColors,
  emblemPreferredColor,
  hexHue,
  hueDistance,
  nearestPaletteColor,
  SESSION_PALETTE,
  themeCssVars,
  type SessionColorInput,
} from './sessionTheme';

const sess = (key: string, over: Partial<SessionColorInput> = {}): SessionColorInput => ({
  key,
  project: 'Northwind',
  startedAt: 1_000,
  ...over,
});

describe('sessionTheme multi-color chords and emblem matching', () => {
  it('avoids semantic amber and alarm-red representative hues across all 12 curated chords', () => {
    for (const c of SESSION_PALETTE) {
      expect(c.hue < 65 || c.hue > 95, `${c.id} hue ${c.hue} intrudes on amber`).toBe(true);
      expect(c.hue < 18 || c.hue > 30, `${c.id} hue ${c.hue} intrudes on alert red`).toBe(true);
      expect(c.light.stops).toHaveLength(4);
      expect(c.dark.stops).toHaveLength(4);
    }
  });

  it('matches emoji and named emblems to natural curated moods', () => {
    expect(emblemPreferredColor({ name: 'Dinosaur', emoji: '🦕' })).toBe('botanical');
    expect(emblemPreferredColor({ name: 'Rocket', emoji: '🚀' })).toBe('dusk');
    expect(emblemPreferredColor({ name: 'Ocean wave', emoji: '🌊' })).toBe('aurora');
    expect(emblemPreferredColor({ name: 'Fire', emoji: '🔥' })).toBe('terracotta');
    expect(emblemPreferredColor({ name: 'Palette', emoji: '🎨' })).toBe('blossom');
    expect(emblemPreferredColor(null)).toBeNull();
  });

  it('snaps arbitrary sampled hues to the closest curated chord', () => {
    expect(nearestPaletteColor(152)).toBe('botanical');
    expect(nearestPaletteColor(278)).toBe('dusk');
    expect(nearestPaletteColor(186)).toBe('aurora');
    expect(hueDistance(355, 5)).toBe(10);
  });
});

describe('assignSessionColors', () => {
  it('gives active sessions in the same project collision-free chords even when emblems want the same mood', () => {
    const list: SessionColorInput[] = [
      sess('a', { startedAt: 10, emblem: { name: 'Bug', emoji: '🐛' } }),
      sess('b', { startedAt: 20, emblem: { name: 'Cactus', emoji: '🌵' } }),
      sess('c', { startedAt: 30, emblem: { name: 'Dinosaur', emoji: '🦕' } }),
      sess('d', { startedAt: 40, emblem: { name: 'Test tube', emoji: '🧪' } }),
    ];
    const assigned = assignSessionColors(list);
    const ids = list.map((s) => assigned.get(s.key)!.color.id);
    expect(new Set(ids).size).toBe(list.length);
    expect(assigned.get('a')!.color.id).toBe('botanical');
    expect(assigned.get('a')!.source).toBe('emblem');
    expect(assigned.get('b')!.color.id).not.toBe('botanical');
  });

  it('assigns all 12 distinct chords before repeating within one project', () => {
    const list = Array.from({ length: SESSION_PALETTE.length }, (_, i) => sess(`s-${i}`, { startedAt: i * 100 }));
    const assigned = assignSessionColors(list);
    const ids = new Set(list.map((s) => assigned.get(s.key)!.color.id));
    expect(ids.size).toBe(SESSION_PALETTE.length);
  });

  it('honors manual chord picks and steers other sessions away from them', () => {
    const list = [
      sess('a', { startedAt: 10, emblem: { name: 'Rocket', emoji: '🚀' } }),
      sess('b', { startedAt: 20, emblem: { name: 'Moon', emoji: '🌙' } }),
    ];
    const assigned = assignSessionColors(list, { b: 'dusk' });
    expect(assigned.get('b')!.color.id).toBe('dusk');
    expect(assigned.get('b')!.source).toBe('picked');
    expect(assigned.get('a')!.color.id).not.toBe('dusk');
  });

  it('uses sampled drawn emblem image hues ahead of name fallback', () => {
    const list = [sess('a', { emblem: { name: 'Mystery', picture: 'abc.png' } })];
    const assigned = assignSessionColors(list, {}, { 'abc.png': 'blossom' });
    expect(assigned.get('a')!.color.id).toBe('blossom');
    expect(assigned.get('a')!.source).toBe('emblem');
  });

  it('inherits parent chord for helpers while keeping stable chords when a new session arrives', () => {
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

  it('does not let archived sessions block active sessions from getting their emblem mood', () => {
    const list = [
      sess('old', { startedAt: 1, archived: true, emblem: { name: 'Rocket', emoji: '🚀' } }),
      sess('active', { startedAt: 100, emblem: { name: 'Rocket', emoji: '🚀' } }),
    ];
    const assigned = assignSessionColors(list);
    expect(assigned.get('active')!.color.id).toBe('dusk');
    expect(assigned.get('active')!.source).toBe('emblem');
  });

  it('harmonizes chord and accent override from an AI backdrop while keeping helpers clean', () => {
    expect(hexHue('#2a8068')).toBeGreaterThan(140);
    expect(hexHue('#2a8068')).toBeLessThan(185);
    expect(hexHue('#888888')).toBeNull();
    const list = [
      sess('parent', {
        startedAt: 10,
        backdrop: { picture: 'bg1.jpg', name: 'Eucalyptus shadows', accent: '#2a8068', luminance: 215 },
      }),
      sess('helper', { startedAt: 20, helperOf: 'parent' }),
    ];
    const assigned = assignSessionColors(list);
    const parent = assigned.get('parent')!;
    expect(parent.source).toBe('backdrop');
    expect(parent.accentOverride).toBe('#2a8068');
    expect(parent.backdrop?.name).toBe('Eucalyptus shadows');
    expect(parent.backdropImage).toBeTruthy();

    const helper = assigned.get('helper')!;
    expect(helper.source).toBe('inherited');
    expect(helper.backdrop).toBeNull();
    expect(helper.backdropImage).toBeNull();
    expect(helper.accentOverride).toBe('#2a8068');

    // Every UI color variable comes directly from the image's accent, never a preset chord's stops.
    const vars = themeCssVars(parent.color, 'medium', parent.accentOverride) as Record<string, string>;
    for (const key of ['--st-l-1', '--st-l-2', '--st-l-accent', '--st-l-accent2', '--st-d-1', '--st-d-2', '--st-d-accent', '--st-d-accent2']) {
      expect(vars[key], key).toContain('#2a8068');
    }
  });
});
