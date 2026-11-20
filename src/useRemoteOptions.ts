// Where accounts can start on your other machines, for the quick panel: asked of the one a new
// session would start on once it's chosen, kept by machine, and asked again when what's kept is
// old. This machine's own is read where the panel opens (ui/Quick.tsx), from a copy as old at most.

import { useEffect, useState } from 'react';
import { deviceStartOptions } from './api';
import { errorText } from './report';
import type { StartOptions } from './types';

/** How long a read of where accounts can start is used without waiting for a new one. */
export const OPTIONS_FOR_MS = 60_000;
/** How long after another Mac didn't say where its accounts can start it's asked again. */
export const RETRY_MS = 5_000;

/** What a machine last said of where its accounts can start: when, and why not if it didn't,
 *  with what it said before that. */
export interface RemoteOptions {
  at: number;
  options: StartOptions | null;
  error: string | null;
}

/** Where the accounts of the machine `deviceNow` (its short id; null for this one) can start,
 *  asked of it while the panel is `shown` and again once the answer is a minute old (five
 *  seconds for one that failed): `remote` is undefined until it has answered. `askAgain` forgets
 *  a machine's answer, so it's asked afresh the next time it's the one chosen. */
export function useRemoteOptions(shown: boolean, deviceNow: string | null): { remote: RemoteOptions | undefined; askAgain: (device: string) => void } {
  // Where accounts can start on your other Macs, asked of each once it's chosen.
  const [remoteOptions, setRemoteOptions] = useState<Record<string, RemoteOptions>>({});
  const remote = deviceNow ? remoteOptions[deviceNow] : undefined;
  // Where its accounts can start is asked of it, and asked again once it's a minute old.
  useEffect(() => {
    if (!shown || !deviceNow || (remote && Date.now() - remote.at < (remote.error ? RETRY_MS : OPTIONS_FOR_MS))) return;
    const d = deviceNow;
    deviceStartOptions(d)
      .then((o) => setRemoteOptions((r) => ({ ...r, [d]: { at: Date.now(), options: o, error: null } })))
      .catch((e: unknown) => setRemoteOptions((r) => ({ ...r, [d]: { at: Date.now(), options: r[d]?.options ?? null, error: errorText(e) } })));
  }, [shown, deviceNow, remote]);
  const askAgain = (d: string) => setRemoteOptions(({ [d]: _, ...rest }) => rest);
  return { remote, askAgain };
}
