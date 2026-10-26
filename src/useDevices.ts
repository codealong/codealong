// Your other devices beside this Mac's board, as every window that shows them follows them: the
// devices view (src-tauri/src/peers.rs) from the `devices` events, their worktrees with what any
// window changed of their sessions that the engine hasn't shown yet (ahead.ts), and the two halves
// observed together, so such a change goes once either shows it. What the engine sends the windows
// that don't get the board (the bar, desktop cards) it cuts from the same two halves
// (src-tauri/src/shown.rs).

import { useEffect, useMemo, useState } from 'react';
import { boardView, lanesAhead, observe, type Ahead } from './ahead';
import { getDevices, onDevices } from './api';
import { reuse } from './reuse';
import type { Board, DevicesView, Lane } from './types';

/** Your other devices as the engine hands them to the windows, followed while `on`: read as it
 *  turns on, then each push, the parts that didn't change kept as they were (reuse.ts). The last
 *  view stays while it's off. */
export function useDevices(on = true): DevicesView | null {
  const [devices, setDevices] = useState<DevicesView | null>(null);
  useEffect(() => {
    if (!on) return;
    let over = false;
    const take = (d: DevicesView) => {
      if (!over) setDevices((prev) => (prev ? reuse(prev, d) : d));
    };
    // One that couldn't be read comes with the next push.
    getDevices().then((d) => d && take(d), () => {});
    const off = onDevices(take);
    return () => {
      over = true;
      off();
    };
  }, [on]);
  return devices;
}

/** Their worktrees, with what any window changed of their sessions that the engine hasn't shown
 *  yet; none while devices are off. */
export function useDeviceLanes(devices: DevicesView | null, ahead: Ahead[]): Lane[] {
  return useMemo(() => (devices?.enabled ? lanesAhead(devices.lanes, ahead) : []), [devices, ahead]);
}

/** While `on`, the window's changes ahead of the engine settle by both halves: `engine`, the board
 *  as the engine sent it, and your devices' worktrees. */
export function useObserve(engine: Pick<Board, 'generatedAt' | 'lanes'> | null, devices: DevicesView | null, on = true): void {
  useEffect(() => {
    if (on) observe(boardView(engine?.generatedAt ?? null, [engine?.lanes ?? [], devices?.lanes ?? []]));
  }, [on, engine, devices]);
}
