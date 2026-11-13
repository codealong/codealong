// A PR's merge box, under its title wherever its details show (PrInspector.tsx `PrDetails`: the
// PR panel, and a PR's card in the notch and the waiting window). Its buttons share one row with
// the window's own for the PR, Approve before them and the rest after (`lead`, `trail`), so
// merging is where approving is. It's GitHub's merge box: what you may do about the PR now
// (merge.ts says which), and under the row where it stands for merging, unless that's plainly
// ready. The main button merges with the method beside it, picked from its caret and kept per
// repo, or turns on auto-merge, or adds it to the merge queue; a box under the row merges past
// the rules for one who may. Merging, like GitHub's, takes a second press, in a dialog as Merge
// and done… has (`MergeDialog`): the commit's title and message as GitHub would write them for
// that method, to change, the method itself, the email it goes under, and whether the branch goes
// with it. ⌘⏎ or Return in its title confirms and Escape puts it away. It's lifted out of the box
// (primitives.tsx `DialogHost`): over the window, or over the card it's on in a floating window,
// and a PR link's card (PrLink.tsx) stays up under it. Confirmed, a merge shows done at once, as
// though GitHub had merged it (`act`): the dialog put away, the box merged and saying what it did,
// until GitHub answers; one it refuses brings the dialog back with the commit as it was written,
// and why. Bringing the branch up to date, taking it out of draft, turning auto-merge off, leaving
// the queue and deleting a merged branch are a press each, as on GitHub. The commit's fields
// (`CommitFields`, `useCommit`), the method's chip (`MethodChip`) and the method kept per repo
// (`useMethod`) are Merge and done…'s too (MergeDone.tsx).
//
// The box is read when it shows (prmerge.rs `merge_box`), kept for the PR as long as it isn't
// updated, read again a few times while GitHub works out whether it can merge, again after each
// thing done here, and as the window reloads (reload.ts). What was done, or why it wasn't, shows under the row.

import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode, type Ref } from 'react';
import { mergeAhead } from '../ahead';
import { mergeBox, openUrl } from '../api';
import { below, showContextMenu, type MenuSpec } from '../contextMenu';
import { boxKey, commitText, confirmLabel, DELETE_KEY, deletesWith, doneWords, mainMode, mergeAsk, mergingBox, METHOD, METHODS_KEY, methodsByRepo, modeLabel, offers, opFor, pickMethod, plainlyReady, rereads, standing, type MergeForm, type MergeMode, type Tone } from '../merge';
import { errorText, type Report } from '../report';
import { onReload, reloadCount, useReloads } from '../reload';
import { useLatest } from '../latest';
import { flag, readStored, writeStored } from '../stored';
import type { MergeBox as Box, MergeMethod, MergeOp, PrItem } from '../types';
import { Button, CheckBox, cx, Dialog, Icon } from './primitives';
import { cmd, keysIn } from '../platform';

/** Boxes already read, by what they were read for (merge.ts `boxKey`), so going back to a PR
 *  shows it at once. */
const cache = new Map<string, Box>();
onReload(() => cache.clear());

/** How often, and how many times, it's read again while GitHub works out whether it can merge, or
 *  says otherwise than the board. */
export const RECHECK_MS = 3000;
export const RECHECKS = 3;

export const TONE_ICON: Record<Tone, string> = { ok: 'check', wait: 'clock', bad: 'alert', done: 'merge' };

/** What's being done, for the button that does it. */
type Doing = MergeOp['op'] | null;

/** What was done here, or why it wasn't, under the box. */
type Note = { text: string; error?: boolean } | null;

/** The window's own buttons for the PR, in the row with the merge's. */
interface Row {
  /** Before the merge's: Approve. */
  lead?: ReactNode;
  /** After them: Snooze, GitHub, Review with. */
  trail?: ReactNode;
  /** A window that goes on to the next PR as one is merged (a PR's card in a queue): the merge
   *  shows at once (ahead.ts `mergeAhead`), so the PR has left what waits on you as it's asked,
   *  and one GitHub refuses comes back with why, on whatever the window shows then. */
  goesOn?: MergeGoesOn;
  /** How the window says why GitHub refused a merge once the box has gone (another PR shown, the
   *  card put away); while it's up, the box says it. */
  report?: Report;
}

export interface MergeGoesOn {
  left: () => void;
  failed: (why: string) => void;
}

/** The row: the window's buttons with the merge's between them, when there's any. */
function Actions({ lead, trail, children }: Row & { children?: ReactNode }) {
  if (!lead && !trail && !children) return null;
  return (
    <div className="ca-insp-actions">
      {lead}
      {children}
      {trail}
    </div>
  );
}

/** One PR's box: its details give each PR one of its own (`key`). */
export function MergeBox({ pr, lead, trail, goesOn, report }: { pr: PrItem } & Row) {
  // Read again as the board's word on where the PR stands changes, as when its checks finish,
  // which doesn't update the PR: else it said blocked while its pills said ready to merge.
  const key = boxKey(pr);
  const [read, setRead] = useState<{ key: string; box?: Box; error?: string }>({ key: '' });
  const [again, setAgain] = useState(0);
  const reloads = useReloads();
  const rechecks = useRef({ key, n: 0 });
  if (rechecks.current.key !== key) rechecks.current = { key, n: 0 };
  // Kept here, so what was done stays said once the PR it merged has nothing left to offer.
  const [note, setNote] = useState<Note>(null);

  useEffect(() => {
    const hit = again === 0 ? cache.get(key) : undefined;
    if (hit) {
      setRead({ key, box: hit });
      return;
    }
    let live = true;
    mergeBox(pr.repoSlug, pr.number)
      .then((b) => {
        // Not one asked before a reload, which would put back what the reload forgot.
        if (reloadCount() === reloads) cache.set(key, b);
        if (live) setRead({ key, box: b });
      })
      .catch((e: unknown) => {
        if (live) setRead((was) => ({ key, box: was.box, error: errorText(e) }));
      });
    return () => {
      live = false;
    };
  }, [key, again, reloads, pr.repoSlug, pr.number]);

  // The box last read for this PR shows while it's read again, so its buttons hold still.
  const box = read.box;
  const error = read.key === key ? read.error : undefined;

  // GitHub works out whether a PR can merge when it's first asked, and can take a moment to agree
  // with what the board read: read again a few times.
  const unsure = !!box && read.key === key && rereads(box, pr);
  useEffect(() => {
    if (!unsure || rechecks.current.n >= RECHECKS) return;
    const t = window.setTimeout(() => {
      rechecks.current.n += 1;
      setAgain((n) => n + 1);
    }, RECHECK_MS);
    return () => window.clearTimeout(t);
  }, [unsure, box]);

  // A merged or closed PR has nothing left to do here but delete its branch.
  if (!box || (box.state !== 'OPEN' && !offers(box).deleteBranch && !note)) {
    const row = <Actions lead={lead} trail={trail} />;
    return error && !box ? (
      <div className="ca-merge">
        {row}
        <p className="ca-form-error">Couldn’t read how it merges: {error}</p>
      </div>
    ) : (
      row
    );
  }
  return <Loaded pr={pr} box={box} note={note} setNote={setNote} reread={() => setAgain((n) => n + 1)} lead={lead} trail={trail} goesOn={goesOn} report={report} />;
}

function Loaded({ pr, box, note, setNote, reread, lead, trail, goesOn, report }: { pr: PrItem; box: Box; note: Note; setNote: (n: Note) => void; reread: () => void } & Row) {
  const [doing, setDoing] = useState<Doing>(null);
  // The box as it was read when GitHub said it merged the PR, until it's read again.
  const [mergedOver, setMergedOver] = useState<Box | null>(null);
  const boxNow = useLatest(box);
  // A merge on its way shows the box merged (merge.ts `mergingBox`), whatever GitHub says of the
  // PR as it's read meanwhile, and once merged, until the box is read again.
  const shown = doing === 'merge' || mergedOver === box ? mergingBox(box) : box;
  const can = offers(shown);
  const said = standing(shown, pr);
  const [bypassing, setBypassing] = useState(false);
  const mode = mainMode(can, bypassing);
  const [picked, pickFor] = useMethod(pr.repoSlug);
  const method = pickMethod(box, picked);
  const [writing, setWriting] = useState(false);
  const commit = useCommit(box);
  const { title, body, email, deleteBranch, fill } = commit;
  const titleBox = useRef<HTMLInputElement>(null);
  // Whether the box is still up when GitHub answers, to say a refusal itself.
  const up = useRef(true);
  useEffect(() => {
    up.current = true;
    return () => {
      up.current = false;
    };
  }, []);

  const pick = (m: MergeMethod) => {
    pickFor(m);
    if (writing) fill(m);
  };

  /** Does it. A merge shows done as it's asked, here and in every window (ahead.ts
   *  `mergeAhead`): the commit put away, what it did said and the box merged until GitHub answers.
   *  One GitHub refuses brings the commit back as it was written, saying why, or where the box
   *  has gone by then, the window says why. In a window that goes on, the window goes on to the
   *  next PR at once, and one GitHub refuses comes back there. */
  const act = (op: MergeOp) => {
    if (doing) return;
    setDoing(op.op);
    const merge = op.op === 'merge';
    setNote(merge ? { text: doneWords(op, pr.number, box.branch) } : null);
    if (merge) setWriting(false);
    const going = merge ? goesOn : undefined;
    const done = mergeAhead(pr, box.id, op);
    going?.left();
    done
      .then(
        (warning) => {
          setNote(warning ? { text: warning, error: true } : { text: doneWords(op, pr.number, box.branch) });
          setWriting(false);
          if (merge) setMergedOver(boxNow.current);
          reread();
        },
        (e: unknown) => {
          const why = errorText(e);
          if (going) going.failed(why);
          // Every window that shows a box gives it a way to say this; one without leaves the PR to
          // show open again.
          else if (!up.current) report?.error(why);
          else {
            setNote({ text: why, error: true });
            if (merge) setWriting(true);
          }
        },
      )
      .finally(() => setDoing(null));
  };

  const start = () => {
    if (!mode || !method) return;
    if (mode === 'queue') {
      act(opFor(box, mode, { method, title: '', body: '', email: null, deleteBranch: false }));
      return;
    }
    fill(method);
    setNote(null);
    setWriting(true);
    // The title takes the keyboard from the dialog with its start in view: a long one would show
    // only its end. Brought back by a refusal, it doesn't: the dialog takes it itself.
    window.setTimeout(() => {
      const el = titleBox.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(0, 0);
      el.scrollLeft = 0;
    }, 0);
  };

  const confirm = () => {
    if (!mode || !method) return;
    if (method !== 'rebase' && !title.trim()) {
      setNote({ text: 'The commit needs a title.', error: true });
      titleBox.current?.focus();
      return;
    }
    commit.keep();
    act(opFor(box, mode, { method, title, body, email, deleteBranch }));
  };

  // ⌘⏎ confirms from the fields, the message's too, before a card's own ⌘⏎ (which approves)
  // hears it. Escape is the dialog's.
  const onFieldKey = (e: ReactKeyboardEvent) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === 'Enter' && cmd(e)) {
      e.preventDefault();
      e.stopPropagation();
      confirm();
    }
  };

  const show = (items: MenuSpec, el: Element) => void showContextMenu(items, below(el)).catch((e: unknown) => setNote({ text: errorText(e), error: true }));
  const methodMenu = (el: Element) => show(methodItems(box, method, pick), el);
  const updateMenu = (el: Element) =>
    show(
      [
        { label: 'Update with a merge commit', run: () => act({ op: 'update-branch', rebase: false, head: box.head }) },
        { label: 'Update with a rebase', run: () => act({ op: 'update-branch', rebase: true, head: box.head }) },
      ],
      el,
    );
  const ready = said.tone === 'ok' && mode === 'merge';
  const busyWord = doing === 'auto' ? 'Turning on…' : doing === 'enqueue' ? 'Queuing…' : null;
  const tone = shown.state === 'MERGED' ? 'merged' : said.tone;
  const plain = plainlyReady(shown, pr);

  const buttons = (
    <>
      {mode && method && (
        <div className="ca-split ca-merge-main">
          <button
            type="button"
            className={cx('ca-btn', ready ? 'ca-btn--primary' : 'ca-btn--secondary', 'ca-btn--md', mode !== 'queue' && 'ca-split-main', mode === 'bypass' && 'is-bypass')}
            disabled={!!doing}
            title={mode === 'queue' ? 'The queue merges it once it’s ready, with the method its rules say' : METHOD[method].what}
            onClick={start}
          >
            <Icon name="merge" size={16} />
            <span>{busyWord ?? modeLabel(mode, method)}</span>
          </button>
          {mode !== 'queue' && (
            <button type="button" className={cx('ca-btn', ready ? 'ca-btn--primary' : 'ca-btn--secondary', 'ca-btn--md', 'ca-split-caret')} aria-haspopup="menu" aria-label="How it merges" title="How it merges" disabled={!!doing} onClick={(e) => methodMenu(e.currentTarget)}>
              <Icon name="chevron-down" size={14} />
            </button>
          )}
        </div>
      )}
      {can.stopAuto && (
        <Button disabled={!!doing} onClick={() => act({ op: 'stop-auto' })}>
          {doing === 'stop-auto' ? 'Turning off…' : 'Disable auto-merge'}
        </Button>
      )}
      {can.dequeue && (
        <Button disabled={!!doing} onClick={() => act({ op: 'dequeue' })}>
          {doing === 'dequeue' ? 'Removing…' : 'Remove from queue'}
        </Button>
      )}
      {can.ready && (
        <Button disabled={!!doing} onClick={() => act({ op: 'ready' })}>
          {doing === 'ready' ? 'Marking…' : 'Ready for review'}
        </Button>
      )}
      {can.update && (
        <div className="ca-split">
          <Button icon="refresh" className="ca-split-main" disabled={!!doing} title="Merge the latest changes from its base into the branch" onClick={() => act({ op: 'update-branch', rebase: false, head: box.head })}>
            {doing === 'update-branch' ? 'Updating…' : 'Update branch'}
          </Button>
          <Button className="ca-split-caret" icon="chevron-down" aria-haspopup="menu" aria-label="How to update it" title="Update with a merge commit or a rebase" disabled={!!doing} onClick={(e) => updateMenu(e.currentTarget)} />
        </div>
      )}
      {can.conflicts && (
        <Button icon="external" title="GitHub’s editor for conflicts" onClick={() => void openUrl(`${pr.url}/conflicts`).catch((e: unknown) => setNote({ text: errorText(e), error: true }))}>
          Resolve on GitHub
        </Button>
      )}
      {can.deleteBranch && box.branchRef && (
        <Button icon="trash" disabled={!!doing} title="GitHub’s page for the PR can restore it" onClick={() => box.branchRef && act({ op: 'delete-branch', branchRef: box.branchRef })}>
          {doing === 'delete-branch' ? 'Deleting…' : `Delete ${box.branch}`}
        </Button>
      )}
    </>
  );

  return (
    <div className="ca-merge">
      <Actions lead={lead} trail={trail}>
        {buttons}
      </Actions>
      {!plain && (
        <div className={cx('ca-merge-standing', `is-${tone}`)}>
          <Icon name={shown.state === 'CLOSED' ? 'pr-closed' : TONE_ICON[said.tone]} size={16} stroke={2} />
          <div>
            <strong>{said.says}</strong>
            {said.more && <span>{said.more}</span>}
          </div>
        </div>
      )}
      {can.bypass && (
        <label className={cx('ca-check', 'ca-merge-bypass', doing && 'is-disabled')} title="You may merge it before its requirements are met: GitHub lets it past the repo’s rules">
          <CheckBox checked={bypassing} disabled={!!doing} onChange={setBypassing} />
          Merge without waiting for its requirements (bypass rules)
        </label>
      )}
      {/* While the dialog is up, what went wrong is said in it. */}
      {note && !(writing && note.error) && (
        <p className={cx('ca-merge-note', note.error && 'is-error')} role="status">
          {note.text}
        </p>
      )}
      {writing && mode && method && (
        <MergeDialog
          pr={pr}
          box={box}
          mode={mode}
          method={method}
          commit={commit}
          doing={busyWord}
          busy={!!doing}
          error={note?.error ? note.text : null}
          titleRef={titleBox}
          onKey={onFieldKey}
          onPick={pick}
          onError={(why) => setNote({ text: why, error: true })}
          onConfirm={confirm}
          onClose={() => setWriting(false)}
        />
      )}
    </div>
  );
}

interface MergeDialogProps {
  pr: PrItem;
  box: Box;
  mode: MergeMode;
  method: MergeMethod;
  commit: Commit;
  /** What the confirm button says while it's done: turning auto-merge on. */
  doing: string | null;
  busy: boolean;
  error: string | null;
  titleRef: Ref<HTMLInputElement>;
  onKey: (e: ReactKeyboardEvent) => void;
  onPick: (m: MergeMethod) => void;
  onError: (why: string) => void;
  onConfirm: () => void;
  onClose: () => void;
}

/** The merge's second press: the commit it goes in with, and the button that does it. */
function MergeDialog({ pr, box, mode, method, commit, doing, busy, error, titleRef, onKey, onPick, onError, onConfirm, onClose }: MergeDialogProps) {
  const ask = mergeAsk(mode, pr);
  const label = confirmLabel(mode, method);
  return (
    <Dialog
      lifted
      className="ca-mergedlg"
      title={ask.title}
      lede={ask.lede}
      busy={busy}
      onClose={onClose}
      onSubmit={onConfirm}
      aside={error ? <p className="ca-form-error ca-mergedlg-error">{error}</p> : undefined}
      foot={
        <>
          <Button variant="ghost" size="sm" disabled={busy} onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" icon="merge" disabled={busy} title={keysIn(`${label} (⌘⏎)`)}>
            {doing ?? label}
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        {mode === 'bypass' && <p className="ca-merge-warn">It merges now, without waiting for its requirements: past the repo’s rules.</p>}
        <MethodChip box={box} method={method} busy={busy} pick={onPick} onError={onError} />
        <CommitFields box={box} mode={mode} method={method} commit={commit} busy={busy} titleRef={titleRef} onKey={onKey} onError={onError} />
      </div>
    </Dialog>
  );
}

/** The method a merge goes by, in its dialog, from a menu of the ways the repo allows. */
export function MethodChip({ box, method, busy = false, pick, onError }: { box: Pick<Box, 'methods'>; method: MergeMethod; busy?: boolean; pick: (m: MergeMethod) => void; onError: (why: string) => void }) {
  return (
    <div className="ca-merge-method">
      <span>Merge by</span>
      <button
        type="button"
        className="ca-modelmenu-chip"
        aria-haspopup="menu"
        title="How it merges: kept for this repo"
        disabled={busy}
        onClick={(e) => void showContextMenu(methodItems(box, method, pick), below(e.currentTarget)).catch((err: unknown) => onError(errorText(err)))}
      >
        <span>{METHOD[method].label}</span>
        <Icon name="chevron-down" size={12} />
      </button>
    </div>
  );
}

/** The method picked for merges in a repo, kept for the next (`METHODS_KEY`), and the way to pick
 *  another: null until one is, when the box starts on GitHub's (merge.ts `pickMethod`). */
export function useMethod(repoSlug: string): [MergeMethod | null, (m: MergeMethod) => void] {
  const [picked, setPicked] = useState<MergeMethod | null>(() => readStored<Record<string, MergeMethod>>(METHODS_KEY, methodsByRepo, {})[repoSlug] ?? null);
  const pick = (m: MergeMethod) => {
    setPicked(m);
    writeStored(METHODS_KEY, methodsByRepo, { ...readStored<Record<string, MergeMethod>>(METHODS_KEY, methodsByRepo, {}), [repoSlug]: m });
  };
  return [picked, pick];
}

/** The ways of merging as a menu, the one in use checked and those the repo doesn't allow dimmed. */
export function methodItems(box: Pick<Box, 'methods'>, method: MergeMethod | null, pick: (m: MergeMethod) => void): MenuSpec {
  return (['merge', 'squash', 'rebase'] as const).map((m) => ({ label: METHOD[m].label, checked: m === method, enabled: box.methods.includes(m), run: () => pick(m) }));
}

/** The commit a merge goes in with, as it's written before it's confirmed: its title and message
 *  as GitHub would write them for a method (`fill`; `first`'s from the start), the email it goes
 *  under, and whether the branch goes with it, the last way that was left (`keep`, as a merge is
 *  confirmed). `written` starts it as it was written before, for a merge GitHub refused. */
export function useCommit(box: Box, first: MergeMethod | null = null, written?: MergeForm) {
  const [title, setTitle] = useState(() => written?.title ?? (first && commitText(box, first)?.title) ?? '');
  const [body, setBody] = useState(() => written?.body ?? (first && commitText(box, first)?.body) ?? '');
  const [email, setEmail] = useState<string | null>(written ? written.email : box.defaultEmail);
  const [deleteBranch, setDeleteBranch] = useState(() => written?.deleteBranch ?? readStored(DELETE_KEY, flag, false));
  /** The commit as GitHub would write it for `m`, in the fields. */
  const fill = (m: MergeMethod) => {
    const text = commitText(box, m);
    setTitle(text?.title ?? '');
    setBody(text?.body ?? '');
  };
  const keep = () => writeStored(DELETE_KEY, flag, deleteBranch);
  return { title, setTitle, body, setBody, email, setEmail, deleteBranch, setDeleteBranch, fill, keep };
}

export type Commit = ReturnType<typeof useCommit>;

/** The commit's fields, in the dialog a merge is confirmed in and in Merge and done…
 *  (MergeDone.tsx): its title and message, or what a rebase does, which writes none; the email
 *  it goes under, where there's a choice; and whether the branch goes with it. `onKey` hears
 *  the keys in them; `onError` why the email's menu didn't show. */
export function CommitFields({ box, mode, method, commit, busy, titleRef, onKey, onError }: { box: Box; mode: MergeMode; method: MergeMethod; commit: Commit; busy: boolean; titleRef?: Ref<HTMLInputElement>; onKey?: (e: ReactKeyboardEvent) => void; onError: (why: string) => void }) {
  const { title, setTitle, body, setBody, email, setEmail, deleteBranch, setDeleteBranch } = commit;
  const writes = method !== 'rebase';
  const emailMenu = (el: Element) =>
    void showContextMenu(
      box.emails.map((e) => ({ label: e, checked: e === email, run: () => setEmail(e) })),
      below(el),
    ).catch((e: unknown) => onError(errorText(e)));
  return (
    <>
      {writes ? (
        <>
          <label className="ca-field">
            <span>Commit title</span>
            <input ref={titleRef} value={title} spellCheck={false} disabled={busy} onChange={(e) => setTitle(e.target.value)} onKeyDown={onKey} />
          </label>
          <label className="ca-field">
            <span>Commit message</span>
            <textarea value={body} rows={Math.min(10, Math.max(3, body.split('\n').length + 1))} spellCheck={false} disabled={busy} onChange={(e) => setBody(e.target.value)} onKeyDown={onKey} />
          </label>
        </>
      ) : (
        <p className="ca-meta">{METHOD.rebase.what}</p>
      )}
      <div className="ca-merge-opts">
        {writes && box.emails.length > 1 && (
          <button type="button" className="ca-modelmenu-chip" title="The email the commit goes under" aria-haspopup="menu" disabled={busy} onClick={(e) => emailMenu(e.currentTarget)}>
            <span>{email ?? 'Email'}</span>
            <Icon name="chevron-down" size={12} />
          </button>
        )}
        {deletesWith(box, mode) && (
          <label className={cx('ca-check', busy && 'is-disabled')}>
            <CheckBox checked={deleteBranch} disabled={busy} onChange={setDeleteBranch} onKeyDown={onKey} />
            Delete <code className="ca-ic">{box.branch}</code> once merged
          </label>
        )}
        {box.deletesBranch && mode !== 'auto' && box.branchRef && <span className="ca-meta">GitHub deletes {box.branch} once it merges.</span>}
      </div>
    </>
  );
}
