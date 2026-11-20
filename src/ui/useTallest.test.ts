// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { mountHook } from '../testing/dom';
import { useTallest } from './useTallest';

// An element that only grows while it's on the page (`useTallest`), as the quick panel's
// Suggested group does. jsdom lays nothing out, so the element here says how tall it is itself.

/** An element as tall as `tall.px` whenever it's measured, which notes the least height it had
 *  as it was (`measuredWith`): it has to be measured as it would be by itself. */
function element(tall: { px: number }) {
  const el = document.createElement('div');
  const measuredWith: string[] = [];
  Object.defineProperty(el, 'offsetHeight', {
    get: () => {
      measuredWith.push(el.style.minHeight);
      return tall.px;
    },
  });
  return { el, measuredWith };
}

describe('an element that keeps the tallest it has been', () => {
  it('has its height as its least height, and keeps it when it has less to show', () => {
    const tall = { px: 96 };
    const { el } = element(tall);
    const held = mountHook(() => useTallest<HTMLDivElement>());
    held.current.current = el;
    held.rerender();
    expect(el.style.minHeight).toBe('96px');
    tall.px = 32;
    held.rerender();
    expect(el.style.minHeight).toBe('96px');
    tall.px = 128;
    held.rerender();
    expect(el.style.minHeight).toBe('128px');
  });

  it('is measured as it would be by itself, without the height it’s held to', () => {
    const tall = { px: 96 };
    const { el, measuredWith } = element(tall);
    const held = mountHook(() => useTallest<HTMLDivElement>());
    held.current.current = el;
    held.rerender();
    held.rerender();
    held.rerender();
    expect(measuredWith).toEqual(['', '', '']);
  });

  it('starts afresh once it’s been off the page', () => {
    const tall = { px: 96 };
    const first = element(tall);
    const held = mountHook(() => useTallest<HTMLDivElement>());
    held.current.current = first.el;
    held.rerender();
    held.current.current = null;
    held.rerender();
    const next = element({ px: 32 });
    held.current.current = next.el;
    held.rerender();
    expect(next.el.style.minHeight).toBe('32px');
  });

  it('does nothing with no element to hold', () => {
    const held = mountHook(() => useTallest<HTMLDivElement>());
    held.rerender();
    expect(held.current.current).toBeNull();
  });
});
