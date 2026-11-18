// Mark as done on a session whose helpers' work isn't over (menus.ts `doneHelpers`): marked done,
// a session's helpers end with it, as the work they did was for it, but one still at work or
// waiting on you would be cut off, its turn stopped or its question left unanswered. So the window
// asks first, listing those, whether to make them sessions of your own, which go on in your lists
// with their turns waiting on you (`Make it my own`), or to end them with it. Helpers whose work
// is over end with it unasked. Several sessions picked together are asked about once. The main
// window holds the dialog (App.tsx), and so does a session's card in the other windows
// (SessionCard.tsx); a window without it makes them yours unasked, as the engine does when it
// isn't told (actions.rs `finish_helpers`).

import { useState, type ReactNode } from 'react';
import { count } from '../format';
import { helperStands } from '../lanes';
import type { DoneHelpersAsk } from '../menus';
import type { Session } from '../types';
import { Button, Dialog, StatusGlyph } from './primitives';

/** What the dialog says: whose helpers they are, how many, and what each choice does. */
export function doneHelpersWords(ask: Pick<DoneHelpersAsk, 'sessions' | 'helpers'>): { title: string; lede: string } {
  const { sessions, helpers } = ask;
  const whose = sessions.length === 1 ? `“${sessions[0].title}”` : `The ${sessions.length} sessions you picked`;
  const have = sessions.length === 1 ? 'has' : 'have';
  const them = helpers.length === 1 ? 'it' : 'them';
  return {
    title: sessions.length === 1 ? `Mark “${sessions[0].title}” as done?` : `Mark ${count(sessions.length, 'session')} as done?`,
    lede: `${whose} ${have} ${helpers.length === 1 ? 'a helper' : count(helpers.length, 'helper')} whose work isn’t over. Make ${them} your own to go on with ${them} in your lists, or end ${them} too: a turn under way stops, and a question waiting on you goes unanswered.`,
  };
}

/** Where one of them stands, as its session's helpers chip says it, or what it left running. */
export function standsWords(h: Pick<Session, 'status' | 'statusSince' | 'lastActivity' | 'background' | 'agents'>, now: number): string {
  const since = h.statusSince ?? h.lastActivity;
  if (h.status === 'working' || h.status === 'needs' || h.status === 'error') return helperStands({ status: h.status, since }, now);
  return h.background != null ? 'left a command running in the background' : 'left agents running in the background';
}

/** A window's dialog: `ask` opens it, for Mark as done in the session's menu and keys (menus.ts
 *  `SessionCan.doneHelpers`). Each ask is a dialog of its own. */
export function useDoneHelpers(): { ask: (ask: DoneHelpersAsk) => void; dialog: ReactNode } {
  const [asked, setAsked] = useState<{ ask: DoneHelpersAsk; n: number } | null>(null);
  const ask = (a: DoneHelpersAsk) => setAsked((was) => ({ ask: a, n: (was?.n ?? 0) + 1 }));
  return { ask, dialog: asked && <DoneHelpersDialog key={asked.n} ask={asked.ask} onClose={() => setAsked(null)} /> };
}

function DoneHelpersDialog({ ask, onClose }: { ask: DoneHelpersAsk; onClose: () => void }) {
  const [now] = useState(() => Date.now());
  const { title, lede } = doneHelpersWords(ask);
  const one = ask.helpers.length === 1;
  const answer = (end: boolean) => {
    onClose();
    ask.then(end);
  };
  return (
    <Dialog
      className="ca-donehelpers"
      title={title}
      lede={lede}
      onClose={onClose}
      onSubmit={() => answer(false)}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="secondary" size="sm" onClick={() => answer(true)}>
            {one ? 'End it' : 'End them'}
          </Button>
          <Button variant="primary" size="sm" type="submit" autoFocus>
            {one ? 'Make it my own' : 'Make them my own'}
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        <ul className="ca-donehelpers-list">
          {ask.helpers.map((h) => (
            <li key={h.key} className="ca-donehelpers-row">
              <StatusGlyph status={h.status} title="" waiting={h.status === 'working' && (h.background != null || (h.agents?.length ?? 0) > 0)} />
              <span className="ca-donehelpers-text">
                <span className="ca-donehelpers-title">{h.title}</span>
                <span className="ca-donehelpers-stands">
                  {standsWords(h, now)} · {h.account}
                </span>
              </span>
            </li>
          ))}
        </ul>
      </div>
    </Dialog>
  );
}
