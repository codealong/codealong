// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, engine, mountHook } from './testing/dom';
import type { Reveal } from './api';
import { drawerOf, sessionViewOf, TERMINAL_WAIT_MS } from './terminals';
import * as fx from './testing/fixtures';
import type { Board, TerminalGlance } from './types';
import { useShownTerminal } from './useShownTerminal';

// A terminal of Codealong's the server asked the main window to show (useShownTerminal.ts):
// where it goes once the board says whose it is, and how long the window waits for that.

const terminal = (id: string, session: string, over: Partial<TerminalGlance> = {}): TerminalGlance => ({ id, session, title: 'zsh', shell: 'zsh', folder: '/r/northwind/app', startedAt: fx.NOW, running: true, ...over });
/** A board with one session in a worktree, and the terminals a test says, by the session each is for. */
function board(key: string, terminals: TerminalGlance[] = []): Board {
  const lane = fx.lane({ id: '/r/northwind/app', sessions: [fx.session({ key })] });
  const byKey: Record<string, TerminalGlance[]> = {};
  for (const t of terminals) (byKey[t.session] ??= []).push(t);
  return fx.board({ lanes: [lane], terminals: byKey });
}

let did: string[] = [];
let revealed: Reveal[] = [];
beforeEach(() => {
  did = [];
  revealed = [];
  // Showing one starts a wait: on a clock the test moves, so none outlasts its test.
  vi.useFakeTimers();
  // No terminal has a window of its own, until the engine says one has.
  engine.answer('terminal_windows', []);
});
afterEach(() => {
  vi.useRealTimers();
});

/** Lets what waits on the engine's answer run, and `ms` go by. */
const pass = (ms = 0) => act(() => vi.advanceTimersByTimeAsync(ms));
const has = (b: Board | null) => ({ board: b, reveal: { current: (r: Reveal) => void revealed.push(r) }, setToast: (m: string) => void did.push(`say “${m}”`) });
/** The window, and the server asking it to show the terminal `id`, as the window's `reveal` asks. */
async function asked(b: Board | null, id: string, wait = false) {
  const w = mountHook(useShownTerminal, has(b));
  await pass();
  const ask = () => act(() => w.current.showTerminal(id, wait));
  return { w, ask };
}

describe('a terminal the server asked the window to show', () => {
  it('shows its session with the terminal’s tab open in its drawer, for one beside a session', async () => {
    const key = 'claude:northwind:tab';
    const { ask } = await asked(board(key, [terminal('t1', key)]), 't1');
    ask();
    expect(revealed).toEqual([{ kind: 'session', laneId: '/r/northwind/app', key }]);
    expect(drawerOf(key)).toMatchObject({ open: true, active: 't1' });
    expect(drawerOf(key).focusAt).toBeGreaterThan(0);
    expect(sessionViewOf(key)).toBe('conversation');
    // Placed once: the look once the wait is up finds nothing left to place.
    await pass(TERMINAL_WAIT_MS + 200);
    expect(revealed).toHaveLength(1);
  });

  it('shows the terminal in its session’s panel, for the one the session runs in', async () => {
    const key = 'claude:northwind:home';
    const { ask } = await asked(board(key, [terminal('t1', key, { hosts: true })]), 't1');
    ask();
    expect(revealed).toEqual([{ kind: 'session', laneId: '/r/northwind/app', key }]);
    expect(sessionViewOf(key)).toBe('terminal');
    expect(drawerOf(key).homeAt).toBeGreaterThan(0);
    expect(drawerOf(key).open).toBe(false);
  });

  it('brings one that’s no session’s forward in a window of its own', async () => {
    const { ask } = await asked(board('claude:northwind:other', [terminal('t9', '')]), 't9');
    ask();
    await pass();
    expect(engine.sent('terminal_window')).toEqual([{ key: '', id: 't9', keys: true }]);
    expect(revealed).toEqual([]);
    // Once: not again when the wait is up.
    await pass(TERMINAL_WAIT_MS + 200);
    expect(engine.sent('terminal_window')).toHaveLength(1);
  });

  it('brings a session’s terminal forward in its own window when it’s popped out', async () => {
    const key = 'claude:northwind:popped';
    const { ask } = await asked(board(key, [terminal('t2', key)]), 't2');
    act(() => engine.emit('terminal-windows', ['t2']));
    ask();
    await pass();
    expect(engine.sent('terminal_window')).toEqual([{ key, id: 't2', keys: true }]);
    expect([revealed, drawerOf(key).open]).toEqual([[], false]);
    act(() => engine.emit('terminal-windows', []));
  });

  it('says why when its window can’t be shown', async () => {
    engine.answer('terminal_window', () => {
      throw new Error('That terminal has ended.');
    });
    const { ask } = await asked(board('claude:northwind:other', [terminal('t9', '')]), 't9');
    ask();
    await pass();
    expect(did).toEqual(['say “Error: That terminal has ended.”']);
  });
});

describe('waiting for the board to say whose it is', () => {
  it('waits for one the board doesn’t list yet, and places it as the board that lists it comes', async () => {
    const key = 'claude:northwind:late';
    const { w, ask } = await asked(board(key), 't1');
    ask();
    expect(revealed).toEqual([]);
    w.rerender(has(board(key, [terminal('t1', key)])));
    expect(revealed).toEqual([{ kind: 'session', laneId: '/r/northwind/app', key }]);
    // Placed once: a later board doesn't show it again.
    w.rerender(has(board(key, [terminal('t1', key)])));
    expect(revealed).toHaveLength(1);
  });

  it('waits for one opened for a command, which may turn out to be a session’s, then gives it a window though no board comes', async () => {
    const { ask } = await asked(board('claude:northwind:other', [terminal('t9', '')]), 't9', true);
    ask();
    await pass(TERMINAL_WAIT_MS - 100);
    expect(engine.sent('terminal_window')).toEqual([]);
    // The wait is up on a quiet board: the window looks again by itself.
    await pass(300);
    expect(engine.sent('terminal_window')).toEqual([{ key: '', id: 't9', keys: true }]);
    expect(revealed).toEqual([]);
  });

  it('shows one opened for a command in its session’s drawer when the board says in time that it’s a session’s', async () => {
    const key = 'claude:northwind:started';
    const { w, ask } = await asked(board(key, [terminal('t9', '')]), 't9', true);
    ask();
    await pass(1000);
    w.rerender(has(board(key, [terminal('t9', key)])));
    expect(revealed).toEqual([{ kind: 'session', laneId: '/r/northwind/app', key }]);
    await pass(TERMINAL_WAIT_MS);
    expect(engine.sent('terminal_window')).toEqual([]);
  });

  it('lets go of one the board never lists, so a board that lists it later shows nothing', async () => {
    const { w, ask } = await asked(board('claude:northwind:other'), 'gone');
    ask();
    await pass(TERMINAL_WAIT_MS + 200);
    w.rerender(has(board('claude:northwind:other', [terminal('gone', '')])));
    await pass();
    expect([revealed, engine.sent('terminal_window')]).toEqual([[], []]);
  });

  it('does nothing with none asked for, whatever the board says', async () => {
    const key = 'claude:northwind:idle';
    const w = mountHook(useShownTerminal, has(board(key, [terminal('t1', key)])));
    await pass();
    w.rerender(has(board(key, [terminal('t1', key)])));
    expect([revealed, drawerOf(key).open]).toEqual([[], false]);
  });
});
