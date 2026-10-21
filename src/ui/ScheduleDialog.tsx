// Making a schedule, or changing one (Schedules view): its name; when it runs, at times written in
// a few words (`weekdays 9:00`, previewed as you type: schedule_times), when a command it runs
// prints something new, when something happens on the board, or only when a script fires it;
// what it does, a new session each time, one session kept going, or a message to a session that's
// there; where and on what (the agent menu's Auto, routing groups and accounts, AgentPromptBox.tsx)
// with what it's told; and what it does when the last run is still going or a time is missed. The
// engine checks what's saved (schedules/mod.rs `check`) and says why it can't be.

import { useEffect, useMemo, useRef, useState } from 'react';
import { scheduleOp, scheduleTimes } from '../api';
import { fireCommand, scheduleFolders, scheduleTargets } from '../schedules';
import { useStartAccounts, useStartGroups } from '../startAccounts';
import type { Board, Schedule, ScheduleEvent, ScheduleTimes, ScheduleWhen, StartPlace } from '../types';
import { AgentPromptBox } from './AgentPromptBox';
import { BoxText, Button, CheckBox, Dialog, Icon, Seg } from './primitives';

type WhenKind = ScheduleWhen['kind'];

const WHENS: { id: WhenKind; label: string; title: string }[] = [
  { id: 'time', label: 'At times', title: 'At times you write: weekdays 9:00, every 2h, monthly 1 9:00' },
  { id: 'watch', label: 'A watch', title: 'When a command it runs every so often prints something it hasn’t before' },
  { id: 'event', label: 'On the board', title: 'When a PR merges, a PR’s checks fail, or a todo is ready' },
  { id: 'fired', label: 'When fired', title: 'Only when a script fires it (codealong schedule fire), or you press Run now' },
];

const PRESETS = ['weekdays 9:00', 'daily 18:00', 'every 2h', 'mon 9:00', 'monthly 1 9:00'];

const EVENTS: { id: ScheduleEvent; label: string }[] = [
  { id: 'pr-merged', label: 'A PR merges' },
  { id: 'checks-failed', label: 'A PR’s checks fail' },
  { id: 'todo-ready', label: 'A todo is ready' },
];

const TOS: { id: Schedule['run']['to']; label: string; title: string }[] = [
  { id: 'new', label: 'New session', title: 'A new session each time' },
  { id: 'thread', label: 'One session', title: 'The first run starts a session, and each later run is sent to it' },
  { id: 'session', label: 'A session', title: 'Each run is sent to a session that’s already there' },
];

const PLACES: { id: StartPlace | ''; label: string }[] = [
  { id: '', label: 'Out of sight' },
  { id: 'background', label: 'Background' },
  { id: 'terminal', label: 'Terminal' },
  { id: 'codealong', label: 'In Codealong' },
  { id: 'codex-app', label: 'Codex app' },
];

/** A `when` of another kind, keeping what the dialog had for each so switching back finds it. */
function whenOf(kind: WhenKind, kept: Partial<Record<WhenKind, ScheduleWhen>>): ScheduleWhen {
  if (kept[kind]) return kept[kind];
  switch (kind) {
    case 'time':
      return { kind, rule: 'weekdays 9:00' };
    case 'watch':
      return { kind, command: '', every: '5m', mode: 'lines' };
    case 'event':
      return { kind, event: 'pr-merged', repo: '', label: '', mine: false };
    case 'fired':
      return { kind };
  }
}

/** A select drawn as the start menus' are. */
function Select<T extends string>({ label, value, options, onPick }: { label: string; value: T; options: { id: T; label: string }[]; onPick: (v: T) => void }) {
  return (
    <div className="ca-start-select">
      <select aria-label={label} value={value} onChange={(e) => onPick(e.target.value as T)}>
        {options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.label}
          </option>
        ))}
      </select>
      <Icon name="chevron-down" size={12} />
    </div>
  );
}

export interface ScheduleDialogProps {
  /** The schedule to change, or a new one's draft (no id). */
  schedule: Schedule;
  board: Board;
  now: number;
  onClose: () => void;
  /** It was saved: the saved schedule's id, to select it. */
  onSaved: (id: string) => void;
}

export function ScheduleDialog({ schedule, board, now, onClose, onSaved }: ScheduleDialogProps) {
  const [draft, setDraft] = useState<Schedule>(schedule);
  const [kept, setKept] = useState<Partial<Record<WhenKind, ScheduleWhen>>>({ [schedule.when.kind]: schedule.when });
  const [times, setTimes] = useState<ScheduleTimes | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const accounts = useStartAccounts();
  const groups = useStartGroups();
  const folders = useMemo(() => scheduleFolders(board.lanes), [board.lanes]);
  const targets = useMemo(() => scheduleTargets(board.lanes, draft.run.session), [board.lanes, draft.run.session]);
  const name = useRef<HTMLInputElement>(null);
  useEffect(() => name.current?.focus(), []);

  const when = draft.when;
  const run = draft.run;
  const setWhen = (w: ScheduleWhen) => {
    setDraft((d) => ({ ...d, when: w }));
    setKept((k) => ({ ...k, [w.kind]: w }));
  };
  const setRun = (r: Partial<Schedule['run']>) => setDraft((d) => ({ ...d, run: { ...d.run, ...r } }));

  // A time is read back as it's written: in words and its next few times, or why it can't be.
  const rule = when.kind === 'time' ? when.rule : null;
  useEffect(() => {
    if (rule === null) return;
    let live = true;
    const t = setTimeout(() => {
      scheduleTimes(rule).then(
        (r) => live && setTimes(r),
        // The preview is a help, not the check: saving says what's wrong.
        () => live && setTimes(null),
      );
    }, 200);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [rule]);

  const save = () => {
    setBusy(true);
    setError(null);
    scheduleOp({ op: 'save', schedule: { ...draft, name: draft.name.trim() } }).then(
      (saved) => {
        setBusy(false);
        onSaved((saved as Schedule | null)?.id ?? draft.id);
        onClose();
      },
      (e: unknown) => {
        setBusy(false);
        setError(String(e));
      },
    );
  };

  const folderOptions = [{ id: '', label: 'A chat of its own, in no project' }, ...folders, ...(run.folder && !folders.some((f) => f.id === run.folder) ? [{ id: run.folder, label: run.folder }] : [])];
  const labels = [{ id: '', label: 'Any todo' }, ...board.labels.map((l) => ({ id: l.name, label: `Labeled ${l.name}` }))];
  const repos = [...new Set(board.lanes.filter((l) => !l.device && l.repoSlug).map((l) => l.repoSlug as string))].sort();

  return (
    <Dialog
      className="ca-scheddlg"
      title={schedule.id ? 'Schedule settings' : 'New schedule'}
      lede="A session Codealong starts, or a message it sends one, when its time comes or something happens. What it watches costs no usage: an agent runs only when it does."
      busy={busy}
      onClose={onClose}
      onSubmit={save}
      aside={error ? <span className="ca-form-error">{error}</span> : undefined}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" disabled={busy || !draft.name.trim()}>
            {schedule.id ? 'Save' : 'Make schedule'}
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body ca-scheddlg-body">
        <label className="ca-field">
          <span>Name</span>
          <input ref={name} value={draft.name} spellCheck={false} placeholder="Morning triage" onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
        </label>

        <div className="ca-field">
          <span>When</span>
          <Seg label="When" options={WHENS} value={when.kind} onPick={(k) => setWhen(whenOf(k, kept))} />
        </div>
        {when.kind === 'time' && (
          <div className="ca-field">
            <input aria-label="Times" value={when.rule} spellCheck={false} onChange={(e) => setWhen({ kind: 'time', rule: e.target.value })} />
            <div className="ca-scheddlg-presets">
              {PRESETS.map((p) => (
                <button key={p} type="button" className="ca-linkbtn" onClick={() => setWhen({ kind: 'time', rule: p })}>
                  {p}
                </button>
              ))}
            </div>
            <small className={times?.problem ? 'ca-form-error' : undefined}>
              {times?.problem ??
                (times && times.next.length > 0
                  ? `${times.words}. Next: ${times.next.map((t) => new Date(t).toLocaleString(undefined, { weekday: 'short', month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })).join(', ')}.`
                  : 'Weekdays 9:00, mon,thu 10:30, daily 6pm, every 2h, every 30m weekdays 09:00-18:00, monthly 1 9:00, once 2026-10-21 15:00, or cron 0 9 * * 1-5. On this Mac’s clock.')}
            </small>
          </div>
        )}
        {when.kind === 'watch' && (
          <div className="ca-field">
            <BoxText aria-label="The command it runs" lines={3} hint="gh api 'repos/acme/web/deployments?environment=production' --jq '.[0:5][] | {id, title: .sha}' -c" value={when.command} spellCheck={false} onChange={(e) => setWhen({ ...when, command: e.target.value })} />
            <div className="ca-scheddlg-row">
              <label className="ca-scheddlg-inline">
                <span>Every</span>
                <input aria-label="How often it looks" value={when.every} spellCheck={false} onChange={(e) => setWhen({ ...when, every: e.target.value })} />
              </label>
              <Seg
                label="What its output is"
                options={[
                  { id: 'lines' as const, label: 'Each new line', title: 'Each line is one thing (a deploy, a message), run for once' },
                  { id: 'value' as const, label: 'Any change', title: 'Its whole output is one value, run for whenever it changes' },
                ]}
                value={when.mode}
                onPick={(mode) => setWhen({ ...when, mode })}
              />
            </div>
            <small>Runs through your login shell in its worktree. Its first look only notes what’s there. A line that’s JSON with an id is that thing, with its title, url and text.</small>
          </div>
        )}
        {when.kind === 'event' && (
          <div className="ca-field">
            <Seg label="What happens" options={EVENTS} value={when.event} onPick={(event) => setWhen({ ...when, event })} />
            {when.event === 'todo-ready' ? (
              <Select label="Which todos" value={when.label} options={labels} onPick={(label) => setWhen({ ...when, label })} />
            ) : (
              <div className="ca-scheddlg-row">
                <Select label="In which repo" value={when.repo} options={[{ id: '', label: 'In any repo' }, ...repos.map((r) => ({ id: r, label: r }))]} onPick={(repo) => setWhen({ ...when, repo })} />
                <label className="ca-check">
                  <CheckBox checked={when.mine} onChange={(mine) => setWhen({ ...when, mine })} />
                  <span>Only mine</span>
                </label>
              </div>
            )}
            <small>{when.event === 'todo-ready' ? 'A new session is linked to the todo and told it.' : 'Only what happens after it’s made; what’s there already isn’t run for.'}</small>
          </div>
        )}
        {when.kind === 'fired' && (
          <div className="ca-field">
            <code className="ca-scheddlg-code">{fireCommand(draft.name.trim() || 'name')}</code>
            <small>From the last step of a deploy script, a git hook, any program. With a key it runs once for each thing, however often it’s fired.</small>
          </div>
        )}

        <div className="ca-field">
          <span>Does</span>
          <Seg label="Does" options={TOS} value={run.to} onPick={(to) => setRun({ to })} />
        </div>
        {run.to === 'session' ? (
          <>
            <Select label="The session it sends to" value={run.session} options={[{ id: '', label: 'Pick a session' }, ...targets]} onPick={(session) => setRun({ session })} />
            {/* A watch and a check run in a worktree, though what it sends goes to a session. */}
            {(when.kind === 'watch' || draft.onlyIf.trim() !== '') && (
              <Select label="Where its commands run" value={run.folder} options={[{ id: '', label: 'Its commands run in your home folder' }, ...folders.map((f) => ({ ...f, label: `Its commands run in ${f.label}` }))]} onPick={(folder) => setRun({ folder })} />
            )}
            <BoxText aria-label="What it sends" lines={6} hint="What it sends each time" value={run.prompt} onChange={(e) => setRun({ prompt: e.target.value })} />
          </>
        ) : (
          <>
            <div className="ca-scheddlg-row">
              <Select label="Where it starts" value={run.folder} options={folderOptions} onPick={(folder) => setRun({ folder })} />
              <Select label="Where it runs" value={(run.place ?? '') as StartPlace | ''} options={PLACES} onPick={(p) => setRun({ place: p || null })} />
            </div>
            <AgentPromptBox
              accounts={accounts}
              groups={groups}
              lanes={board.lanes}
              usage={board.usage}
              now={now}
              account={run.account}
              model={run.model ?? ''}
              effort={run.effort ?? ''}
              prompt={run.prompt}
              defaultPrompt=""
              label="What it's told each run"
              hint="What it's told each run"
              autoTitle="The account the quick panel’s Auto would start, picked again at each run: an account with room (or from Auto’s routing group), the one whose week resets soonest."
              onAgent={(account, model, effort) => setRun({ account, model: model || null, effort: effort || null })}
              onPrompt={(prompt) => setRun({ prompt })}
            />
            <div className="ca-scheddlg-checks">
              <label className="ca-check">
                <CheckBox checked={run.worktree} onChange={(worktree) => setRun({ worktree })} />
                <span>A new worktree each time</span>
              </label>
              {run.to === 'new' && (
                <label className="ca-check">
                  <CheckBox checked={draft.overlap === 'allow'} onChange={(on) => setDraft({ ...draft, overlap: on ? 'allow' : 'skip' })} />
                  <span>Start another while the last one still works</span>
                </label>
              )}
            </div>
          </>
        )}

        <div className="ca-field">
          <span>Options</span>
          <div className="ca-scheddlg-checks">
            <label className="ca-check" title="Its sessions don’t wait on you when they end their turn: only when they need you">
              <CheckBox checked={draft.quiet} onChange={(quiet) => setDraft({ ...draft, quiet })} />
              <span>Quiet</span>
            </label>
            <label className="ca-check" title="Each run is told of a notes file to read first and add to, so a new session still knows what the last ones found">
              <CheckBox checked={draft.notes} onChange={(notes) => setDraft({ ...draft, notes })} />
              <span>Keep notes between runs</span>
            </label>
            {when.kind === 'time' && (
              <label className="ca-check" title="A time missed while the Mac slept or Codealong was closed runs once, late, when it’s back">
                <CheckBox checked={draft.catchUp === 'once'} onChange={(on) => setDraft({ ...draft, catchUp: on ? 'once' : 'skip' })} />
                <span>Run a missed time when back</span>
              </label>
            )}
          </div>
          <label className="ca-scheddlg-inline">
            <span>Only if</span>
            <input aria-label="Only if this command succeeds and prints something" placeholder="A command; empty for always" value={draft.onlyIf} spellCheck={false} onChange={(e) => setDraft({ ...draft, onlyIf: e.target.value })} />
          </label>
          <label className="ca-scheddlg-inline">
            <span>At most</span>
            <input aria-label="Runs a day, at most" className="ca-scheddlg-num" inputMode="numeric" placeholder="any" value={draft.perDay ?? ''} onChange={(e) => setDraft({ ...draft, perDay: /^\d+$/.test(e.target.value) ? Number(e.target.value) : null })} />
            <span>runs a day</span>
          </label>
        </div>
      </div>
    </Dialog>
  );
}
