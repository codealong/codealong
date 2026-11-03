// A session's terminal popped out of the drawer under its box into a window of its own (kept.rs
// `pop_terminal`), as a side conversation pops out: a strip that drags the window, says which
// terminal and session it is, and puts it back or closes it, over the terminal itself. The
// terminal goes on in the server either way; the window is only another place to show it, and
// closes when the terminal does.

import { useEffect, useRef, useState } from 'react';
import { keptClose, keptFocus, keptReady, keptSession, keptTerminal, terminalClose } from '../../api';
import { followBoard } from '../../followBoard';
import { terminalName } from '../../terminals';
import type { Board, TerminalGlance } from '../../types';
import { Icon } from '../primitives';
import { TerminalView } from './TerminalView';

export function TerminalWindow() {
  const [ids, setIds] = useState<{ key: string; id: string } | null>(null);
  const [board, setBoard] = useState<Board | null>(null);
  const [focusAt] = useState(() => Date.now());
  const readied = useRef(false);

  useEffect(() => {
    void Promise.all([keptSession(), keptTerminal()]).then(([key, id]) => {
      if (key && id) setIds({ key, id });
      else void keptClose();
    });
    return followBoard((b) => setBoard(b)).off;
  }, []);

  const glance: TerminalGlance | undefined = ids && board ? board.terminals?.[ids.key]?.find((t) => t.id === ids.id) : undefined;
  const session = ids && board ? board.lanes.flatMap((l) => l.sessions).find((s) => s.key === ids.key) : undefined;

  // Shown once there's a terminal to show; gone once the board no longer lists it.
  useEffect(() => {
    if (!ids || !board) return;
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
  return (
    <div className="ca-termwin" onMouseDown={() => void keptFocus()}>
      <div className="ca-termwin-bar" data-tauri-drag-region="deep">
        <Icon name="terminal" size={14} />
        <span className="ca-termwin-name">{terminalName(glance)}</span>
        {session && <span className="ca-termwin-session">{session.title}</span>}
        <span className="ca-termdrawer-gap" />
        <button type="button" className="ca-termdrawer-act" aria-label="Put it back under the session" title="Put it back under the session" onClick={() => void keptClose()}>
          <Icon name="picture-in-picture-off" size={14} />
        </button>
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
      <TerminalView id={glance.id} focusAt={focusAt} onLeave={() => void keptClose()} />
    </div>
  );
}
