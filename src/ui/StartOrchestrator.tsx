// Starting an orchestrator (orchestrators.ts) from Sessions: the bar's compass, there while the
// view's project has none running (under All projects, while All projects or any project of yours
// has none: `freeScopes`), asks here first, since one keeps going until it's turned off and spends
// usage whenever something it keeps watch over needs a look. Under All projects it asks which:
// All projects', or a project's. Its session then leads Sessions, and its menu looks now, starts
// it afresh or turns it off.

import { useState } from 'react';
import { scopeWords } from '../orchestrators';
import { Button, Dialog, Icon } from './primitives';

interface StartOrchestratorProps {
  /** What it can keep watch over, the first picked: a project, or null for All projects. With
   *  more than one, the dialog asks which. */
  scopes: (string | null)[];
  /** Projects with orchestrators of their own, which All projects' leaves to them. */
  others: string[];
  /** Starts the one picked; the dialog stays up, busy, until that's done, and closes once it is. */
  onStart: (project: string | null) => Promise<unknown>;
  onClose: () => void;
}

export function StartOrchestratorDialog({ scopes, others, onStart, onClose }: StartOrchestratorProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [project, setProject] = useState<string | null>(scopes[0] ?? null);
  const words = scopeWords(project);
  const start = () => {
    setBusy(true);
    setError(null);
    onStart(project).then(onClose, (e: unknown) => {
      setBusy(false);
      setError(e instanceof Error ? e.message : String(e));
    });
  };
  const over = project
    ? `${project}’s sessions`
    : others.length > 0
      ? `every project’s sessions but ${others.join(', ')}’s, which have orchestrators of their own, and the sessions in no project`
      : 'every project’s sessions, and the sessions in no project';
  return (
    <Dialog
      className="ca-startorch"
      title={`Start an orchestrator for ${words}?`}
      lede={`An agent keeps watch over ${over} for you: when one starts to wait on you, a PR moves, or two agents work in one worktree, it reads what happened, tags it and suggests replies for you to send.`}
      busy={busy}
      onClose={onClose}
      onSubmit={start}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" disabled={busy} autoFocus>
            {busy ? 'Starting…' : 'Start orchestrator'}
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        {scopes.length > 1 && (
          <label className="ca-field ca-startorch-scope">
            <span>Keeps watch over</span>
            <div className="ca-start-select">
              {/* All projects is the empty value: no project is named that way. */}
              <select aria-label="Keeps watch over" value={project ?? ''} disabled={busy} onChange={(e) => setProject(e.target.value || null)}>
                {scopes.map((s) => (
                  <option key={s ?? ''} value={s ?? ''}>
                    {scopeWords(s)}
                  </option>
                ))}
              </select>
              <Icon name="chevron-down" size={12} />
            </div>
          </label>
        )}
        <p className="ca-startorch-note">It runs out of sight on an account with room, only when something needs a look, and never messages a session for you. Its session leads Sessions, quiet unless it asks you something, and starts afresh each day from its notes. Its menu turns it off.</p>
        {error && <p className="ca-startorch-error">{error}</p>}
      </div>
    </Dialog>
  );
}
