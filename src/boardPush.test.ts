import { describe, expect, it, vi } from 'vitest';
import { applyPush, Pushes, type BoardPush } from './boardPush';
import type { Board } from './types';
// What the engine pushes for a run of boards, each with the board a window should have once it's
// put together: written by windowfeed.rs `pushes_fixture_is_current`, so the two sides agree.
import fixture from './testing/boardPushes.json';

const run = fixture as unknown as { push: BoardPush; board: Board }[];

describe('applyPush', () => {
  it('puts together each board the engine pushed', () => {
    let board: Board | null = null;
    for (const [i, { push, board: expected }] of run.entries()) {
      board = applyPush(board, push);
      expect(board, `push ${i}`).toEqual(expected);
    }
  });

  it('keeps what didn’t change as the same objects', () => {
    const first = applyPush(null, run[0].push)!;
    const second = applyPush(first, run[1].push)!;
    // One session in the first lane changed; the other lane, and the PRs, are the same objects.
    expect(second.lanes[1]).toBe(first.lanes[1]);
    expect(second.lanes[0].sessions[0]).toBe(first.lanes[0].sessions[0]);
    expect(second.lanes[0].sessions[1]).not.toBe(first.lanes[0].sessions[1]);
    expect(second.prs).toBe(first.prs);
  });

  it('can’t change a board it isn’t for', () => {
    const first = applyPush(null, run[0].push)!;
    expect(applyPush(first, run[2].push)).toBeNull();
    expect(applyPush(null, run[1].push)).toBeNull();
  });

  it('can’t name a session the board doesn’t have', () => {
    const first = applyPush(null, run[0].push)!;
    const push = { rev: 2, base: first.rev ?? 0, sessionsOf: { [first.lanes[0].id]: ['nowhere'] } };
    expect(applyPush(first, push)).toBeNull();
  });
});

describe('Pushes', () => {
  const pushes = (read: () => Promise<Board>) => {
    const handed: (number | undefined)[] = [];
    return { p: new Pushes(read, (b) => handed.push(b.rev)), handed };
  };
  const board = (i: number) => run[i].board;

  it('hands on each board as its push comes', () => {
    const read = vi.fn(() => Promise.resolve(board(5)));
    const { p, handed } = pushes(read);
    for (const { push } of run) p.take(push);
    expect(handed).toEqual(run.map((r) => r.board.rev));
    expect(read).not.toHaveBeenCalled();
  });

  it('starts from a board read whole, and hands it on when its push comes', () => {
    const { p, handed } = pushes(() => Promise.reject(new Error('not asked')));
    p.saw(board(2));
    p.take(run[2].push);
    p.take(run[3].push);
    expect(handed).toEqual([board(2).rev, board(3).rev]);
  });

  it('reads the board whole for a push it can’t put together, and goes on from it', async () => {
    let answer: (b: Board) => void = () => {};
    const read = vi.fn(() => new Promise<Board>((resolve) => (answer = resolve)));
    const { p, handed } = pushes(read);
    p.saw(board(0));
    // The push for board 2 is missed.
    p.take(run[2].push);
    expect(read).toHaveBeenCalledTimes(1);
    // Pushes that come meanwhile wait for the read.
    p.take(run[3].push);
    p.take(run[4].push);
    expect(handed).toEqual([]);
    // The read has board 3, handed on as it comes; board 4's push changes it.
    answer(board(3));
    await Promise.resolve();
    await Promise.resolve();
    expect(handed).toEqual([board(3).rev, board(4).rev]);
    expect(read).toHaveBeenCalledTimes(1);
    p.take(run[5].push);
    expect(handed).toEqual([board(3).rev, board(4).rev, board(5).rev]);
  });

  it('hands on a board once, however many times its push is heard', () => {
    const { p, handed } = pushes(() => Promise.reject(new Error('not asked')));
    p.take(run[0].push);
    p.take(run[0].push);
    p.take(run[1].push);
    p.take(run[1].push);
    expect(handed).toEqual([board(0).rev, board(1).rev]);
  });
});
