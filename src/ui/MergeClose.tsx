// Merge and close…: the primary session of a PR that's ready to merge (menus.ts `mergesWith`)
// merges it and is done with, from the session's menu or the button beside the PR's chip in its
// header. It asks first, as the PR's merge box does before a merge (MergeBox.tsx), with what the
// merge goes in with: the method kept for the repo, the commit as GitHub would write it, the
// email it goes under and whether the branch goes with it. It reads the box as it opens and says
// why when GitHub won't merge the PR now; the box itself has the rest (auto-merge, the queue,
// merging past the rules). Once GitHub has merged it, the session is marked done (menus.ts
// `sessionActions`), any work under way ended too, as the dialog says; a merge GitHub turns down
// leaves the session as it was and the dialog up, saying why. The main window holds the dialog
// (App.tsx), and so does a session's card in the other windows (SessionCard.tsx).

import { useEffect, useState } from 'react';
import { mergeAhead } from '../ahead';
import { mergeBox } from '../api';
import { below, showContextMenu } from '../contextMenu';
import { middleTruncate } from '../format';
import { doneWords, METHOD, offers, opFor, pickMethod, standing } from '../merge';
import { whatDoneDoes, type MergeCloseAsk } from '../menus';
import { errorText } from '../report';
import type { MergeBox as Box, MergeMethod, Session } from '../types';
import { CommitFields, methodItems, TONE_ICON, useCommit, useMethod } from './MergeBox';
import { Button, Dialog, Icon } from './primitives';

/** What the dialog says it does: merges the PR into its base, then marks the session done, and
 *  what that ends of it, the work under way first. */
export function mergeCloseWords(ask: Pick<MergeCloseAsk, 'pr'> & { session: Session }): string {
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

interface MergeCloseDialogProps {
  ask: MergeCloseAsk;
  onClose: () => void;
}

export function MergeCloseDialog({ ask, onClose }: MergeCloseDialogProps) {
  const { pr } = ask;
  const [read, setRead] = useState<{ box?: Box; error?: string }>({});
  useEffect(() => {
    let live = true;
    mergeBox(pr.repoSlug, pr.number)
      .then((box) => live && setRead({ box }))
      .catch((e: unknown) => live && setRead({ error: errorText(e) }));
    return () => {
      live = false;
    };
  }, [pr.repoSlug, pr.number]);

  // Read, it's a dialog of its own, which takes the keyboard on its confirm button.
  if (read.box) return <Ready ask={ask} box={read.box} onClose={onClose} />;
  return (
    <Dialog
      className="ca-mergeclosedlg"
      title="Merge and close"
      lede={mergeCloseWords(ask)}
      onClose={onClose}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" icon="merge" disabled>
            Merge and close
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">{read.error ? <p className="ca-form-error">Couldn’t read how it merges: {read.error}</p> : <p className="ca-meta">Reading how it merges…</p>}</div>
    </Dialog>
  );
}

function Ready({ ask, box, onClose }: { ask: MergeCloseAsk; box: Box; onClose: () => void }) {
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
    setBusy(true);
    setError(null);
    mergeAhead(pr, box.id, op).then(
      (warning) => {
        onClose();
        ask.then(warning ?? doneWords(op, pr.number, box.branch), warning != null);
      },
      (e: unknown) => {
        setBusy(false);
        setError(errorText(e));
      },
    );
  };

  return (
    <Dialog
      className="ca-mergeclosedlg"
      title="Merge and close"
      lede={mergeCloseWords(ask)}
      busy={busy}
      onClose={onClose}
      onSubmit={confirm}
      aside={error ? <p className="ca-form-error ca-mergeclosedlg-error">{error}</p> : undefined}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" icon="merge" disabled={!merges || busy} autoFocus title={method ? METHOD[method].what : undefined}>
            {busy ? 'Merging…' : 'Merge and close'}
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        {merges && method ? (
          <>
            <div className="ca-mergeclosedlg-method">
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
              <span>{said.more ? `${said.more} ` : ''}Its merge box on the PR has what GitHub offers now.</span>
            </div>
          </div>
        )}
      </div>
    </Dialog>
  );
}
