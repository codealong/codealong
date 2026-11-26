// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { act, mountHook } from './testing/dom';
import type { MoveFollow } from './handoff';
import * as fx from './testing/fixtures';
import type { Lane, Session } from './types';
import { useMoveFollow } from './useMoveFollow';

describe('useMoveFollow', () => {
  it('follows a synchronous key and an asynchronous origin fork while ignoring older forks', async () => {
    const from = 'claude:a:1';
    const oldFork = fx.session({ key: 'codex:b:old', origin: { key: from, how: 'fork' }, startedAt: 100 });
    const newCopy = fx.session({ key: 'codex:b:new', origin: { key: from, how: 'fork' }, startedAt: 200 });
    const followed: string[] = [];

    const baseLane = fx.lane({ id: '/r/app', sessions: [fx.session({ key: from }), oldFork] });
    const hook = mountHook(
      ({ shownKey, lanes }: { shownKey: string | null; lanes: readonly Lane[] }) =>
        useMoveFollow(shownKey, lanes, (copy: { lane: Lane; session: Session }) => followed.push(copy.session.key)),
      { shownKey: from, lanes: [baseLane] },
    );

    // Async launch (`origin`): ignores `oldFork` that already existed before the move started.
    let settle: (v: MoveFollow) => void = () => {};
    act(() => {
      hook.current.start(from, new Promise((r) => (settle = r)));
    });
    expect(hook.current.moving).toBe(true);
    await act(async () => {
      settle({ kind: 'origin' });
      await Promise.resolve();
    });
    expect(followed).toEqual([]);
    expect(hook.current.moving).toBe(true);

    // Once the new fork lands on the board, `onFollow` fires and `moving` clears.
    const nextLane = fx.lane({ id: '/r/app', sessions: [fx.session({ key: from }), oldFork, newCopy] });
    hook.rerender({ shownKey: from, lanes: [nextLane] });
    expect(followed).toEqual(['codex:b:new']);
    expect(hook.current.moving).toBe(false);

    // Window switches to the followed session, then a synchronous key move follows the exact key.
    hook.rerender({ shownKey: 'codex:b:new', lanes: [nextLane] });
    const syncCopy = fx.session({ key: 'claude:c2:9' });
    await act(async () => {
      hook.current.start('codex:b:new', Promise.resolve({ kind: 'key', key: syncCopy.key }));
      await Promise.resolve();
    });
    hook.rerender({ shownKey: 'codex:b:new', lanes: [fx.lane({ id: '/r/app', sessions: [newCopy, syncCopy] })] });
    expect(followed).toEqual(['codex:b:new', 'claude:c2:9']);
    expect(hook.current.moving).toBe(false);

    // A failed move (resolving `null`) clears `moving` without calling `onFollow`.
    await act(async () => {
      hook.current.start(syncCopy.key, Promise.resolve(null));
      await Promise.resolve();
    });
    expect(hook.current.moving).toBe(false);
    expect(followed).toEqual(['codex:b:new', 'claude:c2:9']);

    hook.unmount();
  });
});
