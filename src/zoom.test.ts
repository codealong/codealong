import { describe, expect, it } from 'vitest';
import { cardZoom, pageZoom, setPageZoom, zoomStep } from './zoom';

const key = (k: string, mods: { shift?: boolean; ctrl?: boolean; alt?: boolean; meta?: boolean } = {}) => ({ key: k, metaKey: mods.meta ?? true, ctrlKey: !!mods.ctrl, altKey: !!mods.alt, shiftKey: !!mods.shift });

describe('zoomStep', () => {
  it('takes VS Code’s keys', () => {
    expect(zoomStep(key('='))).toBe('in');
    expect(zoomStep(key('+', { shift: true }))).toBe('in');
    expect(zoomStep(key('-'))).toBe('out');
    expect(zoomStep(key('_', { shift: true }))).toBe('out');
    expect(zoomStep(key('0'))).toBe('actual');
  });

  it('leaves the keys other chords have', () => {
    // ⌃- goes back through the sessions looked at (history.ts), ⌥⌘0 to every project.
    expect(zoomStep(key('-', { meta: false, ctrl: true }))).toBeNull();
    expect(zoomStep(key('0', { alt: true }))).toBeNull();
    expect(zoomStep(key('-', { ctrl: true }))).toBeNull();
    expect(zoomStep(key(')', { shift: true }))).toBeNull();
    expect(zoomStep(key('=', { meta: false }))).toBeNull();
  });
});

describe('cardZoom', () => {
  it('draws a card at the zoom, and leaves one at actual size alone', () => {
    expect(cardZoom(125)).toEqual({ zoom: 1.25, '--card-zoom': 1.25 });
    expect(cardZoom(100)).toBeUndefined();
    expect(cardZoom(undefined)).toBeUndefined();
  });
});

describe('pageZoom', () => {
  it('is the board’s zoom as a factor, and actual size without one', () => {
    setPageZoom(125);
    expect(pageZoom()).toBe(1.25);
    setPageZoom(undefined);
    expect(pageZoom()).toBe(1);
    setPageZoom(0);
    expect(pageZoom()).toBe(1);
  });
});
