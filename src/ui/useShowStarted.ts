// The quick panel's switch for whether a session it starts in the background or in Codealong is
// shown in the main window as it starts (newSession.ts `showsStart`): `[sessions] show_started`
// in config.toml, which the board says. A flip shows at once and is written to config.toml; the
// board catching up takes over from it, and a write that fails puts it back and says why.

import { useEffect, useState } from 'react';
import { keepShowStarted } from '../api';
import { errorText } from '../report';

/** The switch as it stands, `saved` being what the board says, and what flips it. `onError` says
 *  why a flip couldn't be kept. */
export function useShowStarted(saved: boolean | undefined, onError: (message: string) => void): [boolean, (on: boolean) => void] {
  const [flipped, setFlipped] = useState<boolean | null>(null);
  // What config.toml says now, once the board brings it, whoever changed it.
  useEffect(() => setFlipped(null), [saved]);
  const flip = (on: boolean) => {
    setFlipped(on);
    keepShowStarted(on).catch((e: unknown) => {
      setFlipped(null);
      onError(`Couldn’t keep that in config.toml: ${errorText(e)}`);
    });
  };
  return [flipped ?? saved ?? false, flip];
}
