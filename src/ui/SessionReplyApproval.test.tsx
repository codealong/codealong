// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { act, engine, settle } from '../testing/dom';
import { createElement } from 'react';
import { createRoot } from 'react-dom/client';
import * as fx from '../testing/fixtures';
import type { Pending, Session } from '../types';
import { Reply } from './SessionReply';

function renderReply(session: Session, deviceName?: string) {
  engine.answer('device_start_options', () => ({ accounts: [], codexProjects: [], claudeModels: [] }));
  const host = document.body.appendChild(document.createElement('div'));
  const root = createRoot(host);
  const onOpen = vi.fn();
  act(() => {
    root.render(createElement(Reply, { session, terminal: 'Rex', deviceName, onOpen }));
  });
  return {
    host,
    onOpen,
    unmount: () => {
      act(() => root.unmount());
      host.remove();
    },
  };
}

const waiting = (over: Partial<Session> = {}): Session =>
  fx.session({
    key: '@mini:codex:codex:01a2097b',
    harness: 'codex',
    account: 'codex',
    status: 'needs',
    live: true,
    surface: 'cli',
    terminal: undefined,
    asks: false,
    hosted: false,
    ...over,
  });

describe('Reply approval box on another device', () => {
  it('shows the remote device name without a broken local terminal button when pending is null', async () => {
    engine.answer('session_pending', () => null);
    engine.answer('session_way', () => ({ kind: 'typed', app: 'tmux' }));
    const { host, unmount } = renderReply(waiting(), 'Gordon’s Mac mini');
    await settle();
    await settle();

    const bar = host.querySelector('.ca-composer.is-waiting');
    expect(bar?.textContent).toContain('Answer on Gordon’s Mac mini');
    expect(bar?.textContent).not.toContain('Rex');
    expect(bar?.querySelector('button')).toBeNull();
    unmount();
  });

  it('shows the remote place and keeps pending.cannot in the tooltip when unanswerable', async () => {
    const cannot: Pending = {
      id: 'p1',
      label: 'Edit files',
      input: '/private/tmp/check.py',
      cannot: 'Codex asks in its own window; answer it there.',
    };
    engine.answer('session_pending', () => cannot);
    engine.answer('session_way', () => ({ kind: 'codex-app' }));
    const { host, unmount } = renderReply(waiting({ surface: 'app' }), 'Gordon’s Mac mini');
    await settle();
    await settle();

    const box = host.querySelector('.ca-composer.is-approval.is-elsewhere');
    const note = box?.querySelector('.ca-composer-note');
    expect(note?.textContent).toBe('Answer in Codex on Gordon’s Mac mini');
    expect(note?.getAttribute('title')).toBe(cannot.cannot);
    expect(box?.querySelector('.ca-composer-foot button')).toBeNull();
    unmount();
  });

  it('shows Allow, Always, and Deny for a remote screen-read Codex approval', async () => {
    const screenPending: Pending = {
      id: 'screen:42:abcdef0123456789',
      label: 'Edit files',
      input: '/private/tmp/codealong-codex-model-check.py (+5 -2)',
      cannot: null,
      always: true,
      alwaysRule: "Yes, and don't ask again for these files",
    };
    engine.answer('session_pending', () => screenPending);
    engine.answer('session_way', () => ({ kind: 'typed', app: 'tmux' }));
    const { host, unmount } = renderReply(waiting(), 'Gordon’s Mac mini');
    await settle();
    await settle();

    const buttons = [...host.querySelectorAll('.ca-composer.is-approval .ca-composer-foot button')].map((b) => b.textContent?.trim());
    expect(buttons.some((b) => b?.includes('Deny'))).toBe(true);
    expect(buttons.some((b) => b?.includes('Always'))).toBe(true);
    expect(buttons.some((b) => b?.includes('Allow'))).toBe(true);
    unmount();
  });

  it('still shows Answer in Rex for a local session when pending is null', async () => {
    engine.answer('session_pending', () => null);
    engine.answer('session_way', () => ({ kind: 'typed', app: 'Rex' }));
    const { host, unmount } = renderReply(waiting({ key: 'codex:codex:local-1' }));
    await settle();
    await settle();

    const button = host.querySelector('.ca-composer.is-waiting button');
    expect(button?.textContent).toContain('Answer in Rex');
    unmount();
  });
});
