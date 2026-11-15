// Glass, an experiment (`[experimental] glass`, Settings › Experimental): the app puts macOS's
// glass behind a board window's page (glass.rs), where it shows only through what the page leaves
// see-through. So while it's on the page's root says `is-glass`, and what was gray lets it through,
// as the Finder's chrome does: the title bar and the board's ground (App.css), the usage bar
// (UsageBar.css), the sidebar (Sidebar.css), the strip over the list (ListBar.css) and Kanban's
// columns (KanbanView.css), over a page with no ground of its own (base.css). The rows, the cards
// and the panel keep theirs, so what you read stays as legible; the strip over the panel takes the
// panel's (SessionPanel.css). Only on macOS, the one platform the app puts glass behind a window.

import { useEffect } from 'react';
import { platform } from './platform';

/** Keeps a board window's page see-through while glass is on (the board's `experimental.glass`). */
export function useGlass(on: boolean) {
  useEffect(() => {
    document.documentElement.classList.toggle('is-glass', on && platform() === 'mac');
  }, [on]);
}
