// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, engine, mountHook } from './testing/dom';
import type { Reveal } from './api';
import { NEW_SESSION_WAIT_MS, type NewSession, type NewSessionShown } from './newSession';
import * as fx from './testing/fixtures';
import type { Board, Lane, Started } from './types';
import { useNewSession, type NewSessionInput } from './useNewSession';

// A start left for its first prompt, as the main window keeps it (useNewSession.ts): how long its
// panel stays, what a send does, and how the session it started takes its place.

const app = fx.lane({ id: '/r/northwind/app', project: 'Northwind' });
const draft: NewSession = { spec: { laneId: app.id, account: 'northwind', place: 'background', prompt: '', model: null, effort: null, worktree: false, todos: [] }, does: 'Start in the background', placeLabel: 'Background', harness: 'claude' };
const KEY = 'claude:northwind:kestrel';
/** The board once it lists the session a send started. */
const listing = (): Board => fx.board({ lanes: [{ ...app, sessions: [fx.session({ key: KEY })] }] });

let did: string[] = [];
let revealed: Reveal[] = [];
beforeEach(() => {
  did = [];
  revealed = [];
});
afterEach(() => {
  vi.useRealTimers();
});

/** The window with `selectedKey` selected, on a board that lists no session yet. */
function has(over: Partial<NewSessionInput> = {}): NewSessionInput {
  return {
    selectedKey: 'row-a',
    board: fx.board({ lanes: [app] }),
    deviceLanes: [],
    reveal: { current: (r) => void revealed.push(r) },
    setBoxFocus: (key) => void did.push(`cursor in ${key}`),
    setToast: (m) => void did.push(`say “${m}”`),
    ...over,
  };
}
/** A new session opened over the row selected, as the quick panel's start leaves one. */
const opened = (over: Partial<NewSessionShown> = {}): NewSessionShown => ({ draft, over: 'row-a', opened: 1, sending: null, error: null, started: null, ...over });

describe('a new session waiting for its first prompt', () => {
  it('stays while the row it opened over is selected, and goes once another is picked', () => {
    const w = mountHook(useNewSession, has());
    act(() => w.current.setNewShown(opened()));
    w.rerender(has({ board: fx.board({ lanes: [app], rev: 2 }) }));
    expect(w.current.newShown).toEqual(opened());
    w.rerender(has({ selectedKey: 'row-b' }));
    expect(w.current.newShown).toBeNull();
  });

  it('takes the row the window selects by itself when it opened with nothing selected', () => {
    const w = mountHook(useNewSession, has({ selectedKey: null }));
    act(() => w.current.setNewShown(opened({ over: null })));
    w.rerender(has({ selectedKey: 'row-a' }));
    expect(w.current.newShown?.over).toBe('row-a');
    w.rerender(has({ selectedKey: 'row-b' }));
    expect(w.current.newShown).toBeNull();
  });
});

describe('sending it', () => {
  // A start that gives a session to wait for starts the wait for it (`NEW_SESSION_WAIT_MS`): on a
  // clock the test moves, so no wait outlasts its test and says its piece in a later one.
  beforeEach(() => {
    vi.useFakeTimers();
  });
  /** Lets what waits on the engine's answer run. */
  const answered = () => act(() => vi.advanceTimersByTimeAsync(0));

  it('starts the session on what was written, saying it’s on its way, and then waits for the board to list it', async () => {
    let answer: (started: Started | null) => void = () => {};
    engine.answer('start_from_quick', () => new Promise<Started | null>((resolve) => (answer = resolve)));
    const w = mountHook(useNewSession, has());
    act(() => w.current.setNewShown(opened()));
    let sent: Promise<void> = Promise.resolve();
    act(() => void (sent = w.current.sendNew(opened())('Fix the importer', 'fix the importer')));
    expect(w.current.newShown).toMatchObject({ sending: 'fix the importer', error: null, started: null });
    await answered();
    expect(engine.sent('start_from_quick')).toEqual([{ spec: { ...draft.spec, prompt: 'Fix the importer' } }]);
    answer({ key: KEY, message: null });
    await act(() => sent);
    expect(w.current.newShown?.started).toMatchObject({ key: KEY, message: null });
    expect([revealed, did]).toEqual([[], []]);
  });

  it('gives way to the session it started once the board lists it, with the cursor in its box', async () => {
    engine.answer('start_from_quick', { key: KEY, message: null });
    const w = mountHook(useNewSession, has());
    act(() => w.current.setNewShown(opened()));
    await act(() => w.current.sendNew(opened())('Fix the importer', 'fix the importer'));
    w.rerender(has({ board: listing() }));
    expect(w.current.newShown).toBeNull();
    expect(revealed).toEqual([{ kind: 'session', laneId: app.id, key: KEY }]);
    expect(did).toEqual([`cursor in ${app.id}::${KEY}`]);
  });

  it('finds the session on another device’s worktree too', async () => {
    engine.answer('start_from_quick', { key: KEY, message: null });
    const w = mountHook(useNewSession, has());
    act(() => w.current.setNewShown(opened()));
    await act(() => w.current.sendNew(opened())('Fix the importer', 'fix the importer'));
    const far: Lane = { ...app, id: '@m1:/r/northwind/app', sessions: [fx.session({ key: KEY })] };
    w.rerender(has({ deviceLanes: [far] }));
    expect(revealed).toEqual([{ kind: 'session', laneId: far.id, key: KEY }]);
  });

  it('stops waiting after a while, and says what the start said instead', async () => {
    engine.answer('start_from_quick', { key: KEY, message: 'Started in the background.' });
    const w = mountHook(useNewSession, has());
    act(() => w.current.setNewShown(opened()));
    await act(() => w.current.sendNew(opened())('Fix the importer', 'fix the importer'));
    await act(() => vi.advanceTimersByTimeAsync(NEW_SESSION_WAIT_MS - 1000));
    expect(w.current.newShown?.started?.key).toBe(KEY);
    await act(() => vi.advanceTimersByTimeAsync(2000));
    expect(w.current.newShown).toBeNull();
    expect([did, revealed]).toEqual([['say “Started in the background.”'], []]);
  });

  it('says so itself when the start said nothing', async () => {
    engine.answer('start_from_quick', { key: KEY, message: null });
    const w = mountHook(useNewSession, has());
    act(() => w.current.setNewShown(opened()));
    await act(() => w.current.sendNew(opened())('Fix the importer', 'fix the importer'));
    await act(() => vi.advanceTimersByTimeAsync(NEW_SESSION_WAIT_MS + 1000));
    expect(did).toEqual(['say “It started, and shows here once it’s listed.”']);
  });

  it('goes at once, saying what the start said, when the start gives no session to wait for', async () => {
    engine.answer('start_from_quick', { key: null, message: 'Opened the Codex app on a new thread.' });
    const w = mountHook(useNewSession, has());
    act(() => w.current.setNewShown(opened()));
    await act(() => w.current.sendNew(opened())('Fix the importer', 'fix the importer'));
    expect(w.current.newShown).toBeNull();
    expect(did).toEqual(['say “Opened the Codex app on a new thread.”']);
    // Nothing at all said: it just goes.
    did = [];
    engine.answer('start_from_quick', null);
    act(() => w.current.setNewShown(opened({ opened: 2 })));
    await act(() => w.current.sendNew(opened({ opened: 2 }))('Again', 'again'));
    expect([w.current.newShown, did]).toEqual([null, []]);
  });

  it('keeps the panel with why the start failed, and rejects, so what was written stays in its box', async () => {
    engine.answer('start_from_quick', () => {
      throw new Error('That account is signed out.');
    });
    const w = mountHook(useNewSession, has());
    act(() => w.current.setNewShown(opened()));
    let failed: unknown = null;
    await act(() => w.current.sendNew(opened())('Fix the importer', 'fix the importer').catch((e: unknown) => void (failed = e)));
    expect(String(failed)).toContain('That account is signed out.');
    expect(w.current.newShown).toMatchObject({ sending: null, error: 'That account is signed out.', started: null });
  });

  it('answers only the open that sent it, never one opened after it', async () => {
    let answer: (started: Started | null) => void = () => {};
    engine.answer('start_from_quick', () => new Promise<Started | null>((resolve) => (answer = resolve)));
    const w = mountHook(useNewSession, has());
    act(() => w.current.setNewShown(opened()));
    let sent: Promise<void> = Promise.resolve();
    act(() => void (sent = w.current.sendNew(opened())('Fix the importer', 'fix the importer')));
    await answered();
    // Another start opened meanwhile, in the same place.
    act(() => w.current.setNewShown(opened({ opened: 2 })));
    answer({ key: KEY, message: null });
    await act(() => sent);
    expect(w.current.newShown).toEqual(opened({ opened: 2 }));
  });
});
