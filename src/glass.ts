// Glass, an experiment (`[experimental] glass`, Settings › Experimental): the app puts macOS's
// glass behind a board window's page (glass.rs), where it shows only through what the page leaves
// see-through. So while it's on the page's root says `is-glass`, and the window's chrome lets it
// through, as the Finder's does: the title bar (App.css), the usage bar (UsageBar.css) and the
// sidebar (Sidebar.css), over a page with no ground of its own (base.css). The board and the panel
// keep theirs, so what you read stays as legible. Only on macOS, the one platform the app puts
// glass behind a window.

import { useEffect } from 'react';
import { platform } from './platform';

/** Keeps a board window's page see-through while glass is on (the board's `experimental.glass`). */
export function useGlass(on: boolean) {
  useEffect(() => {
    document.documentElement.classList.toggle('is-glass', on && platform() === 'mac');
  }, [on]);
}
