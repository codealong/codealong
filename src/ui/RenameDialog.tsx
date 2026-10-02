// Renames a session from the main window: Rename… in its right-click menu, or `r` on the list.
// The name goes where the session's harness keeps names (harness.rs `rename`), so the harness
// shows it too. Every window shows it at once (ahead.ts); one the harness didn't take goes back,
// and the window's toast says why.

import { useEffect, useRef, useState } from 'react';
import { renameAhead } from '../ahead';
import { NAME_KEPT, sessionName } from '../harness';
import type { Lane, Session } from '../types';
import { Button, Dialog } from './primitives';
import { errorText } from '../report';

interface RenameDialogProps {
  lane: Lane;
  session: Session;
  onClose: () => void;
  /** Why the harness didn't take the name. */
  onFailed: (why: string) => void;
}

export function RenameDialog({ lane, session: s, onClose, onFailed }: RenameDialogProps) {
  // The title it opened on: one the board changes meanwhile isn't a change of yours.
  const [opened] = useState(s.title);
  const [name, setName] = useState(s.title);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.select(), []);

  const next = sessionName(name);
  const save = () => {
    // Left as it was, a title the board cut short isn't taken for a new name.
    if (next && name !== opened) renameAhead(s.key, next).catch((e: unknown) => onFailed(errorText(e)));
    onClose();
  };

  return (
    <Dialog
      className="ca-renamedlg"
      title="Rename session"
      lede={
        <>
          {s.account} in {lane.callsign}
          {lane.device ? ` on ${lane.device.name}` : ''}. {NAME_KEPT[s.harness]}
        </>
      }
      onClose={onClose}
      onSubmit={save}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" disabled={!next}>
            Rename
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        <label className="ca-field">
          <span>Name</span>
          <input ref={input} value={name} autoFocus spellCheck={false} onChange={(e) => setName(e.target.value)} />
        </label>
      </div>
    </Dialog>
  );
}
