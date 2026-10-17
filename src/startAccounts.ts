// This Mac's accounts as a start or a resume offers them (`start-options`: where each can start a
// session, what its command line takes, where one of its sessions that isn't running can go on),
// read once a window needs them and kept a minute: the cards, boxes and menus that offer them
// would each ask the engine again otherwise. Another device's are asked of it and kept the same
// way, for the box of a session that's on it.

import { useEffect, useState } from 'react';
import { deviceStartOptions, startOptions } from './api';
import { withLists } from './start';
import type { RoutingGroup, Session, StartAccount } from './types';

const FRESH_MS = 60_000;
let read: { at: number; accounts: StartAccount[]; groups: RoutingGroup[] } | null = null;
let reading: Promise<{ accounts: StartAccount[]; groups: RoutingGroup[] }> | null = null;

/** Reads them again once what's kept is a minute old, and resolves with what's kept. */
function refresh(): Promise<{ accounts: StartAccount[]; groups: RoutingGroup[] }> {
  if (read && Date.now() - read.at < FRESH_MS) return Promise.resolve({ accounts: read.accounts, groups: read.groups });
  reading ??= startOptions()
    .then(
      (o) => {
        read = { at: Date.now(), accounts: withLists(o), groups: o.groups ?? [] };
        return { accounts: read.accounts, groups: read.groups };
      },
      // The last read stands, and what offers them goes without until a read works: the quick
      // panel and the main window say so when start options can't be read.
      () => ({ accounts: read?.accounts ?? [], groups: read?.groups ?? [] }),
    )
    .finally(() => {
      reading = null;
    });
  return reading;
}

/** What's kept, for a menu that's built at once; a stale list is read again meanwhile. */
export function startAccountsNow(): StartAccount[] {
  void refresh();
  return read?.accounts ?? [];
}

/** This Mac's accounts, as they're read; read only while `wanted`. */
export function useStartAccounts(wanted = true): StartAccount[] {
  const [accounts, setAccounts] = useState<StartAccount[]>(() => read?.accounts ?? []);
  useEffect(() => {
    if (!wanted) return;
    let live = true;
    void refresh().then((r) => {
      if (live) setAccounts(r.accounts);
    });
    return () => {
      live = false;
    };
  }, [wanted]);
  return accounts;
}

/** This Mac's routing groups, read alongside its accounts. */
export function useStartGroups(wanted = true): RoutingGroup[] {
  const [groups, setGroups] = useState<RoutingGroup[]>(() => read?.groups ?? []);
  useEffect(() => {
    if (!wanted) return;
    let live = true;
    void refresh().then((r) => {
      if (live) setGroups(r.groups);
    });
    return () => {
      live = false;
    };
  }, [wanted]);
  return groups;
}

/** Other devices' accounts, by their short ids, and the reads under way. */
const devices = new Map<string, { at: number; accounts: StartAccount[] }>();
const devicesReading = new Map<string, Promise<StartAccount[]>>();

/** Asks a device for its accounts again once what's kept is a minute old. */
function refreshDevice(device: string): Promise<StartAccount[]> {
  const kept = devices.get(device);
  if (kept && Date.now() - kept.at < FRESH_MS) return Promise.resolve(kept.accounts);
  let reading = devicesReading.get(device);
  if (!reading) {
    reading = deviceStartOptions(device)
      .then(
        (o) => {
          const accounts = withLists(o);
          devices.set(device, { at: Date.now(), accounts });
          return accounts;
        },
        // A device that can't be asked now (offline, say) offers what it did last, or nothing,
        // and the box says its sessions' models without a menu until it answers.
        () => devices.get(device)?.accounts ?? [],
      )
      .finally(() => devicesReading.delete(device));
    devicesReading.set(device, reading);
  }
  return reading;
}

/** The accounts of the device whose short id is `device` (this Mac's for null), as they're
 *  read; read only while `wanted`. */
export function useDeviceStartAccounts(device: string | null, wanted = true): StartAccount[] {
  const here = useStartAccounts(wanted && !device);
  const [there, setThere] = useState<{ device: string; accounts: StartAccount[] } | null>(null);
  useEffect(() => {
    if (!wanted || !device) return;
    let live = true;
    void refreshDevice(device).then((accounts) => {
      if (live) setThere({ device, accounts });
    });
    return () => {
      live = false;
    };
  }, [wanted, device]);
  if (!device) return here;
  return there?.device === device ? there.accounts : (devices.get(device)?.accounts ?? []);
}

/** A session's account among them: its name and harness. */
export function accountOf(accounts: StartAccount[], s: Pick<Session, 'account' | 'harness'>): StartAccount | undefined {
  return accounts.find((a) => a.name === s.account && a.harness === s.harness);
}
