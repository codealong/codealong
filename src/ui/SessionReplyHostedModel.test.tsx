// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { act, engine, settle } from '../testing/dom';
import { createElement } from 'react';
import { createRoot } from 'react-dom/client';
import * as fx from '../testing/fixtures';
import type { Session, StartAccount } from '../types';
import { Reply } from './SessionReply';

// OneLine and Picker use ResizeObserver and scrollIntoView, which jsdom omits.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
Element.prototype.scrollIntoView ??= () => {};

const agAccount: StartAccount = {
  name: 'antigravity',
  harness: 'antigravity',
  chosen: null,
  unavailable: null,
  defaultModel: null,
  models: [
    { id: 'gemini-4-neon-high', label: 'Gemini 4 Neon (High)' },
    { id: 'gemini-4-carbon-high', label: 'Gemini 4 Carbon (High)' },
  ],
  efforts: [],
  places: [
    { place: 'codealong', label: 'Codealong', behind: false },
    { place: 'terminal', label: 'Ghostty', behind: false },
  ],
  switchesModel: false,
  switchesEffort: false,
  switchesMidTurn: false,
  switchesInApp: false,
};

const claudeAccount: StartAccount = {
  name: 'claude',
  harness: 'claude',
  chosen: null,
  unavailable: null,
  defaultModel: null,
  models: [
    { id: 'opus', label: 'Opus' },
    { id: 'sonnet', label: 'Sonnet' },
  ],
  efforts: [
    { id: 'low', label: 'Low' },
    { id: 'medium', label: 'Medium' },
    { id: 'high', label: 'High' },
  ],
  places: [
    { place: 'codealong', label: 'Codealong', behind: false },
    { place: 'terminal', label: 'Ghostty', behind: false },
  ],
  switchesModel: true,
  switchesEffort: true,
  switchesMidTurn: true,
  switchesInApp: false,
};

function renderReply(session: Session) {
  engine.answer('start_options', () => ({ accounts: [agAccount, claudeAccount], codexProjects: [], claudeModels: [] }));
  engine.answer('session_way', () => ({ kind: 'hosted' }));
  engine.answer('session_pending', () => null);
  const host = document.body.appendChild(document.createElement('div'));
  const root = createRoot(host);
  act(() => {
    root.render(createElement(Reply, { session, terminal: 'Ghostty', onOpen: vi.fn() }));
  });
  return {
    host,
    unmount: () => {
      act(() => root.unmount());
      host.remove();
    },
  };
}

describe('Reply model menu on Codealong-hosted sessions', () => {
  it('keeps models enabled mid-turn for Antigravity and queues the switch for the next turn', async () => {
    const switches: Array<{ key: string; model: string }> = [];
    engine.answer('switch_session_model', (args: unknown) => {
      switches.push(args as { key: string; model: string });
      return '“Brainstorm” switches to Gemini 4 Carbon (High) after its current turn.';
    });
    const s = fx.session({
      key: 'antigravity:antigravity:s1',
      title: 'Brainstorm',
      harness: 'antigravity',
      account: 'antigravity',
      status: 'working',
      live: true,
      hosted: true,
      model: 'gemini-4-neon-high',
    });
    const { host, unmount } = renderReply(s);
    await settle();
    await settle();
    await settle();

    const chip = host.querySelector<HTMLButtonElement>('.ca-modelmenu-chip');
    expect(chip).not.toBeNull();
    expect(chip?.title).toContain('from its next turn');

    act(() => chip!.click());
    await settle();

    const opts = Array.from(host.querySelectorAll<HTMLButtonElement>('.ca-modelmenu .ca-picker-opt'));
    expect(opts.length).toBe(2);
    expect(opts.every((b) => !b.disabled)).toBe(true);

    const carbon = opts.find((b) => b.textContent?.includes('Gemini 4 Carbon (High)'));
    expect(carbon).toBeDefined();
    act(() => carbon!.click());
    await settle();

    expect(switches).toEqual([{ key: s.key, model: 'gemini-4-carbon-high' }]);
    expect(host.querySelector('.ca-composer-note')?.textContent).toContain('after its current turn');
    unmount();
  });

  it('keeps model and effort enabled on a hosted session waiting on approval', async () => {
    const efforts: Array<{ key: string; effort: string }> = [];
    engine.answer('switch_session_effort', (args: unknown) => {
      efforts.push(args as { key: string; effort: string });
      return '“Fix” switches to medium effort after its current turn.';
    });
    const s = fx.session({
      key: 'claude:claude:s2',
      title: 'Fix',
      harness: 'claude',
      account: 'claude',
      status: 'needs',
      need: 'approval',
      live: true,
      hosted: true,
      model: 'opus',
      effort: 'high',
    });
    const { host, unmount } = renderReply(s);
    await settle();
    await settle();
    await settle();

    const chip = host.querySelector<HTMLButtonElement>('.ca-modelmenu-chip');
    expect(chip?.title).toContain('from its next turn');
    act(() => chip!.click());
    await settle();

    const opts = Array.from(host.querySelectorAll<HTMLButtonElement>('.ca-modelmenu .ca-picker-opt'));
    expect(opts.every((b) => !b.disabled)).toBe(true);

    const medium = Array.from(host.querySelectorAll<HTMLButtonElement>('.ca-modelmenu .ca-picker-foot [role=radio]')).find((b) => b.textContent?.trim() === 'medium');
    expect(medium).toBeDefined();
    act(() => medium!.click());
    await settle();

    expect(efforts).toEqual([{ key: s.key, effort: 'medium' }]);
    unmount();
  });
});
