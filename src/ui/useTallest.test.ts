// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { mountHook } from '../testing/dom';
import { useLayoutEffect } from 'react';
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

/** The element held, with a layout effect declared after it that notes in `found` the least
 *  height the element has by then: what the panel's own layout effect, which reads where its
 *  rows sit, finds the Suggested group held to. */
function useTallestThenRead(found: string[]) {
  const held = useTallest<HTMLDivElement>();
  useLayoutEffect(() => {
    if (held.current) found.push(held.current.style.minHeight);
  });
  return held;
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

  it('is held to its height before the layout effects after it run, in the render that measures it', () => {
    const tall = { px: 96 };
    const { el } = element(tall);
    const found: string[] = [];
    const held = mountHook(() => useTallestThenRead(found));
    held.current.current = el;
    held.rerender();
    tall.px = 128;
    held.rerender();
    tall.px = 32;
    held.rerender();
    // Not a render late: an effect that waited for the page to draw would leave the first of
    // these with no height held, and each after it with the one before.
    expect(found).toEqual(['96px', '128px', '128px']);
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
