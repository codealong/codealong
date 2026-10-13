// Clean up worktree…, from a worktree's right-click menu or several picked on the Worktrees board:
// asks before removing, since it can't be undone. It lists each worktree with what would run,
// as the engine says on its device (cleanup.rs, asked with a dry run): the repo's own
// WorktreeRemove hook when it has one, else git, and a background job's record after. Only ones
// with nothing in them to lose come here (board.rs `cleanup`), and the engine checks each again
// as it goes; one that has changed since stays, and says why.

import { useEffect, useState } from 'react';
import { removeWorktree, type Removal } from '../api';
import { count } from '../format';
import { errorText } from '../report';
import type { Lane } from '../types';
import { Button, Dialog, Icon } from './primitives';

interface Props {
  lanes: Lane[];
  onClose: () => void;
  /** Called with what was done, for the window's toast. */
  onDone: (message: string) => void;
}

/** What removing them did, for the window's toast: the one's own sentence, or how many went. */
export function cleanedSummary(lanes: Lane[], out: (Removal | null)[]): string {
  const gone = out.filter((r) => r?.removed).length;
  if (lanes.length === 1) return out[0]?.message ?? '';
  return gone > 0 ? `Removed ${count(gone, 'worktree')}.` : '';
}

export function CleanUpDialog({ lanes, onClose, onDone }: Props) {
  // What each would run, as its device says, by lane id.
  const [plans, setPlans] = useState<Record<string, Removal | string>>({});
  const [removing, setRemoving] = useState(false);
  const [failed, setFailed] = useState<{ lane: Lane; why: string }[] | null>(null);

  useEffect(() => {
    let live = true;
    for (const l of lanes) {
      removeWorktree(l.id, true).then(
        (r) => live && setPlans((p) => ({ ...p, [l.id]: r })),
        (e: unknown) => live && setPlans((p) => ({ ...p, [l.id]: errorText(e) })),
      );
    }
    return () => {
      live = false;
    };
  }, [lanes]);

  // Only what its device says can go goes: one that changed since the menu was built stays, and
  // says why.
  const ready = lanes.filter((l) => {
    const plan = plans[l.id];
    return typeof plan === 'object' && plan.removed;
  });
  const checking = lanes.some((l) => plans[l.id] === undefined);

  const run = async () => {
    setRemoving(true);
    // One after another: two of a repo's teardowns at once could trip over each other.
    const out: (Removal | null)[] = [];
    const notDone: { lane: Lane; why: string }[] = [];
    for (const l of ready) {
      try {
        const r = await removeWorktree(l.id, false);
        out.push(r);
        if (!r.removed) notDone.push({ lane: l, why: r.message });
      } catch (e) {
        out.push(null);
        notDone.push({ lane: l, why: errorText(e) });
      }
    }
    setRemoving(false);
    const said = cleanedSummary(ready, out);
    if (notDone.length === 0) {
      onDone(said);
      onClose();
      return;
    }
    setFailed(notDone);
    if (said && ready.length > 1) onDone(said);
  };

  const one = lanes.length === 1;
  const body = failed ? (
    <>
      <p className="ca-cleanup-note">Not removed:</p>
      <ul className="ca-cleanup-list">
        {failed.map(({ lane, why }) => (
          <li key={lane.id} className="ca-cleanup-row is-failed">
            <Icon name="alert" size={14} />
            <span className="ca-cleanup-text">
              <span className="ca-cleanup-name">{lane.callsign}</span>
              <span className="ca-cleanup-why">{why}</span>
            </span>
          </li>
        ))}
      </ul>
    </>
  ) : (
    <ul className="ca-cleanup-list" aria-label="Worktrees to remove">
      {lanes.map((l) => {
        const plan = plans[l.id];
        return (
          <li key={l.id} className="ca-cleanup-row" title={l.path}>
            <Icon name="folder" size={14} />
            <span className="ca-cleanup-text">
              <span className="ca-cleanup-name">
                {l.callsign}
                {l.branch && <span className="ca-cleanup-branch"> · {l.branch}</span>}
              </span>
              {plan === undefined ? (
                <span className="ca-cleanup-why">Checking…</span>
              ) : typeof plan === 'string' ? (
                <span className="ca-cleanup-why is-alert">{plan}</span>
              ) : (
                <>
                  <span className={plan.removed ? 'ca-cleanup-why' : 'ca-cleanup-why is-alert'}>{plan.message}</span>
                  {plan.steps.map((step) => (
                    <code key={step} className="ca-cleanup-step">
                      {step}
                    </code>
                  ))}
                </>
              )}
            </span>
          </li>
        );
      })}
    </ul>
  );

  return (
    <Dialog
      className="ca-cleanup"
      title={one ? 'Clean up worktree' : `Clean up ${count(lanes.length, 'worktree')}`}
      lede={`Removes ${one ? 'its folder' : 'their folders'}, through the repo’s own WorktreeRemove hook where it has one, and ${one ? 'its branch' : 'each one’s branch'} where nothing else uses it and its work is merged. Nothing uncommitted and no commit that isn’t on a remote is lost; files git ignores, such as dependencies and env files, go too. This can’t be undone.`}
      busy={removing}
      onClose={onClose}
      foot={
        failed ? (
          <Button variant="primary" size="sm" onClick={onClose}>
            Done
          </Button>
        ) : (
          <>
            <Button variant="ghost" size="sm" onClick={onClose} disabled={removing}>
              Cancel
            </Button>
            <Button variant="primary" size="sm" onClick={() => void run()} disabled={removing || checking || ready.length === 0}>
              {removing ? 'Removing…' : one ? 'Remove worktree' : `Remove ${count(ready.length, 'worktree')}`}
            </Button>
          </>
        )
      }
    >
      <div className="ca-dialog-body">{body}</div>
    </Dialog>
  );
}
