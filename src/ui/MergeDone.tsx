// Merge and done…: the primary session of a PR that's ready to merge (menus.ts `mergesWith`)
// merges it and is done with, from the session's menu or the button beside the PR's chip in its
// header. It asks first, as the PR's merge box does before a merge (MergeBox.tsx), with what the
// merge goes in with: the method kept for the repo, the commit as GitHub would write it, the
// email it goes under and whether the branch goes with it. It reads the box as it opens and says
// why when GitHub won't merge the PR now; the box itself has the rest (auto-merge, the queue,
// merging past the rules); while GitHub is still working out whether it can merge, it reads the
// box again, as the box does. Once GitHub has merged it, the engine marks the session done in the
// same request (ahead.ts `mergeDoneAhead`), any work under way ended too, as the dialog says, so
// a window closed meanwhile doesn't leave it half done; menus.ts says how it went. A merge GitHub
// turns down leaves the session as it was and the dialog up, saying why. The main window holds
// the dialog (App.tsx), and so does a session's card in the other windows (SessionCard.tsx).

import { useEffect, useState } from 'react';
import { mergeDoneAhead } from '../ahead';
import { mergeBox } from '../api';
import { below, showContextMenu } from '../contextMenu';
import { middleTruncate } from '../format';
import { doneWords, METHOD, offers, opFor, pickMethod, rereads, standing } from '../merge';
import { whatDoneDoes, type MergeDoneAsk } from '../menus';
import { errorText } from '../report';
import type { MergeBox as Box, MergeMethod, Session } from '../types';
import { CommitFields, methodItems, RECHECK_MS, RECHECKS, TONE_ICON, useCommit, useMethod } from './MergeBox';
import { Button, Dialog, Icon } from './primitives';

/** What the dialog says it does: merges the PR into its base, then marks the session done, and
 *  what that ends of it, the work under way first. */
export function mergeDoneWords(ask: Pick<MergeDoneAsk, 'pr'> & { session: Session }): string {
  const { pr, session: s } = ask;
  const into = pr.base ? ` into ${pr.base}` : '';
  const background = s.background?.trim();
  const first =
    s.status === 'working'
      ? ' Its turn is stopped first.'
      : background != null
        ? ` ${background ? `\`${middleTruncate(background, 60)}\`, which it left running in the background,` : 'The command it left running in the background'} stops with it.`
        : '';
  return `Merges #${pr.number}${into}, then marks “${s.title}” done: ${whatDoneDoes(s)}.${first}`;
}

interface MergeDoneDialogProps {
  ask: MergeDoneAsk;
  onClose: () => void;
}

export function MergeDoneDialog({ ask, onClose }: MergeDoneDialogProps) {
  const { pr } = ask;
  const [read, setRead] = useState<{ box?: Box; error?: string }>({});
  const [again, setAgain] = useState(0);
  useEffect(() => {
    let live = true;
    mergeBox(pr.repoSlug, pr.number)
      .then((box) => live && setRead({ box }))
      // Read before, the box stays as it was.
      .catch((e: unknown) => live && setRead((was) => ({ box: was.box, error: errorText(e) })));
    return () => {
      live = false;
    };
  }, [pr.repoSlug, pr.number, again]);
  // GitHub works out whether a PR can merge when it's first asked, and can take a moment to agree
  // with the board that it's ready: read again a few times, as the merge box does, each a while
  // after the last read came back, so a slow one isn't dropped for the next.
  const checking = !!read.box && rereads(read.box, pr) && again < RECHECKS;
  useEffect(() => {
    if (!checking) return;
    const t = window.setTimeout(() => setAgain((n) => n + 1), RECHECK_MS);
    return () => window.clearTimeout(t);
  }, [checking, read]);

  // Read, it's a dialog of its own, which takes the keyboard on its confirm button once it merges.
  if (read.box) return <Ready key={offers(read.box).merge ? 'merges' : 'waits'} ask={ask} box={read.box} checking={checking} onClose={onClose} />;
  return (
    <Dialog
      className="ca-mergedonedlg"
      title="Merge and done"
      lede={mergeDoneWords(ask)}
      onClose={onClose}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" icon="merge" disabled>
            Merge and done
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">{read.error ? <p className="ca-form-error">Couldn’t read how it merges: {read.error}</p> : <p className="ca-meta">Reading how it merges…</p>}</div>
    </Dialog>
  );
}

/** `checking`: GitHub is still working out whether it can merge, and the box is read again. */
function Ready({ ask, box, checking, onClose }: { ask: MergeDoneAsk; box: Box; checking: boolean; onClose: () => void }) {
  const { pr } = ask;
  const [picked, keepMethod] = useMethod(pr.repoSlug);
  const method = pickMethod(box, picked);
  // The commit as GitHub would write it, for the method it opens on and each picked after.
  const commit = useCommit(box, method);
  const pick = (m: MergeMethod) => {
    keepMethod(m);
    commit.fill(m);
  };
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Only a merge now, as GitHub would do it from its box without waiting: the box has the rest.
  const merges = offers(box).merge && method != null;
  const said = standing(box, pr);

  const confirm = () => {
    if (!merges || busy) return;
    if (method !== 'rebase' && !commit.title.trim()) {
      setError('The commit needs a title.');
      return;
    }
    commit.keep();
    const op = opFor(box, 'merge', { method, title: commit.title, body: commit.body, email: commit.email, deleteBranch: commit.deleteBranch });
    if (op.op !== 'merge') return;
    setBusy(true);
    setError(null);
    mergeDoneAhead(pr, box.id, op, ask.session.key).then(
      (d) => {
        onClose();
        ask.then(d.note ?? doneWords(op, pr.number, box.branch), d.note != null, d);
      },
      (e: unknown) => {
        setBusy(false);
        setError(errorText(e));
      },
    );
  };

  return (
    <Dialog
      className="ca-mergedonedlg"
      title="Merge and done"
      lede={mergeDoneWords(ask)}
      busy={busy}
      onClose={onClose}
      onSubmit={confirm}
      aside={error ? <p className="ca-form-error ca-mergedonedlg-error">{error}</p> : undefined}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" icon="merge" disabled={!merges || busy} autoFocus title={method ? METHOD[method].what : undefined}>
            {busy ? 'Merging and wrapping up…' : 'Merge and done'}
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        {merges && method ? (
          <>
            <div className="ca-mergedonedlg-method">
              <span>Merge by</span>
              <button
                type="button"
                className="ca-modelmenu-chip"
                aria-haspopup="menu"
                title="How it merges: kept for this repo"
                disabled={busy}
                onClick={(e) => void showContextMenu(methodItems(box, method, pick), below(e.currentTarget)).catch((err: unknown) => setError(errorText(err)))}
              >
                <span>{METHOD[method].label}</span>
                <Icon name="chevron-down" size={12} />
              </button>
            </div>
            <CommitFields box={box} mode="merge" method={method} commit={commit} busy={busy} onError={setError} />
          </>
        ) : (
          <div className={`ca-merge-standing is-${box.state === 'MERGED' ? 'merged' : said.tone}`}>
            <Icon name={box.state === 'CLOSED' ? 'pr-closed' : TONE_ICON[said.tone]} size={16} stroke={2} />
            <div>
              <strong>{said.says}</strong>
              <span>{[said.more, !checking && 'Its merge box on the PR has what GitHub offers now.'].filter(Boolean).join(' ')}</span>
            </div>
          </div>
        )}
      </div>
    </Dialog>
  );
}
