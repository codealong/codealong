import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Board } from './types';

// The engine as a window sees it: pushes handed to whoever listens, listening that starts a
// moment after it's asked for, and reads that come back when the test says.
const engine = vi.hoisted(() => ({
  push: null as ((b: Board) => void) | null,
  listening: null as (() => void) | null,
  reads: [] as { resolve: (b: Board) => void; reject: (e: unknown) => void }[],
  unlistened: 0,
}));
vi.mock('./api', () => ({
  onBoard: (cb: (b: Board) => void, listening?: () => void) => {
    engine.push = cb;
    engine.listening = listening ?? null;
    return () => {
      engine.unlistened++;
      engine.push = null;
    };
  },
  getBoard: () => new Promise<Board>((resolve, reject) => engine.reads.push({ resolve, reject })),
}));

import { followBoard, readWins } from './followBoard';
import * as fx from './testing/fixtures';

const at = (generatedAt: number) => fx.board({ generatedAt });
/** Lets what's waiting on a read that came back run. */
const settle = () => new Promise((r) => setTimeout(r, 0));

describe('readWins', () => {
  it('takes a read unless the board taken last was made after it', () => {
    expect(readWins(null, 5)).toBe(true);
    expect(readWins(5, 6)).toBe(true);
    expect(readWins(5, 5)).toBe(true);
    expect(readWins(6, 5)).toBe(false);
  });
});

describe('followBoard', () => {
  beforeEach(() => {
    engine.push = null;
    engine.listening = null;
    engine.reads = [];
    engine.unlistened = 0;
  });

  it('reads the board only once the pushes are listened to', () => {
    const taken: [number, string][] = [];
    followBoard((b, how) => taken.push([b.generatedAt, how]));
    expect(engine.reads).toHaveLength(0);
    engine.listening!();
    expect(engine.reads).toHaveLength(1);
  });

  it('drops a read that comes back after a newer push', async () => {
    const taken: [number, string][] = [];
    followBoard((b, how) => taken.push([b.generatedAt, how]));
    engine.listening!();
    engine.push!(at(2_000));
    engine.reads[0].resolve(at(1_000));
    await settle();
    expect(taken).toEqual([[2_000, 'push']]);
  });

  it('takes a read that is newer than any push, and every push after it', async () => {
    const taken: [number, string][] = [];
    followBoard((b, how) => taken.push([b.generatedAt, how]));
    engine.listening!();
    engine.push!(at(1_000));
    engine.reads[0].resolve(at(2_000));
    await settle();
    engine.push!(at(2_000));
    engine.push!(at(3_000));
    expect(taken).toEqual([
      [1_000, 'push'],
      [2_000, 'read'],
      [2_000, 'push'],
      [3_000, 'push'],
    ]);
  });

  it('takes every push in the order it comes, whatever its time', () => {
    const taken: number[] = [];
    followBoard((b) => taken.push(b.generatedAt));
    engine.push!(at(5_000));
    // A clock set back: the engine's next board is still its latest.
    engine.push!(at(4_000));
    expect(taken).toEqual([5_000, 4_000]);
  });

  it('reads again by the same rule', async () => {
    const taken: [number, string][] = [];
    const following = followBoard((b, how) => taken.push([b.generatedAt, how]));
    engine.listening!();
    engine.reads[0].resolve(at(1_000));
    await settle();
    const failed = vi.fn();
    following.read(failed);
    engine.push!(at(3_000));
    engine.reads[1].resolve(at(2_000));
    following.read(failed);
    engine.reads[2].reject(new Error('gone'));
    await settle();
    expect(taken).toEqual([
      [1_000, 'read'],
      [3_000, 'push'],
    ]);
    expect(failed).toHaveBeenCalledOnce();
  });

  it('says when the first read fails, and takes nothing once it is off', async () => {
    const taken: number[] = [];
    const failed = vi.fn();
    const following = followBoard((b) => taken.push(b.generatedAt), failed);
    engine.listening!();
    engine.reads[0].reject(new Error('no engine'));
    await settle();
    expect(failed).toHaveBeenCalledOnce();

    following.read();
    const push = engine.push!;
    following.off();
    expect(engine.unlistened).toBe(1);
    push(at(9_000));
    engine.reads[1].resolve(at(9_000));
    await settle();
    expect(taken).toEqual([]);
  });
});
