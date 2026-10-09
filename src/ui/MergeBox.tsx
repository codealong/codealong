// A PR's merge box, under its title wherever its details show (PrInspector.tsx `PrDetails`: the
// PR panel, and a PR's card in the notch and the waiting window). Its buttons share one row with
// the window's own for the PR, Approve before them and the rest after (`lead`, `trail`), so
// merging is where approving is. It's GitHub's merge box: what you may do about the PR now
// (merge.ts says which), and under the row where it stands for merging, unless that's plainly
// ready. The main button merges with the method beside it, picked from its caret and kept per
// repo, or turns on auto-merge, or adds it to the merge queue; a box under the row merges past
// the rules for one who may. Merging, like GitHub's, takes a second press: under the row, the
// commit's title and message as GitHub would write them for that method, to change, the email it
// goes under, and whether the branch goes with it. ⌘⏎ in them confirms and Escape puts them away.
// Bringing the branch up to date, taking it out of draft, turning auto-merge off, leaving the
// queue and deleting a merged branch are a press each, as on GitHub.
//
// The box is read when it shows (prmerge.rs `merge_box`), kept for the PR as long as it isn't
// updated, read again a few times while GitHub works out whether it can merge, and again after
// each thing done here. What was done, or why it wasn't, shows under the row.

import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from 'react';
import { mergeBox, mergePr, openUrl } from '../api';
import { below, showContextMenu, type MenuSpec } from '../contextMenu';
import { commitText, confirmLabel, DELETE_KEY, deletesWith, doneWords, mainMode, METHOD, METHODS_KEY, methodsByRepo, modeLabel, offers, opFor, pickMethod, standing, type Tone } from '../merge';
import { errorText } from '../report';
import { flag, readStored, writeStored } from '../stored';
import type { MergeBox as Box, MergeMethod, MergeOp, PrItem } from '../types';
import { Button, cx, Icon } from './primitives';

/** Boxes already read, by PR and last update, so going back to a PR shows it at once. */
const cache = new Map<string, Box>();

/** How often, and how many times, it's read again while GitHub works out whether it can merge. */
const RECHECK_MS = 3000;
const RECHECKS = 3;

const TONE_ICON: Record<Tone, string> = { ok: 'check', wait: 'clock', bad: 'alert', done: 'merge' };

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
export function MergeBox({ pr, lead, trail }: { pr: PrItem } & Row) {
  const key = `${pr.key}@${pr.updatedAt}`;
  const [read, setRead] = useState<{ key: string; box?: Box; error?: string }>({ key: '' });
  const [again, setAgain] = useState(0);
  const rechecks = useRef(0);
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
        cache.set(key, b);
        if (live) setRead({ key, box: b });
      })
      .catch((e: unknown) => {
        if (live) setRead((was) => ({ key, box: was.key === key ? was.box : undefined, error: errorText(e) }));
      });
    return () => {
      live = false;
    };
  }, [key, again, pr.repoSlug, pr.number]);

  const box = read.key === key ? read.box : undefined;
  const error = read.key === key ? read.error : undefined;

  // GitHub works out whether a PR can merge when it's first asked: read again a few times.
  const unknown = box?.state === 'OPEN' && box.mergeState === 'UNKNOWN';
  useEffect(() => {
    if (!unknown || rechecks.current >= RECHECKS) return;
    const t = window.setTimeout(() => {
      rechecks.current += 1;
      setAgain((n) => n + 1);
    }, RECHECK_MS);
    return () => window.clearTimeout(t);
  }, [unknown, box]);

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
  return <Loaded pr={pr} box={box} note={note} setNote={setNote} reread={() => setAgain((n) => n + 1)} lead={lead} trail={trail} />;
}

function Loaded({ pr, box, note, setNote, reread, lead, trail }: { pr: PrItem; box: Box; note: Note; setNote: (n: Note) => void; reread: () => void } & Row) {
  const can = offers(box);
  const said = standing(box, pr);
  const [bypassing, setBypassing] = useState(false);
  const mode = mainMode(can, bypassing);
  const [picked, setPicked] = useState<MergeMethod | null>(() => readStored<Record<string, MergeMethod>>(METHODS_KEY, methodsByRepo, {})[pr.repoSlug] ?? null);
  const method = pickMethod(box, picked);
  const [writing, setWriting] = useState(false);
  const [title, setTitle] = useState('');
  const [body, setBody] = useState('');
  const [email, setEmail] = useState<string | null>(box.defaultEmail);
  const [deleteBranch, setDeleteBranch] = useState(() => readStored(DELETE_KEY, flag, false));
  const [doing, setDoing] = useState<Doing>(null);
  const titleBox = useRef<HTMLInputElement>(null);

  /** The commit as GitHub would write it for `m`, in the fields. */
  const fill = (m: MergeMethod) => {
    const text = commitText(box, m);
    setTitle(text?.title ?? '');
    setBody(text?.body ?? '');
  };

  const pick = (m: MergeMethod) => {
    setPicked(m);
    writeStored(METHODS_KEY, methodsByRepo, { ...readStored<Record<string, MergeMethod>>(METHODS_KEY, methodsByRepo, {}), [pr.repoSlug]: m });
    if (writing) fill(m);
  };

  const act = (op: MergeOp) => {
    if (doing) return;
    setDoing(op.op);
    setNote(null);
    mergePr(pr.number, box.id, op)
      .then(
        (warning) => {
          setNote(warning ? { text: warning, error: true } : { text: doneWords(op, pr.number, box.branch) });
          setWriting(false);
          reread();
        },
        (e: unknown) => setNote({ text: errorText(e), error: true }),
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
    // The title takes the keyboard with its start in view: a long one would show only its end.
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
    writeStored(DELETE_KEY, flag, deleteBranch);
    act(opFor(box, mode, { method, title, body, email, deleteBranch }));
  };

  // ⌘⏎ confirms from the fields, before a card's own ⌘⏎ (which approves) hears it; Escape puts
  // them away and nothing else.
  const onFieldKey = (e: ReactKeyboardEvent) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === 'Enter' && e.metaKey) {
      e.preventDefault();
      e.stopPropagation();
      confirm();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      setWriting(false);
    }
  };

  const show = (items: MenuSpec, el: Element) => void showContextMenu(items, below(el)).catch((e: unknown) => setNote({ text: errorText(e), error: true }));
  const methodMenu = (el: Element) =>
    show(
      (['merge', 'squash', 'rebase'] as const).map((m) => ({ label: METHOD[m].label, checked: m === method, enabled: box.methods.includes(m), run: () => pick(m) })),
      el,
    );
  const updateMenu = (el: Element) =>
    show(
      [
        { label: 'Update with a merge commit', run: () => act({ op: 'update-branch', rebase: false, head: box.head }) },
        { label: 'Update with a rebase', run: () => act({ op: 'update-branch', rebase: true, head: box.head }) },
      ],
      el,
    );
  const emailMenu = (el: Element) => show(box.emails.map((e) => ({ label: e, checked: e === email, run: () => setEmail(e) })), el);

  const writes = method !== 'rebase';
  const ready = said.tone === 'ok' && mode === 'merge';
  const busyWord = doing === 'merge' ? 'Merging…' : doing === 'auto' ? 'Turning on…' : doing === 'enqueue' ? 'Queuing…' : null;
  const tone = box.state === 'MERGED' ? 'merged' : said.tone;
  // A PR that's plainly ready says so in its pills and its button: no line under the row.
  const plain = can.merge && box.canMerge && (box.mergeState === 'CLEAN' || box.mergeState === 'HAS_HOOKS');

  const buttons = !writing && (
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
          <Icon name={box.state === 'CLOSED' ? 'pr-closed' : TONE_ICON[said.tone]} size={16} stroke={2} />
          <div>
            <strong>{said.says}</strong>
            {said.more && <span>{said.more}</span>}
          </div>
        </div>
      )}
      {can.bypass && !writing && (
        <label className={cx('ca-check', 'ca-merge-bypass', doing && 'is-disabled')} title="You may merge it before its requirements are met: GitHub lets it past the repo’s rules">
          <input type="checkbox" checked={bypassing} disabled={!!doing} onChange={(e) => setBypassing(e.target.checked)} />
          <span className="ca-check-box" aria-hidden="true">
            <Icon name="check" size={11} stroke={2.5} />
          </span>
          Merge without waiting for its requirements (bypass rules)
        </label>
      )}
      {writing && mode && method && (
        <div className="ca-merge-write">
          {mode === 'bypass' && <p className="ca-merge-warn">It merges now, without waiting for its requirements: past the repo’s rules.</p>}
          {mode === 'auto' && <p className="ca-meta">GitHub merges it with this commit once its requirements are met.</p>}
          {writes ? (
            <>
              <label className="ca-field">
                <span>Commit title</span>
                <input ref={titleBox} value={title} spellCheck={false} disabled={!!doing} onChange={(e) => setTitle(e.target.value)} onKeyDown={onFieldKey} />
              </label>
              <label className="ca-field">
                <span>Commit message</span>
                <textarea value={body} rows={Math.min(10, Math.max(3, body.split('\n').length + 1))} spellCheck={false} disabled={!!doing} onChange={(e) => setBody(e.target.value)} onKeyDown={onFieldKey} />
              </label>
            </>
          ) : (
            <p className="ca-meta">{METHOD.rebase.what}</p>
          )}
          <div className="ca-merge-opts">
            {writes && box.emails.length > 1 && (
              <button type="button" className="ca-modelmenu-chip" title="The email the commit goes under" aria-haspopup="menu" disabled={!!doing} onClick={(e) => emailMenu(e.currentTarget)}>
                <span>{email ?? 'Email'}</span>
                <Icon name="chevron-down" size={12} />
              </button>
            )}
            {deletesWith(box, mode) && (
              <label className={cx('ca-check', doing && 'is-disabled')}>
                <input type="checkbox" checked={deleteBranch} disabled={!!doing} onChange={(e) => setDeleteBranch(e.target.checked)} onKeyDown={onFieldKey} />
                <span className="ca-check-box" aria-hidden="true">
                  <Icon name="check" size={11} stroke={2.5} />
                </span>
                Delete <code className="ca-ic">{box.branch}</code> once merged
              </label>
            )}
            {box.deletesBranch && mode !== 'auto' && box.branchRef && <span className="ca-meta">GitHub deletes {box.branch} once it merges.</span>}
          </div>
          <div className="ca-merge-confirm">
            <Button variant="primary" size="sm" icon="merge" disabled={!!doing} onClick={confirm} title={`${confirmLabel(mode, method)} (⌘⏎)`}>
              {busyWord ?? confirmLabel(mode, method)}
            </Button>
            <Button variant="ghost" size="sm" disabled={!!doing} onClick={() => setWriting(false)}>
              Cancel
            </Button>
          </div>
        </div>
      )}
      {note && (
        <p className={cx('ca-merge-note', note.error && 'is-error')} role="status">
          {note.text}
        </p>
      )}
    </div>
  );
}
