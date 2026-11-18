import { describe, expect, it } from 'vitest';
import { Hold, HOLD_MS } from './keysShown';

describe('holding ⌘ or ⌃ to see the keys', () => {
  it('shows them once ⌘ alone has been held a moment, until it’s let go', () => {
    const h = new Hold();
    h.down('Meta', 0);
    expect(h.at(HOLD_MS - 1)).toBe(false);
    expect(h.at(HOLD_MS)).toBe(true);
    // A key pressed once they show is a chord pressed while looking: they stay.
    h.down('e', HOLD_MS + 10);
    expect(h.at(HOLD_MS + 20)).toBe(true);
    h.up('Meta');
    expect(h.at(HOLD_MS + 30)).toBe(false);
  });

  it('shows nothing for a chord pressed at once', () => {
    const h = new Hold();
    h.down('Meta', 0);
    h.down('k', 50);
    expect(h.at(HOLD_MS * 2)).toBe(false);
  });

  it('waits on through ⇧ and ⌥, whose chords are worth seeing too, and counts ⌃ as ⌘', () => {
    const h = new Hold();
    h.down('Control', 0);
    h.down('Meta', 100);
    h.down('Shift', 200);
    expect(h.at(HOLD_MS)).toBe(true);
  });

  it('hides them when the window loses the keyboard, and a new hold starts over', () => {
    const h = new Hold();
    h.down('Meta', 0);
    expect(h.at(HOLD_MS)).toBe(true);
    h.lost();
    expect(h.at(HOLD_MS + 1)).toBe(false);
    h.down('Meta', 1000);
    expect(h.at(1000 + HOLD_MS - 1)).toBe(false);
    expect(h.at(1000 + HOLD_MS)).toBe(true);
  });

  it('ignores a repeat of the key it holds', () => {
    const h = new Hold();
    h.down('Meta', 0);
    h.down('Meta', 300);
    expect(h.at(HOLD_MS)).toBe(true);
  });
});
