// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { act, engine, settle } from '../testing/dom';
import { beingRead, noteSide, showSides } from '../side';
import type { Glance, PrItem, SideKept, TranscriptItem } from '../types';
import { CodealongCard, PeerCard } from './MessageCard';
import { usePrLinks } from './PrLink';

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

function mountCodealong(glance: Glance, knownPr?: PrItem) {
  const item: TranscriptItem = { at: 2000, kind: 'codealong', text: 'Full message text.', glance, failed: false, cursor: 10, n: 0 };
  const host = document.body.appendChild(document.createElement('div'));
  const root = createRoot(host);
  roots.push(root);
  function Harness() {
    usePrLinks(knownPr ? [knownPr] : [], [], { showSession: () => {}, openSession: () => {}, sessionMenu: () => {}, prMenu: () => {} }, null);
    return createElement(CodealongCard, { item, at: 2000, glance, now: 62_000, children: createElement('div', { className: 'ca-thread-said' }, item.text) });
  }
  act(() => root.render(createElement(Harness)));
  return host;
}

const prGlance = (parts: string[], extra?: Partial<NonNullable<Glance['pr']>>, about: Glance['about'] = 'pr'): Glance => ({
  about,
  pr: { number: 430, title: 'Severity floor 2026-11-27: Cloud SQL Auth Proxy 2.28.0', url: 'https://github.com/o/r/pull/430', ...extra },
  parts: parts.map((line) => ({ line })),
});

describe('CodealongCard PR glance', () => {
  it('weaves #number concisely into PR happenings without the full PR title', () => {
    const cases: [Glance, string][] = [
      [prGlance(['whather approved it', 'conflicts with main']), 'whather approved #430 · conflicts with main'],
      [prGlance(['whather approved it', 'Its branch now conflicts with main, its base']), 'whather approved #430 · conflicts with main'],
      [prGlance(['ana asked for changes']), 'ana asked for changes on #430'],
      [prGlance(['ana asked for changes', 'ana commented on src/a.ts']), 'ana asked for changes on #430 · ana commented on src/a.ts'],
      [prGlance(['ana commented']), 'ana commented on #430'],
      [prGlance(['ana commented on src/a.ts']), 'ana commented on src/a.ts in #430'],
      [prGlance(['ana reviewed it']), 'ana reviewed #430'],
      [prGlance(['ana had a review dismissed']), 'ana had a review dismissed on #430'],
      [prGlance(['Codex, gpt-5.5 reviewed it'], { number: 7 }, 'review'), 'Codex, gpt-5.5 reviewed #7'],
      [prGlance(['Codex, gpt-5.5 reviewed it, with 2 comments on lines'], { number: 7 }, 'review'), 'Codex, gpt-5.5 reviewed #7, with 2 comments on lines'],
      [prGlance(['approved']), '#430 approved'],
      [prGlance(['It was approved']), '#430 approved'],
      [prGlance(['merged']), '#430 merged'],
      [prGlance(['It was merged']), '#430 merged'],
      [prGlance(['closed']), '#430 closed'],
      [prGlance(['It was closed without being merged']), '#430 closed'],
      [prGlance(['changes asked for']), 'Changes asked for on #430'],
      [prGlance(['Changes were asked for']), 'Changes asked for on #430'],
      [prGlance(['checks failed: lint, test']), '#430 checks failed: lint, test'],
      [prGlance(['Its checks failed: lint, test']), '#430 checks failed: lint, test'],
      [prGlance(['conflicts with main']), '#430 conflicts with main'],
      [prGlance(['Its branch now conflicts with its base']), '#430 conflicts with its base'],
      [prGlance(['something unexpected']), '#430 · something unexpected'],
      [prGlance([]), '#430'],
    ];
    for (const [glance, expected] of cases) {
      const host = mountCodealong(glance);
      expect(host.querySelector('.ca-msgcard-title')?.textContent).toBe(expected);
    }
  });

  it('omits the redundant inline PR icon even when the PR is known on the board', () => {
    const known = { key: 'o/r#430', repoSlug: 'o/r', number: 430, title: 'Severity floor', url: 'https://github.com/o/r/pull/430', state: 'approved' } as PrItem;
    const host = mountCodealong(prGlance(['whather approved it', 'conflicts with main']), known);
    const link = host.querySelector<HTMLAnchorElement>('.ca-msgcard-title a.ca-prlink');
    expect(link?.textContent).toBe('#430');
    expect(link?.querySelector('.ca-prlink-icon')).toBeNull();
    expect(host.querySelectorAll('.ca-msgcard-head .ca-icon').length).toBe(2); // leading PR icon + expand chevron
  });

  it('shows the PR title on hover when the glance has no PR link', () => {
    const host = mountCodealong(prGlance(['Codex reviewed it'], { number: 7, title: 'Add teams', url: undefined }, 'review'));
    const named = host.querySelector<HTMLSpanElement>('.ca-msgcard-named');
    expect(named?.textContent).toBe('#7');
    expect(named?.title).toBe('Add teams');
  });
});
