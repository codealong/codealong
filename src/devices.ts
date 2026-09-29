// Your other devices (src-tauri/src/peers.rs): their worktrees and sessions come in with
// `@<short>:` in front of every lane id and session key, so which device something is on can be
// read off its key, and a click on it goes back to that device.

import type { DeviceView, DevicesView, Lane, Session } from './types';

/** The short id of the device a lane id or session key is on; null when it's this device's. */
export function shortOf(key: string): string | null {
  if (!key.startsWith('@')) return null;
  const end = key.indexOf(':');
  return end > 1 ? key.slice(1, end) : null;
}

/** The linked device a lane id or session key is on, when it's another device's. */
export function deviceOf(devices: DevicesView | null | undefined, key: string): DeviceView | null {
  const short = shortOf(key);
  if (!short || !devices) return null;
  return devices.devices.find((d) => d.short === short) ?? null;
}

/** Which devices' work the views show: the ones picked, `here` for this one and the others by
 *  short id. None picked shows every device's. */
export type DeviceFilter = readonly string[];

/** This device's worktrees and the linked devices', as the filter says. */
export function withDevices(here: Lane[], there: Lane[], picked: DeviceFilter): Lane[] {
  if (picked.length === 0) return there.length === 0 ? here : here.concat(there);
  const mine = picked.includes('here') ? here : [];
  const theirs = there.filter((l) => picked.includes(shortOf(l.id) ?? ''));
  return theirs.length === 0 ? mine : mine.concat(theirs);
}

/** How many of a device's sessions need you, for its row in the sidebar. */
export function needsOn(lanes: Lane[], short: string | null): number {
  return countOn(lanes, short, (s) => s.status === 'needs' || s.status === 'error');
}

/** How many of a device's sessions are working, for its row in the sidebar. */
export function workingOn(lanes: Lane[], short: string | null): number {
  return countOn(lanes, short, (s) => s.status === 'working');
}

function countOn(lanes: Lane[], short: string | null, counts: (s: Session) => boolean): number {
  return lanes.filter((l) => shortOf(l.id) === short).reduce((n, l) => n + l.sessions.filter((s) => !s.visitor && !s.archived && counts(s)).length, 0);
}

/** A device's name as its badge shows it: without its owner's name in front, so "Sam’s MacBook
 *  Pro" reads "MacBook Pro". */
export function deviceLabel(name: string): string {
  return name.replace(/^[^\s’']+[’']s\s+(?=\S)/, '') || name;
}

/** Lanes split by the device they're on, for grouping: this device's first, then the others' in
 *  the order they were linked, each only when it has any. */
export function byDevice(lanes: Lane[], devices: DevicesView | null | undefined): { short: string | null; lanes: Lane[] }[] {
  const order: (string | null)[] = [null, ...(devices?.devices ?? []).map((d) => d.short)];
  const groups = new Map<string | null, Lane[]>(order.map((s) => [s, []]));
  for (const l of lanes) {
    const short = shortOf(l.id);
    if (!groups.has(short)) groups.set(short, []);
    (groups.get(short) as Lane[]).push(l);
  }
  return [...groups].filter(([, ls]) => ls.length > 0).map(([short, ls]) => ({ short, lanes: ls }));
}
