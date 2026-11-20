// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { act, engine, settle } from '../testing/dom';
import { beingRead, noteSide, showSides } from '../side';
import type { SideKept, TranscriptItem } from '../types';
import { PeerCard } from './MessageCard';

let seq = 0;
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
const nextKey = () => `claude:test:msgcard-side-${++seq}`;
const roots: Root[] = [];
const opened: string[] = [];

function mountCard(sessionKey: string, item: TranscriptItem, at: number) {
  opened.push(sessionKey);
  const host = document.body.appendChild(document.createElement('div'));
  const root = createRoot(host);
  roots.push(root);
  act(() => root.render(createElement(PeerCard, { item, at, now: at + 60_000, sessionKey, children: createElement('div', { className: 'ca-thread-said' }, item.text) })));
  return host;
}

const peerItem = (extra: Partial<TranscriptItem> = {}): TranscriptItem => ({
  at: 2000,
  kind: 'peer',
  text: 'Reap the Chrome helpers.',
  from: 'Side conversation',
  failed: false,
  cursor: 10,
  n: 0,
  ...extra,
});

afterEach(() => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  for (const key of opened.splice(0)) {
    act(() => {
      showSides(key, null);
      noteSide({ id: 'clear', key, turns: [], running: false, ended: true });
    });
  }
  document.body.innerHTML = '';
});

describe('PeerCard side conversation link', () => {
  it('links From “Side conversation” to its kept side conversation by fromSide and opens it on click', async () => {
    const key = nextKey();
    const kept: SideKept = { id: 'side-1', key, title: '', startedAt: 1000, lastAt: 1500, turns: [{ question: 'Why is the load high?', answer: 'Chrome helpers.', done: true }] };
    engine.answer('side_get', () => null);
    engine.answer('side_past', () => [kept]);
    const host = mountCard(key, peerItem({ fromSide: 'side-1' }), 2000);
    await act(async () => {
      await settle();
    });
    const link = host.querySelector<HTMLButtonElement>('button.ca-sidelink');
    expect(link?.textContent).toBe('“Side conversation”');
    expect(link?.title).toContain('Why is the load high?');
    act(() => link?.click());
    expect(beingRead(key)).toBe('side-1');
  });

  it('links older side messages without fromSide to the side conversation active then', async () => {
    const key = nextKey();
    const older: SideKept = { id: 'side-old', key, title: '', startedAt: 1000, lastAt: 1200, turns: [{ question: 'Earlier question', answer: 'Done.', done: true }] };
    const newer: SideKept = { id: 'side-new', key, title: '', startedAt: 3000, lastAt: 3200, turns: [{ question: 'Later question', answer: 'Done.', done: true }] };
    engine.answer('side_get', () => null);
    engine.answer('side_past', () => [newer, older]);
    const host = mountCard(key, peerItem({ fromSide: undefined }), 1500);
    await act(async () => {
      await settle();
    });
    const link = host.querySelector<HTMLButtonElement>('button.ca-sidelink');
    expect(link?.title).toContain('Earlier question');
    act(() => link?.click());
    expect(beingRead(key)).toBe('side-old');
  });

  it('renders plain text when the referenced side conversation was forgotten', async () => {
    const key = nextKey();
    engine.answer('side_get', () => null);
    engine.answer('side_past', () => []);
    const host = mountCard(key, peerItem({ fromSide: 'side-gone' }), 2000);
    await act(async () => {
      await settle();
    });
    expect(host.querySelector('button.ca-sidelink')).toBeNull();
    expect(host.querySelector('.ca-msgcard-title')?.textContent).toBe('From “Side conversation”');
  });
});
