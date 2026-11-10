// A session's terminal popped out of the drawer under its box into a window of its own (kept.rs
// `pop_terminal`), as a side conversation pops out: a strip that drags the window, says which
// terminal and session it is, and puts it back or closes it, over the terminal itself, which
// takes the room the window's given as its right and bottom edges size it. The
// terminal goes on in the server either way; the window is only another place to show it, and
// closes when the terminal does. A terminal that's no session's (a folder opened in Codealong's
// terminal, a background job attached to) has only this window, as a terminal app's would: there's
// nowhere to put it back, and closing it closes the terminal.

import { useEffect, useRef, useState } from 'react';
import { keptClose, keptFocus, keptReady, keptResize, keptSession, keptTerminal, terminalClose } from '../../api';
import { followBoard } from '../../followBoard';
import { terminalName } from '../../terminals';
import type { Board, TerminalGlance } from '../../types';
import { errorText } from '../../report';
import { Icon } from '../primitives';
import { useTheme } from '../ThemeSwitch';
import { useSizeGrips } from '../useSizeGrips';
import { TerminalView } from './TerminalView';

export function TerminalWindow() {
  const [ids, setIds] = useState<{ key: string; id: string } | null>(null);
  const [board, setBoard] = useState<Board | null>(null);
  const [focusAt] = useState(() => Date.now());
  const readied = useRef(false);
  const [failed, setFailed] = useState<string | null>(null);
  const grip = useSizeGrips('ca-termwin-grip', {
    onStart: () => void keptFocus(),
    size: (w, h) => void keptResize(w, h),
    done: (w, h) => void keptResize(w, h).catch((e: unknown) => setFailed(errorText(e))),
  });
  // Light or dark as Codealong is, not only as macOS is: the terminal's colors come from it.
  useTheme(board?.theme ?? 'system');

  useEffect(() => {
    void Promise.all([keptSession(), keptTerminal()]).then(([key, id]) => {
      if (id) setIds({ key: key ?? '', id });
      else void keptClose();
    });
    return followBoard((b) => setBoard(b)).off;
  }, []);

  // Found by its own id under whichever session has it now: a session moved to another account
  // hands its terminals to its copy (terminals.rs `follow`).
  const glance: TerminalGlance | undefined = ids && board ? Object.values(board.terminals ?? {}).flat().find((t) => t.id === ids.id) : undefined;
  const session = glance && board ? board.lanes.flatMap((l) => l.sessions).find((s) => s.key === glance.session) : undefined;

  // Shown once there's a terminal to show; gone once the board no longer lists it. A board still
  // loading lists none yet (the app's first, before the server's comes), which says nothing.
  useEffect(() => {
    if (!ids || !board || board.loading) return;
    if (!glance) {
      void keptClose();
      return;
    }
    if (!readied.current) {
      readied.current = true;
      void keptReady();
    }
  }, [ids, board, glance]);

  if (!ids || !glance) return null;
  const mine = !!glance.session;
  return (
    <>
      <div className="ca-termwin" onMouseDown={() => void keptFocus()}>
        <div className="ca-termwin-bar" data-tauri-drag-region="deep">
          <Icon name="terminal" size={14} />
          <span className="ca-termwin-name">{terminalName(glance)}</span>
          {session && <span className="ca-termwin-session">{session.title}</span>}
          <span className="ca-termdrawer-gap" />
          {mine && (
            <button type="button" className="ca-termdrawer-act" aria-label="Put it back under the session" title="Put it back under the session" onClick={() => void keptClose()}>
              <Icon name="picture-in-picture-off" size={14} />
            </button>
          )}
          <button
            type="button"
            className="ca-termdrawer-act"
            aria-label="Close the terminal"
            title={glance.running && glance.program ? `Close it, ending ${glance.program}` : 'Close it'}
            onClick={() => void terminalClose(glance.id).finally(() => void keptClose())}
          >
            <Icon name="x" size={14} />
          </button>
        </div>
        {/* ⌃` puts a session's back where it came from; one that's no session's stays. */}
        <TerminalView id={glance.id} focusAt={focusAt} onLeave={() => mine && void keptClose()} session={mine && !glance.hosts ? glance.session : undefined} />
        {failed && (
          <p className="ca-term-failed" role="status">
            {failed}
          </p>
        )}
      </div>
      {/* In the room the window keeps around the card, which clips what's in it. */}
      {grip('side')}
      {grip('far')}
      {grip('corner')}
    </>
  );
}
