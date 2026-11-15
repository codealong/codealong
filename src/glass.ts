// Glass (`[appearance] glass`, Settings › Appearance): the app puts macOS's
// glass behind a board window's page (glass.rs), where it shows only through what the page leaves
// see-through. So while it's on the page's root says `is-glass`, and what was gray lets it through,
// as the Finder's chrome does: the title bar (App.css), the usage bar (UsageBar.css), the sidebar
// (Sidebar.css), the strip over the list (ListBar.css) and Kanban's columns (KanbanView.css), over a
// page with no ground of its own (base.css), with the board's ground frosted (App.css). What sits on
// the glass draws its neutrals see-through, so they take its tint (styles/glass.css). The rows, the
// cards and the panel keep their grounds, so what you read stays as legible; the strip over the panel
// takes the panel's (SessionPanel.css). Only on macOS, the one platform the app puts glass behind a
// window.

import { useEffect } from 'react';
import { platform } from './platform';

/** Keeps a board window's page see-through while glass is on (the board's `glass`). */
export function useGlass(on: boolean) {
  useEffect(() => {
    document.documentElement.classList.toggle('is-glass', on && platform() === 'mac');
  }, [on]);
}
