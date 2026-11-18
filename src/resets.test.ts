import { describe, expect, it } from 'vitest';
import { earlyNote, expiryPhrase, heldNote, holds, resetsTitle, usable } from './resets';
import type { Meter, Resets } from './types';

const NOW = Date.UTC(2026, 10, 18, 20, 0);
const DAY = 24 * 60 * 60 * 1000;

function resets(available: number, expires: (number | null)[], title: string | null = 'Full reset'): Resets {
  const credits = expires.map((at, i) => ({ id: `c${i}`, title, description: null, grantedAt: null, expiresAt: at }));
  return { account: 'codex', available, credits, canUse: true, note: null, asOf: NOW };
}

function meter(percent: number): Meter {
  return { key: 'week', label: 'Week', window: 'Weekly', percent, resetsAt: null, severity: 'normal', model: null };
}

describe('limit resets', () => {
  it('marks an account holding one, and offers to use only one it can name', () => {
    expect(holds(resets(3, [NOW + DAY]))).toBe(true);
    expect(holds(resets(0, []))).toBe(false);
    expect(holds(undefined)).toBe(false);
    expect(usable(resets(3, [NOW + DAY]))).toBe(true);
    // Only counted: marked, its card says why it can't be used from here.
    expect(holds(resets(2, []))).toBe(true);
    expect(usable(resets(2, []))).toBe(false);
    expect(usable({ ...resets(1, [null]), canUse: false })).toBe(false);
  });

  it('counts them', () => {
    expect(resetsTitle(resets(3, []))).toBe('3 limit resets');
    expect(resetsTitle(resets(1, []))).toBe('1 limit reset');
  });

  it('says what they are and when the first runs out', () => {
    expect(heldNote(resets(3, [NOW + 12 * DAY, NOW + 13 * DAY, null]), NOW)).toBe('Full resets · the first expires Nov 30');
    expect(heldNote(resets(1, [NOW + 5 * 60 * 60 * 1000]), NOW)).toBe('Full reset · expires in 5h');
    expect(heldNote(resets(2, [null, null]), NOW)).toBe('Full resets');
    // Only the count known, or titles that differ: what a reset does, in general.
    expect(heldNote(resets(2, []), NOW)).toBe('Each puts the plan’s limits back to empty');
    expect(heldNote({ ...resets(2, [null]), credits: [{ id: 'a', title: 'Full reset', description: null, grantedAt: null, expiresAt: null }, { id: 'b', title: 'Weekly reset', description: null, grantedAt: null, expiresAt: NOW + 2 * DAY }] }, NOW)).toBe('The first expires Nov 20');
  });

  it('says when one expires', () => {
    expect(expiryPhrase(NOW - 1, NOW)).toBe('now');
    expect(expiryPhrase(NOW + 90 * 60 * 1000, NOW)).toBe('in 1h 30m');
    expect(expiryPhrase(NOW + 12 * DAY, NOW)).toBe('Nov 30');
  });

  it('warns that a reset would free little while the limits are far from full', () => {
    expect(earlyNote([meter(6)])).toBe('Its fullest limit is at 6%, so a reset now would free little.');
    expect(earlyNote([meter(6), meter(93)])).toBeNull();
    expect(earlyNote([])).toBeNull();
  });
});
