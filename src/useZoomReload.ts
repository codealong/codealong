// Zooming and reloading the main window (App.tsx), from its keys (mainKeys.ts) and from View's
// menu. ⌘= ⌘- ⌘0 and View's zoom items zoom every board window: the engine saves the zoom and sets
// it on them, and the board says what it is (zoom.ts). ⌘R and View › Reload read again what the
// window shows, usage too (reload.ts).

import { useCallback, useEffect } from 'react';
import { onReloadMenu, onZoomMenu, zoomWindows } from './api';
import { reloadWindow } from './reload';
import { errorText } from './report';
import type { Board } from './types';
import { setPageZoom, type ZoomStep } from './zoom';

export function useZoomReload(board: Board | null, setToast: (message: string) => void) {
  const zoom = useCallback(
    (step: ZoomStep) => {
      zoomWindows(step)
        .then((percent) => setToast(`Zoomed to ${percent}%.`))
        .catch((e: unknown) => setToast(errorText(e)));
    },
    [setToast],
  );
  useEffect(() => onZoomMenu(zoom), [zoom]);
  const reload = useCallback(() => {
    reloadWindow({ usage: true });
    setToast('Reloading…');
  }, [setToast]);
  useEffect(() => onReloadMenu(reload), [reload]);
  useEffect(() => setPageZoom(board?.zoom), [board?.zoom]);
  return { zoom, reload };
}
