// How tool calls and intermediate work appear in a session's conversation (`[appearance] tools`,
// `Board.tools`, `BarFeed.tools`): `folded` by default (completed turns collapse into one
// expandable "Worked for …" line), `compact` (1-line plain-English tool summaries), `detailed`
// (individual tool lines, folding runs of three or more), or `zen` (prompts and final replies
// only). Each window's root syncs the setting from its board or feed so Settings changes apply
// immediately everywhere.

import { useEffect, useSyncExternalStore } from 'react';
import type { ToolDisplay } from './types';

let current: ToolDisplay = 'folded';
const heard = new Set<() => void>();

/** Sets the current display mode (for tests and window roots). */
export function setToolDisplay(next: ToolDisplay | undefined): void {
  const value: ToolDisplay = next ?? 'folded';
  if (current === value) return;
  current = value;
  heard.forEach((f) => f());
}

/** Keeps the window's tool display mode in step with Settings (`Board.tools` / `BarFeed.tools`). */
export function useToolDisplaySync(mode: ToolDisplay | undefined): void {
  useEffect(() => {
    setToolDisplay(mode);
  }, [mode]);
}

/** How tool calls appear in conversations, as Settings currently has it. */
export function useToolDisplay(): ToolDisplay {
  return useSyncExternalStore(
    (f) => {
      heard.add(f);
      return () => heard.delete(f);
    },
    () => current,
  );
}
