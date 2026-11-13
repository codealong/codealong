// Merge and done…: the primary session of a PR that's ready to merge (menus.ts `mergesWith`)
// merges it and is done with, from the session's menu or the button beside the PR's chip in its
// header. It asks first, as the PR's merge box does before a merge (MergeBox.tsx), with what the
// merge goes in with: the method kept for the repo, the commit as GitHub would write it, the
// email it goes under and whether the branch goes with it. It reads the box as it opens and says
// why when GitHub won't merge the PR now; the box itself has the rest (auto-merge, the queue,
// merging past the rules); while GitHub is still working out whether it can merge, it reads the
// box again, as the box does.
//
// Confirmed, it goes at once, as though GitHub had merged it: the PR shows merged and the session
// done in every window, and the window goes on as from Mark as done (menus.ts), while the engine
// merges it and, once GitHub has, marks the session done in the same request (ahead.ts
// `mergeDoneAhead`), any work under way ended too, as the dialog says, so a window closed
// meanwhile doesn't leave it half done. A merge GitHub refuses puts the PR and the session back as
// they were, the session round again in what waits (menus.ts `SessionCan.failed`), and brings
// the dialog back with the commit as it was written and why: over whatever the main window
// shows, and in a card once it's on the session again, never in place of another dialog
// (`useMergeDone`). Its button doesn't take the keyboard then, so a Return meant for whatever
// you'd gone on to doesn't merge it. The main window holds the dialog (App.tsx), and so does a
// session's card in the other windows (SessionCard.tsx).

import { useEffect, useRef, useState, type ReactNode } from 'react';
import { mergeDoneAhead } from '../ahead';
import { useLatest } from '../latest';
import { mergeBox } from '../api';
import { middleTruncate } from '../format';
import { doneWords, METHOD, offers, opFor, pickMethod, rereads, standing, type MergeForm } from '../merge';
import { whatDoneDoes, type MergeDoneAsk } from '../menus';
import { useReloads } from '../reload';
import { errorText } from '../report';
import type { MergeBox as Box, MergeMethod, Session } from '../types';
import { CommitFields, MethodChip, RECHECK_MS, RECHECKS, TONE_ICON, useCommit, useMethod } from './MergeBox';
import { Button, cx, Dialog, Icon } from './primitives';

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

/** Merges GitHub refused whose dialog couldn't come back where it was asked, by session: the card
 *  had gone on to another session or away, or another dialog was up. Merge and done… on the
 *  session opens with the commit as it was written and why, and a card that comes back to the
 *  session opens it by itself. This window's alone. */
const refusedFor = new Map<string, MergeDoneAsk>();

/** Whether the dialog of `session`'s refused merge comes back now, where it was asked: the window
 *  still has it (`up`), with no other dialog `open`, and it's not a card on another session
 *  (`showing`; the main window shows none). Else it waits for the session (`refusedFor`). */
export function comesBack(session: string, here: { up: boolean; open: boolean; showing?: string }): boolean {
  return here.up && !here.open && (here.showing === undefined || here.showing === session);
}

/** What Merge and done… asked afresh on `a`'s session opens with: the commit and why a refused
 *  merge of the same PR left (taken from `kept`), as now. A session can be the primary of more
 *  than one PR, and one left for another stays for it. */
export function freshAsk(a: MergeDoneAsk, kept: Map<string, MergeDoneAsk>): MergeDoneAsk {
  const left = kept.get(a.session.key);
  if (!left || left.pr.key !== a.pr.key) return a;
  kept.delete(a.session.key);
  return { ...a, refused: left.refused };
}

/** A window's Merge and done… dialog: `ask` opens it, for the session's menu and keys (menus.ts
 *  `SessionCan.mergeDone`), and again once GitHub has refused a merge confirmed in it, where that
 *  doesn't take the place of another dialog, nor show over another session's card (`showing`, a
 *  card's session; the main window has none and shows it over anything); else it waits in
 *  `refusedFor`. `ask` answers whether it's shown and `inFront`: the window surely shows it to
 *  you. Each ask is a dialog of its own, read afresh. */
export function useMergeDone(inFront: boolean, showing?: string): { ask: (ask: MergeDoneAsk) => boolean; dialog: ReactNode } {
  const [asked, setAsked] = useState<{ ask: MergeDoneAsk; n: number } | null>(null);
  const n = useRef(0);
  const open = (a: MergeDoneAsk) => {
    n.current += 1;
    setAsked({ ask: a, n: n.current });
  };
  // Whether it's still here, and what it has up, when GitHub answers.
  const up = useRef(false);
  useEffect(() => {
    up.current = true;
    return () => {
      up.current = false;
    };
  }, []);
  const askedNow = useLatest(asked);
  const showingNow = useLatest(showing);
  const ask = (a: MergeDoneAsk) => {
    const key = a.session.key;
    if (!a.refused) {
      open(freshAsk(a, refusedFor));
      return inFront;
    }
    if (!comesBack(key, { up: up.current, open: !!askedNow.current, showing: showingNow.current })) {
      refusedFor.set(key, a);
      return false;
    }
    open(a);
    return inFront;
  };
  // A card that comes to a session whose merge was refused while it was elsewhere, with no dialog
  // up: the dialog, back.
  useEffect(() => {
    if (showing === undefined || asked) return;
    const left = refusedFor.get(showing);
    if (!left) return;
    refusedFor.delete(showing);
    n.current += 1;
    setAsked({ ask: left, n: n.current });
  }, [showing, asked]);
  return { ask, dialog: asked && <MergeDoneDialog key={asked.n} ask={asked.ask} onClose={() => setAsked(null)} /> };
}

interface MergeDoneDialogProps {
  ask: MergeDoneAsk;
  onClose: () => void;
}

function MergeDoneDialog({ ask, onClose }: MergeDoneDialogProps) {
  const { pr } = ask;
  const [read, setRead] = useState<{ box?: Box; error?: string }>({});
  const [again, setAgain] = useState(0);
  const reloads = useReloads();
  useEffect(() => {
    let live = true;
    mergeBox(pr.repoSlug, pr.number)
      .then((box) => live && setRead({ box }))
      // Read before, the box stays as it was.
      .catch((e: unknown) => live && setRead((was) => ({ box: was.box, error: errorText(e) })));
    return () => {
      live = false;
    };
  }, [pr.repoSlug, pr.number, again, reloads]);
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
      className={cx('ca-mergedlg', 'ca-mergedonedlg')}
      title="Merge and done"
      lede={mergeDoneWords(ask)}
      onClose={onClose}
      aside={ask.refused ? <p className="ca-form-error ca-mergedlg-error">{ask.refused.why}</p> : undefined}
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
  const { pr, refused } = ask;
  const [picked, keepMethod] = useMethod(pr.repoSlug);
  // Back after a refusal, the method it went with, while the repo still allows it, whatever was
  // picked for the repo since.
  const [chosen, choose] = useState<MergeMethod | null>(refused?.commit.method ?? null);
  const method = pickMethod(box, chosen ?? picked);
  // The commit as GitHub would write it, for the method it opens on and each picked after; back
  // after a refusal, as it was written.
  const commit = useCommit(box, method, refused?.commit);
  const pick = (m: MergeMethod) => {
    choose(m);
    keepMethod(m);
    commit.fill(m);
  };
  const [error, setError] = useState<string | null>(refused?.why ?? null);
  // Only a merge now, as GitHub would do it from its box without waiting: the box has the rest.
  const merges = offers(box).merge && method != null;
  const said = standing(box, pr);

  const confirm = () => {
    if (!merges) return;
    if (method !== 'rebase' && !commit.title.trim()) {
      setError('The commit needs a title.');
      return;
    }
    commit.keep();
    const form: MergeForm = { method, title: commit.title, body: commit.body, email: commit.email, deleteBranch: commit.deleteBranch };
    const op = opFor(box, 'merge', form);
    if (op.op !== 'merge') return;
    // It goes as GitHub is asked; menus.ts says how it went, or brings it back with `form`.
    onClose();
    ask.then(mergeDoneAhead(pr, box.id, op, ask.session.key), doneWords(op, pr.number, box.branch), form);
  };

  return (
    <Dialog
      className={cx('ca-mergedlg', 'ca-mergedonedlg')}
      title="Merge and done"
      lede={mergeDoneWords(ask)}
      onClose={onClose}
      onSubmit={confirm}
      aside={error ? <p className="ca-form-error ca-mergedlg-error">{error}</p> : undefined}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" icon="merge" disabled={!merges} autoFocus={!refused} title={method ? METHOD[method].what : undefined}>
            Merge and done
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        {merges && method ? (
          <>
            <MethodChip box={box} method={method} pick={pick} onError={setError} />
            <CommitFields box={box} mode="merge" method={method} commit={commit} busy={false} onError={setError} />
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
