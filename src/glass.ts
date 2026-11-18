// Glass (`[appearance] glass`, Settings › Appearance): the app puts macOS's
// glass behind a board window's page (glass.rs), where it shows only through what the page leaves
// see-through. So while it's on the page's root says `is-glass`, and what was gray lets it through,
// as the Finder's chrome does: the title bar, the sidebar and the usage bar, one pane of it on the
// window's ground (App.css), and Kanban's columns (KanbanView.css), over a page with no ground of its own
// (base.css), with the board's ground frosted (App.css) and the strip over the list, which rows
// scroll under, opaque (ListBar.css). The page draws no blur of its own: WebKit's,
// stacked on the glass, flickers as the window moves. What sits on
// the glass draws its neutrals see-through, so they take its tint (styles/glass.css). The rows, the
// cards and the panel keep their grounds, so what you read stays as legible; the strip over the panel
// takes the shade of the strip over the list, so the two read as one (SessionPanel.css). The quick panel has glass only under its card (quick.rs
// `fit_glass`), which its bar along the foot lets through (Quick.css). Only on macOS, the one
// platform the app puts glass behind a window.

import { useEffect } from 'react';
import { platform } from './platform';

/** Keeps a page see-through where it lets the glass through, a board window's or the quick
 *  panel's, while glass is on (the board's `glass`). */
export function useGlass(on: boolean) {
  useEffect(() => {
    document.documentElement.classList.toggle('is-glass', on && platform() === 'mac');
  }, [on]);
}
