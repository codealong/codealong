// This Mac's accounts as a start or a resume offers them (`start-options`: where each can start a
// session, what its command line takes, where one of its sessions that isn't running can go on),
// read once a window needs them and kept a minute: the cards, boxes and menus that offer them
// would each ask the engine again otherwise.

import { useEffect, useState } from 'react';
import { startOptions } from './api';
import { withLists } from './start';
import type { Session, StartAccount } from './types';

const FRESH_MS = 60_000;
let read: { at: number; accounts: StartAccount[] } | null = null;
let reading: Promise<StartAccount[]> | null = null;

/** Reads them again once what's kept is a minute old, and resolves with what's kept. */
function refresh(): Promise<StartAccount[]> {
  if (read && Date.now() - read.at < FRESH_MS) return Promise.resolve(read.accounts);
  reading ??= startOptions()
    .then(
      (o) => {
        read = { at: Date.now(), accounts: withLists(o) };
        return read.accounts;
      },
      // The last read stands, and what offers them goes without until a read works: the quick
      // panel and the main window say so when start options can't be read.
      () => read?.accounts ?? [],
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
    void refresh().then((a) => {
      if (live) setAccounts(a);
    });
    return () => {
      live = false;
    };
  }, [wanted]);
  return accounts;
}

/** A session's account among them: its name and harness. */
export function accountOf(accounts: StartAccount[], s: Pick<Session, 'account' | 'harness'>): StartAccount | undefined {
  return accounts.find((a) => a.name === s.account && a.harness === s.harness);
}
