// What a session's helpers chip and its side conversations do in the main window (App.tsx;
// ui/HelpersCard.tsx `HelperActions`), from what a render of it has: a helper shown in Sessions,
// under its session, which lists it first when it doesn't yet (a finished one), with its
// conversation in the panel; opened where it runs, listed so too, since one Codealong runs opens
// in its panel here; its menu; and a side conversation shown above its session's box.

import type { Dispatch, SetStateAction } from 'react';
import { sessionInFacets, sessionItemOf, sessionItemsOf, showingHelper, type Facets, type ListRow } from './boardLists';
import type { Placed } from './lanes';
import type { SelectHow } from './picks';
import { showSide } from './side';
import type { Lane, Session, SideGlance } from './types';
import type { HelperActions } from './ui/HelpersCard';
import type { SessionItem } from './ui/SessionsView';

/** What the window has for them, as one render has it. */
export interface HelpersHas {
  /** Where each session the views list is. */
  homes: ReadonlyMap<string, Placed>;
  /** Sessions' Archived fold, whose chips show their side conversations too. */
  archivedItems: readonly SessionItem[];
  picks: { sessions: { select: (row: SessionItem, how: SelectHow) => void } };
  /** Puts the cursor in the box of the session's panel. */
  focusBox: () => void;
  /** What the sidebar narrows Sessions to besides its statuses. */
  facets: Facets;
  setFacets: Dispatch<SetStateAction<Facets>>;
  /** The sessions whose finished helpers Sessions lists under them too. */
  setHelpersShownList: (next: (prev: string[]) => string[]) => void;
  /** What Sessions lists now, and the worktrees the views list. */
  sessionRows: readonly ListRow[];
  lanes: Lane[];
  openRow: (row: ListRow | null) => void;
  sessionMenu: (lane: Lane, s: Session, pin: 'session' | 'worktree') => void;
}

export function helperActsOf(has: HelpersHas): { showSideOf: (key: string, side: SideGlance) => void; helperActions: HelperActions } {
  const { picks } = has;
  const helperRow = (key: string): SessionItem | null => {
    const home = has.homes.get(key);
    return home ? sessionItemOf(home) : null;
  };
  // A side conversation's row under its session, or a row of its chip's card: the session
  // selected, and the side conversation above its box, which takes what's typed.
  const showSideOf = (key: string, side: SideGlance) => {
    // An archived session's chip shows its side conversations too.
    const r = helperRow(key) ?? has.archivedItems.find((x) => x.session.key === key);
    if (r) picks.sessions.select(r, 'one');
    showSide(key, side);
    has.focusBox();
  };
  /** A helper shown from its session's chip runs on an account Agents may leave out, or wears none
   *  of the labels picked: then it shows them all, as showing a row from elsewhere clears what
   *  would hide it. */
  const unhide = (r: SessionItem) => {
    if (!sessionInFacets(r, has.facets)) has.setFacets((f) => ({ ...f, agents: [], sessionLabels: [] }));
  };
  const helperActions: HelperActions = {
    show: (key) => {
      const r = helperRow(key);
      if (!r) return;
      unhide(r);
      has.setHelpersShownList((prev) => showingHelper(prev, has.sessionRows, r, sessionItemsOf(has.lanes)));
      picks.sessions.select(r, 'one');
      has.focusBox();
    },
    open: (key) => {
      const r = helperRow(key);
      if (!r) return;
      unhide(r);
      has.setHelpersShownList((prev) => showingHelper(prev, has.sessionRows, r, sessionItemsOf(has.lanes)));
      has.openRow(r);
    },
    menu: (key) => {
      const r = helperRow(key);
      if (r) has.sessionMenu(r.lane, r.session, 'session');
    },
  };
  return { showSideOf, helperActions };
}
