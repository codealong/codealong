// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { act, engine, settle } from '../testing/dom';
import { createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { noteSide, showSides } from '../side';
import type { Side, SideKept } from '../types';
import { SESSION_SIDES, SideStrip } from './SideStrip';

let seq = 0;
const nextKey = () => `claude:test:side-scroll-${++seq}`;

const roots: Root[] = [];
const opened: string[] = [];

function mountStrip(sideKey: string) {
  opened.push(sideKey);
  const host = document.body.appendChild(document.createElement('div'));
  const root = createRoot(host);
  roots.push(root);
  act(() => root.render(createElement(SideStrip, { sideKey, words: SESSION_SIDES })));
  return host;
}

/** Gives `.ca-sidestrip-turns` a real scroll geometry in jsdom, which reports 0 for layout sizes. */
function makeScrollable(host: HTMLElement, scrollHeight = 800, clientHeight = 200): HTMLDivElement {
  const el = host.querySelector<HTMLDivElement>('.ca-sidestrip-turns');
  if (!el) throw new Error('expected .ca-sidestrip-turns');
  Object.defineProperty(el, 'scrollHeight', { configurable: true, get: () => scrollHeight });
  Object.defineProperty(el, 'clientHeight', { configurable: true, get: () => clientHeight });
  return el;
}

function keptOf(key: string, id: string, question: string, answer: string, answering = false): SideKept {
  const now = Date.now();
  return { id, key, title: question, startedAt: now - 60_000, lastAt: now - 30_000, turns: [{ question, answer, done: !answering }], answering };
}

afterEach(() => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  for (const key of opened.splice(0)) {
    act(() => {
      showSides(key, null);
      noteSide({ id: 'clear', key, turns: [], running: false, ended: true });
    });
  }
});

describe('SideStrip scrolling', () => {
  it('scrolls to the bottom when opening a kept side conversation', async () => {
    const key = nextKey();
    const past = [keptOf(key, 'past-1', 'First question?', 'Long answer.')];
    engine.answer('side_past', () => past);
    showSides(key, { kind: 'list' });
    const host = mountStrip(key);
    await settle();

    const el = makeScrollable(host, 900, 200);
    el.scrollTop = 0;

    act(() => showSides(key, { kind: 'past', id: 'past-1' }));
    expect(el.scrollTop).toBe(900);
  });

  it('scrolls to the bottom when switching between kept side conversations after scrolling up', async () => {
    const key = nextKey();
    const past = [
      keptOf(key, 'past-a', 'Question A?', 'Answer A.'),
      keptOf(key, 'past-b', 'Question B?', 'Answer B.'),
    ];
    engine.answer('side_past', () => past);
    showSides(key, { kind: 'past', id: 'past-a' });
    const host = mountStrip(key);
    await settle();

    const el = makeScrollable(host, 900, 200);
    // Scroll up in past-a.
    el.scrollTop = 100;
    act(() => void el.dispatchEvent(new Event('scroll')));

    // Opening past-b starts at the bottom again.
    act(() => showSides(key, { kind: 'past', id: 'past-b' }));
    expect(el.scrollTop).toBe(900);
  });

  it('scrolls back to the bottom when returning to an open side conversation after viewing earlier ones', async () => {
    const key = nextKey();
    const open: Side = { id: 'open-1', key, turns: [{ question: 'Why?', answer: 'Because.', done: true }], running: false, ended: false };
    engine.answer('side_get', () => open);
    const host = mountStrip(key);
    await settle();

    const el = makeScrollable(host, 900, 200);
    // Scroll up while reading the open one.
    el.scrollTop = 50;
    act(() => void el.dispatchEvent(new Event('scroll')));

    // Switch to the list: resets the shared scroller to the top, then back to the open one puts the end in view.
    act(() => showSides(key, { kind: 'list' }));
    expect(el.scrollTop).toBe(0);
    act(() => showSides(key, null));
    expect(el.scrollTop).toBe(900);
  });

  it('resets the list to the top after viewing a scrolled kept conversation', async () => {
    const key = nextKey();
    const past = [keptOf(key, 'past-long', 'Long one?', 'Lots of text.')];
    engine.answer('side_past', () => past);
    showSides(key, { kind: 'list' });
    const host = mountStrip(key);
    await settle();

    const el = makeScrollable(host, 900, 200);
    act(() => showSides(key, { kind: 'past', id: 'past-long' }));
    expect(el.scrollTop).toBe(900);

    act(() => showSides(key, { kind: 'list' }));
    expect(el.scrollTop).toBe(0);
  });

  it('scrolls to the bottom again when put back from a popped-out window', async () => {
    const key = nextKey();
    const open: Side = { id: 'open-pop', key, turns: [{ question: 'Popped?', answer: 'Yes.', done: true }], running: false, ended: false };
    engine.answer('side_get', () => open);
    const host = mountStrip(key);
    await settle();

    // Pop out: the strip unmounts above the box.
    act(() => engine.emit('side-windows', [key]));
    expect(host.querySelector('.ca-sidestrip-turns')).toBeNull();

    // Put back: remounts at the bottom (with scrollHeight readable once mounted, via prototype getter).
    const desc = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollHeight');
    Object.defineProperty(HTMLElement.prototype, 'scrollHeight', { configurable: true, get: () => 750 });
    try {
      act(() => engine.emit('side-windows', []));
      const el = host.querySelector<HTMLDivElement>('.ca-sidestrip-turns');
      expect(el?.scrollTop).toBe(750);
    } finally {
      if (desc) Object.defineProperty(HTMLElement.prototype, 'scrollHeight', desc);
      else delete (HTMLElement.prototype as { scrollHeight?: number }).scrollHeight;
    }
  });

  it('follows a closed side conversation that is still answering unless the user scrolled up', async () => {
    const key = nextKey();
    const past = [keptOf(key, 'closing-1', 'Still working?', 'Part 1', true)];
    engine.answer('side_past', () => past);
    showSides(key, { kind: 'past', id: 'closing-1' });
    const host = mountStrip(key);
    await settle();

    let height = 600;
    const el = host.querySelector<HTMLDivElement>('.ca-sidestrip-turns')!;
    Object.defineProperty(el, 'scrollHeight', { configurable: true, get: () => height });
    Object.defineProperty(el, 'clientHeight', { configurable: true, get: () => 200 });

    // Re-open with scrollable geometry so it starts at the bottom.
    act(() => showSides(key, { kind: 'list' }));
    act(() => showSides(key, { kind: 'past', id: 'closing-1' }));
    expect(el.scrollTop).toBe(600);

    // More of the answer arrives: stays at the bottom.
    height = 850;
    act(() =>
      engine.emit('side', {
        id: 'closing-1',
        key,
        turns: [{ question: 'Still working?', answer: 'Part 1\nPart 2', done: false }],
        running: true,
        ended: false,
        closed: true,
      } satisfies Side),
    );
    expect(el.scrollTop).toBe(850);

    // User scrolls up to read: new chunks no longer pull it back down.
    el.scrollTop = 100;
    act(() => void el.dispatchEvent(new Event('scroll')));
    height = 1100;
    act(() =>
      engine.emit('side', {
        id: 'closing-1',
        key,
        turns: [{ question: 'Still working?', answer: 'Part 1\nPart 2\nPart 3', done: false }],
        running: true,
        ended: false,
        closed: true,
      } satisfies Side),
    );
    expect(el.scrollTop).toBe(100);
  });
});
