// Where the agents bar shows, as the main window's title bar says it and changes it (App.tsx,
// ui/MainBar.tsx): floating, docked in the notch, in the menu bar, or off; changed here, from the
// bar's own menu or from the Window menu. Picking the place it's in turns it off.

import { useEffect, useState } from 'react';
import { barMode, onBarMode, setBarMode, type BarMode } from './api';

export function useBarPlace(setToast: (message: string) => void) {
  const [barPlace, setBarPlace] = useState<BarMode>('floating');
  useEffect(() => {
    void barMode().then((v) => setBarPlace(v.mode));
    return onBarMode((v) => setBarPlace(v.mode));
  }, []);
  const pickBarPlace = (picked: Exclude<BarMode, 'off'>) => {
    const before = barPlace;
    const next = before === picked ? 'off' : picked;
    setBarPlace(next);
    setBarMode(next).catch((e: unknown) => {
      setBarPlace(before);
      setToast(String(e));
    });
  };
  return { barPlace, pickBarPlace };
}
