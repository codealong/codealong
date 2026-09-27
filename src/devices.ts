// Your other Macs in the windows (src-tauri/src/peers.rs): their worktrees and sessions come in
// with `@<short>:` in front of every lane id and session key, so which Mac something is on can be
// read off its key, and a click on it goes back to that Mac.

import type { DeviceView, DevicesView, Lane } from './types';

/** The short id of the Mac a lane id or session key is on; null when it's this Mac's. */
export function macOf(key: string): string | null {
  if (!key.startsWith('@')) return null;
  const end = key.indexOf(':');
  return end > 1 ? key.slice(1, end) : null;
}

/** The linked Mac a lane id or session key is on, when it's another Mac's. */
export function deviceOf(devices: DevicesView | null | undefined, key: string): DeviceView | null {
  const short = macOf(key);
  if (!short || !devices) return null;
  return devices.devices.find((d) => d.short === short) ?? null;
}

/** Which Macs' work the views show: all of them, this one, or one other by its short id. */
export type MacFilter = null | 'here' | string;

/** This Mac's worktrees and the linked Macs', as the filter says. */
export function withMacs(here: Lane[], there: Lane[], filter: MacFilter): Lane[] {
  if (filter === 'here') return here;
  if (filter === null) return there.length === 0 ? here : here.concat(there);
  return there.filter((l) => macOf(l.id) === filter);
}

/** How many of a Mac's sessions need you, for its row in the sidebar. */
export function needsOn(lanes: Lane[], short: string | null): number {
  return lanes.filter((l) => macOf(l.id) === short).reduce((n, l) => n + l.sessions.filter((s) => !s.visitor && !s.archived && (s.status === 'needs' || s.status === 'error')).length, 0);
}
