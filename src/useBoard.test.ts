// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { act, engine, mountHook, settle } from './testing/dom';
import * as fx from './testing/fixtures';
import type { Board } from './types';
import { useBoard } from './useBoard';

// The board as the main window follows it, from the engine's side: what it's asked and what it
// pushes (`get_board`, the `board` event), through api.ts and the pushes put together
// (boardPush.ts) to the board the window draws. followBoard.test.ts has the rule for a read that
// comes back late, on an api of its own.

const kestrel = fx.session({ key: 'claude:northwind:kestrel', title: 'Kestrel', status: 'working' });
const app = fx.lane({ id: '/r/northwind/app', project: 'Northwind', sessions: [kestrel] });
const docs = fx.lane({ id: '/r/northwind/docs', project: 'Northwind' });

// One engine for the whole file, as a page has one for as long as it lives: each board it makes
// is a rev later than the last, and api.ts keeps the newest it has seen from test to test.
let rev = 0;
/** The engine's next board. */
const next = (over: Partial<Board> = {}): Board => fx.board({ rev: ++rev, generatedAt: fx.NOW + rev * 1000, lanes: [app, docs], ...over });

/** The engine pushes a board whole, as it does the first one a window is sent. */
const push = (board: Board) => act(() => engine.emit('board', { rev: board.rev, full: board }));

/** A window that has read the engine's board and hears its pushes. */
async function following(first: Board = next()) {
  engine.answer('get_board', first);
  const w = mountHook(() => useBoard());
  await settle();
  return w;
}

/** Whether the window can be seen, as the page says it. */
function seen(visible: boolean): void {
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => !visible });
  act(() => void document.dispatchEvent(new Event('visibilitychange')));
}
afterEach(() => {
  Reflect.deleteProperty(document, 'hidden');
});

describe('the board as the main window follows it', () => {
  it('reads the board once it hears the engine’s pushes, and says it’s loading until it has it', async () => {
    engine.answer('get_board', next());
    const w = mountHook(() => useBoard());
    expect([w.current.board, w.current.loading, w.current.loadError]).toEqual([null, true, null]);
    // Asked for only once the listening is in, so no push falls between the two.
    expect(engine.sent('get_board')).toEqual([]);
    await settle();
    expect(engine.listening('board')).toBe(1);
    expect(engine.sent('get_board')).toHaveLength(1);
    expect(w.current.loading).toBe(false);
    expect(w.current.board?.lanes.map((l) => l.id)).toEqual(['/r/northwind/app', '/r/northwind/docs']);
  });

  it('is still loading while the engine’s first scan runs, though a board came', async () => {
    const w = await following(next({ loading: true, lanes: [] }));
    expect([w.current.board?.rev, w.current.loading]).toEqual([rev, true]);
    push(next());
    expect([w.current.board?.rev, w.current.loading]).toEqual([rev, false]);
  });

  it('takes each push, whole or only what changed, and keeps what didn’t change as the objects it had', async () => {
    const w = await following();
    const first = w.current.board;
    // Kestrel's turn ended: the push names only its session.
    const ended = { ...kestrel, status: 'unread' as const };
    const base = rev++;
    act(() => engine.emit('board', { rev, base, fields: { generatedAt: fx.NOW + rev * 1000 }, sessions: { [app.id]: [ended] } }));
    const second = w.current.board;
    expect([second?.rev, second?.lanes[0].sessions[0].status]).toEqual([rev, 'unread']);
    expect(second?.lanes[1]).toBe(first?.lanes[1]);
    // The same board again, whole: the rows are the objects the window has, with nothing to draw anew.
    push(next({ lanes: [{ ...app, sessions: [ended] }, docs] }));
    expect(w.current.board?.rev).toBe(rev);
    expect(w.current.board?.lanes[0]).toBe(second?.lanes[0]);
    expect(engine.sent('get_board')).toHaveLength(1);
  });

  it('reads the board whole when a push changes a board it doesn’t have', async () => {
    const w = await following();
    // It missed a board: what changed since can't be put on the one it has.
    const base = ++rev;
    engine.answer('get_board', next({ lanes: [docs] }));
    act(() => engine.emit('board', { rev, base, laneOrder: [docs.id] }));
    await settle();
    expect(engine.sent('get_board')).toHaveLength(2);
    expect([w.current.board?.rev, w.current.board?.lanes.map((l) => l.id)]).toEqual([rev, ['/r/northwind/docs']]);
  });

  it('keeps only the latest board while the window can’t be seen, and shows it when it’s seen again', async () => {
    const w = await following();
    const had = rev;
    seen(false);
    push(next());
    push(next());
    expect(w.current.board?.rev).toBe(had);
    seen(true);
    expect(w.current.board?.rev).toBe(had + 2);
  });

  it('shows its first board at once, seen or not', async () => {
    seen(false);
    const w = await following();
    expect(w.current.board?.rev).toBe(rev);
  });

  it('says why when the board can’t be read, and stops saying it’s loading', async () => {
    engine.answer('get_board', () => {
      throw new Error('The server isn’t answering.');
    });
    const w = mountHook(() => useBoard());
    await settle();
    expect([w.current.board, w.current.loading, w.current.loadError]).toEqual([null, false, 'Error: The server isn’t answering.']);
  });

  it('stops listening when the window goes, and takes nothing more', async () => {
    const w = await following();
    const last = w.current.board;
    w.unmount();
    await settle();
    expect(engine.listening('board')).toBe(0);
    push(next());
    expect(w.current.board).toBe(last);
  });
});
