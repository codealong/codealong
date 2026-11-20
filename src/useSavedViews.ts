// The saved views as a window has them (savedViews.ts): read from the page's storage as it opens,
// kept there as they change here, and followed as another window changes them.

import { useEffect, useState } from 'react';
import { loadSaved, onSavedChange, storeSaved, type SavedView } from './savedViews';

/** The saved views, and the way to keep them as a change leaves them. */
export function useSavedViews(): [SavedView[], (next: SavedView[]) => void] {
  const [savedViews, setSavedViews] = useState<SavedView[]>(loadSaved);
  useEffect(() => onSavedChange(setSavedViews), []);
  const keepSaved = (next: SavedView[]) => {
    setSavedViews(next);
    storeSaved(next);
  };
  return [savedViews, keepSaved];
}
