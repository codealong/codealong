// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, engine, mountHook, settle } from './testing/dom';
import type { Reveal } from './api';
import { sessionLink, todoLink } from './codealongLinks';
import * as fx from './testing/fixtures';
import type { Board, DevicesView, Lane, Todo } from './types';
import { usePrLinks } from './ui/PrLink';
import { DEVICES_WAIT_MS, useRevealAsks, type RevealAsksInput } from './useRevealAsks';

// What the main window is asked to show from outside it (useRevealAsks.ts): what another window
// asks, and a link macOS opened, taken and shown once the board is read.

const kestrel = fx.session({ key: 'claude:northwind:kestrel' });
const app = fx.lane({ id: '/r/northwind/app', sessions: [kestrel] });
const todo = fx.todo({ id: 't-importer' });
const read = (over: Partial<Board> = {}): Board => fx.board({ lanes: [app], todos: [todo], ...over });
/** A session on another of your devices, in a worktree of its own there. */
const farKey = '@m1:claude:atlas:far';
const far: Lane = fx.lane({ id: '@m1:/r/atlas/site', sessions: [fx.session({ key: farKey })] });

let did: string[] = [];
let revealed: Reveal[] = [];
// How the window shows things and says things: the same from render to render, as its own are
// (a value kept for later with `useLatest`, whose `current` is each render's way of showing, and
// the toasts' own `setToast`).
const reveal = { current: (_: Reveal): void => {} };
const setToast = (m: string) => void did.push(`say “${m}”`);
beforeEach(() => {
  did = [];
  revealed = [];
  reveal.current = (r) => void revealed.push(r);
});
afterEach(() => {
  vi.useRealTimers();
});

interface Has {
  board: Board | null;
  devices: DevicesView | null;
  deviceLanes: Lane[];
  todos: Todo[];
}
// What the window has until a test says otherwise, the same objects from render to render: a
// render a test asks for changes only what the test changes.
const first: Has = { board: read(), devices: fx.devicesView(), deviceLanes: [], todos: [todo] };
const has = (over: Partial<Has> = {}): Has => ({ ...first, ...over });
const links = { showSession: () => {}, openSession: () => {}, sessionMenu: () => {}, prMenu: () => {} };
/** The window: its links know its sessions (usePrLinks, as App.tsx gives them), and it hears what
 *  it's asked to show. Answers what `takeLink` is. */
function useWindow(p: Has) {
  usePrLinks(p.board?.prs, p.board?.lanes, links, p.board, p.deviceLanes);
  const input: RevealAsksInput = { board: p.board, devices: p.devices, deviceLanes: p.deviceLanes, reveal, setToast, todoStore: { todos: p.todos } };
  return useRevealAsks(input);
}
/** macOS opened `url` with Codealong: the engine gives it once, to the first that asks. */
function opened(url: string): void {
  let given = false;
  engine.answer('take_opened_link', () => {
    if (given) return null;
    given = true;
    return url;
  });
}
/** The window once it hears what it's asked to show. */
async function mount(over: Partial<Has> = {}) {
  const w = mountHook(useWindow, has(over));
  await settle();
  return w;
}

describe('what another window asks this one to show', () => {
  it('is shown as the latest render shows things, and only what’s sent to this window', async () => {
    await mount();
    // A render since the window began to hear shows things its own way: what's asked now goes
    // to that, not to the way of the render that began to hear.
    const later: Reveal[] = [];
    reveal.current = (r) => void later.push(r);
    act(() => engine.emitTo('main', 'reveal', { kind: 'session', laneId: app.id, key: kestrel.key }));
    act(() => engine.emitTo('quick', 'reveal', { kind: 'todo', id: 't-importer' }));
    expect([revealed, later]).toEqual([[], [{ kind: 'session', laneId: app.id, key: kestrel.key }]]);
  });

  it('stops hearing once the window goes', async () => {
    const w = await mount();
    w.unmount();
    await settle();
    expect(engine.listening('reveal')).toBe(0);
  });
});

describe('a link macOS opened', () => {
  it('is asked for once the window hears what it’s told to show, not before', async () => {
    const letIn = engine.holdListening();
    mountHook(useWindow, has());
    await settle();
    expect(engine.sent('take_opened_link')).toEqual([]);
    letIn();
    await settle();
    expect(engine.sent('take_opened_link')).toHaveLength(1);
    expect([revealed, did]).toEqual([[], []]);
  });

  it('shows the session it names, once the board has been read', async () => {
    opened(sessionLink(kestrel.key));
    const w = await mount({ board: null });
    expect(revealed).toEqual([]);
    w.rerender(has({ board: read({ loading: true, lanes: [] }) }));
    expect(revealed).toEqual([]);
    w.rerender(has());
    expect(revealed).toEqual([{ kind: 'session', laneId: app.id, key: kestrel.key }]);
    // Shown once: a later board doesn't show it again.
    w.rerender(has({ board: read({ rev: 2 }) }));
    expect(revealed).toHaveLength(1);
  });

  it('shows the todo it names, or says there’s none', async () => {
    opened(todoLink('t-importer'));
    await mount();
    expect(revealed).toEqual([{ kind: 'todo', id: 't-importer' }]);
    opened(todoLink('t-gone'));
    await mount();
    expect(did).toEqual(['say “There’s no todo t-gone here.”']);
  });

  it('is taken again when the app says there’s one, as the window’s `reveal` asks', async () => {
    const w = await mount();
    opened(sessionLink(kestrel.key));
    act(() => w.current());
    await settle();
    expect(engine.sent('take_opened_link')).toHaveLength(2);
    expect(revealed).toEqual([{ kind: 'session', laneId: app.id, key: kestrel.key }]);
  });

  it('says nothing of a link that can’t be asked for', async () => {
    engine.answer('take_opened_link', () => {
      throw new Error('The server isn’t answering.');
    });
    await mount();
    expect([revealed, did]).toEqual([[], []]);
  });
});

describe('a link to a session the window doesn’t list', () => {
  it('is said to be missing at once when every device has been read', async () => {
    opened(sessionLink('claude:northwind:gone'));
    await mount();
    expect(did).toEqual(['say “There’s no session claude:northwind:gone here.”']);
  });

  it('waits for a device still reading its sessions, and shows the session as that device’s worktrees come', async () => {
    opened(sessionLink(farKey));
    const reading = fx.devicesView({ devices: [fx.deviceView({ short: 'm1', loading: true })] });
    const w = await mount({ devices: reading });
    expect([revealed, did]).toEqual([[], []]);
    // Its worktrees come and nothing else does: the board, the todos and what's known of the
    // devices are what they were.
    w.rerender(has({ devices: reading, deviceLanes: [far] }));
    expect(revealed).toEqual([{ kind: 'session', laneId: far.id, key: farKey }]);
    expect(did).toEqual([]);
  });

  it('waits while your devices haven’t been read at all, and says it’s missing once they have without it', async () => {
    opened(sessionLink(farKey));
    const w = await mount({ devices: null });
    expect(did).toEqual([]);
    // They're read and nothing else changes: the board, the todos and their worktrees (none) are
    // what they were.
    w.rerender(has({ devices: fx.devicesView({ devices: [fx.deviceView({ short: 'm1' })] }) }));
    expect(did).toEqual([`say “There’s no session ${farKey} here.”`]);
  });

  it('says it’s missing after a while when a device never finishes reading', async () => {
    vi.useFakeTimers();
    opened(sessionLink(farKey));
    const reading = fx.devicesView({ devices: [fx.deviceView({ short: 'm1', loading: true })] });
    mountHook(useWindow, has({ devices: reading }));
    // The link is taken, and the wait for the device begins.
    await act(() => vi.advanceTimersByTimeAsync(0));
    await act(() => vi.advanceTimersByTimeAsync(DEVICES_WAIT_MS - 500));
    expect(did).toEqual([]);
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(did).toEqual([`say “There’s no session ${farKey} here.”`]);
  });
});
