// File › Close idle sessions…: the sessions that sat idle and want nothing from you, each with
// why and what closing does to it, ticked to close. A Ghostty, iTerm2 or Terminal tab closes,
// ending the agent's process; a background job is stopped. With Also archive them, each is
// archived too, as Mark as done does, and the ones Codealong can't close (an app's, one it runs
// itself, one in tmux) are listed to be archived only. It says first what would happen and does
// it only when you press the button; the engine checks each one again first (idle.rs). The
// conversations stay to resume.

import { useCallback, useEffect, useMemo, useState } from 'react';
import { closeIdleSessions, idleSessions } from '../api';
import { flag, useStored } from '../stored';
import type { CloseOutcome, IdleEnds, IdlePlan, IdleSession, KeptSession } from '../types';
import { Button, cx, Dialog, Icon, Seg } from './primitives';

const HOUR = 3600e3;
export const WAITS: { ms: number; label: string }[] = [
  { ms: 12 * HOUR, label: '12 hours' },
  { ms: 24 * HOUR, label: '1 day' },
  { ms: 72 * HOUR, label: '3 days' },
  { ms: 168 * HOUR, label: '1 week' },
];

/** Whether Also archive them was ticked when the dialog was last used. */
const ARCHIVE_KEY = 'codealong.closeIdle.archive';

/** What closing does to a row, as it says beside it. */
const DOES: Record<IdleEnds, string> = { terminal: 'Closes its tab', job: 'Stops its job', archive: 'Only archived' };

/** The sessions that stay open, counted by reason: "3 working, 2 active recently". */
export function keptSummary(kept: KeptSession[]): string {
  const counts = new Map<string, number>();
  for (const k of kept) {
    const why = k.why.startsWith('active ') ? 'active recently' : k.why;
    counts.set(why, (counts.get(why) ?? 0) + 1);
  }
  return [...counts].map(([why, n]) => `${n} ${why}`).join(', ');
}

/** What the button does to the ticked rows: "Close 3 sessions", "Close and archive 3 sessions",
 *  "Archive 1 session" when nothing ticked can close. */
export function actionLabel(chosen: IdleSession[], archive: boolean): string {
  const n = plural(chosen.length, 'session', 'sessions');
  if (!archive) return `Close ${n}`;
  return chosen.length > 0 && chosen.every((c) => c.ends === 'archive') ? `Archive ${n}` : `Close and archive ${n}`;
}

/** What closing did, for the window's toast: "Closed 3 sessions, stopped 1 background job and
 *  archived 4." Empty when nothing was done. */
export function closedSummary(out: CloseOutcome[]): string {
  const closed = out.filter((o) => o.ends === 'terminal' && o.closed).length;
  const stopped = out.filter((o) => o.ends === 'job' && o.closed).length;
  const archived = out.filter((o) => o.archived).length;
  const parts = [
    closed > 0 && `closed ${plural(closed, 'session', 'sessions')}`,
    stopped > 0 && `stopped ${plural(stopped, 'background job', 'background jobs')}`,
    archived > 0 && (closed + stopped > 0 ? `archived ${archived}` : `archived ${plural(archived, 'session', 'sessions')}`),
  ].filter((p): p is string => !!p);
  if (parts.length === 0) return '';
  const said = parts.length > 1 ? `${parts.slice(0, -1).join(', ')} and ${parts[parts.length - 1]}` : parts[0];
  return `${said[0].toUpperCase()}${said.slice(1)}.`;
}

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

interface Props {
  onClose: () => void;
  /** Called with what closing did, for the window's toast. */
  onDone: (message: string) => void;
}

export function CloseIdleDialog({ onClose, onDone }: Props) {
  const [after, setAfter] = useState(24 * HOUR);
  const [archive, setArchive] = useStored(ARCHIVE_KEY, flag, false);
  const [plan, setPlan] = useState<IdlePlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [off, setOff] = useState<Set<string>>(() => new Set());
  const [closing, setClosing] = useState(false);
  const [failed, setFailed] = useState<CloseOutcome[] | null>(null);

  const load = useCallback((ms: number, archived: boolean) => {
    setPlan(null);
    setError(null);
    idleSessions(ms, archived)
      .then(setPlan)
      .catch((e: unknown) => setError(String(e)));
  }, []);
  useEffect(() => load(after, archive), [after, archive, load]);

  const chosen = useMemo(() => (plan?.close ?? []).filter((c) => !off.has(c.key)), [plan, off]);
  const toggle = (key: string) =>
    setOff((s) => {
      const next = new Set(s);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  const run = async () => {
    setClosing(true);
    try {
      const out = await closeIdleSessions(chosen.map((c) => c.key), after, archive);
      const said = closedSummary(out);
      const notDone = out.filter((o) => !o.ok);
      if (notDone.length === 0) {
        onDone(said);
        onClose();
        return;
      }
      setFailed(notDone);
      if (said) onDone(said);
    } catch (e) {
      setError(String(e));
    } finally {
      setClosing(false);
    }
  };

  let body: React.ReactNode;
  if (failed) {
    body = (
      <>
        <p className="ca-idle-note">Not done for {plural(failed.length, 'session', 'sessions')}:</p>
        <ul className="ca-idle-list">
          {failed.map((o) => (
            <li key={o.key} className="ca-idle-row is-static">
              <Icon name="alert" size={14} />
              <span className="ca-idle-text">
                <span className="ca-idle-title">{o.title || o.key}</span>
                <span className="ca-idle-why">{o.message}</span>
              </span>
            </li>
          ))}
        </ul>
      </>
    );
  } else if (error) {
    body = (
      <p className="ca-form-error" role="alert">
        {error}
      </p>
    );
  } else if (!plan) {
    body = <p className="ca-empty">Looking at your sessions…</p>;
  } else {
    body = (
      <>
        {plan.close.length === 0 ? (
          <p className="ca-empty">No session has sat idle that long with nothing asked of you.</p>
        ) : (
          <ul className="ca-idle-list" aria-label="Sessions to close">
            {plan.close.map((c) => (
              <li key={c.key}>
                <label className="ca-idle-row" title={c.cwd}>
                  <span className="ca-check">
                    <input type="checkbox" checked={!off.has(c.key)} onChange={() => toggle(c.key)} />
                    <span className="ca-check-box" aria-hidden="true">
                      <Icon name="check" size={11} stroke={2.5} />
                    </span>
                  </span>
                  <span className="ca-idle-text">
                    <span className="ca-idle-title">{c.title || c.key}</span>
                    <span className="ca-idle-why">
                      {c.account} in {c.callsign} · {c.why}
                    </span>
                  </span>
                  <span className="ca-idle-does">{DOES[c.ends]}</span>
                </label>
              </li>
            ))}
          </ul>
        )}
        {plan.kept.length > 0 && <p className="ca-idle-note">Stay open: {keptSummary(plan.kept)}.</p>}
      </>
    );
  }

  return (
    <Dialog
      className="ca-idle"
      title="Close idle sessions"
      lede="Sessions that are idle and whose last reply asks you nothing. A Ghostty, iTerm2 or Terminal tab closes, which ends the agent’s process, and a background job is stopped; the conversation stays and resumes like any other."
      busy={closing}
      onClose={onClose}
      aside={<span className="ca-idle-count">{!failed && plan && plan.close.length > 0 ? `${chosen.length} of ${plan.close.length} ticked` : ''}</span>}
      foot={
        failed ? (
          <Button variant="primary" size="sm" onClick={onClose}>
            Done
          </Button>
        ) : (
          <>
            <Button variant="ghost" size="sm" onClick={onClose} disabled={closing}>
              Cancel
            </Button>
            <Button variant="primary" size="sm" onClick={() => void run()} disabled={closing || !plan || chosen.length === 0}>
              {closing ? 'Closing…' : actionLabel(chosen, archive)}
            </Button>
          </>
        )
      }
    >
      {!failed && (
        <div className="ca-idle-wait">
          <span>Idle for at least</span>
          <Seg label="Idle for at least" options={WAITS.map((w) => ({ id: w.ms, label: w.label }))} value={after} disabled={closing} onPick={setAfter} />
          <label className={cx('ca-check', 'ca-idle-archive', closing && 'is-disabled')} title="Each is marked seen and archived too, as Mark as done does, so it leaves every list but Archived. Sessions Codealong can’t close (in the Claude or Codex app, run by Codealong, in tmux) are archived only and stay open where they are.">
            <input type="checkbox" checked={archive} disabled={closing} onChange={(e) => setArchive(e.target.checked)} />
            <span className="ca-check-box" aria-hidden="true">
              <Icon name="check" size={11} stroke={2.5} />
            </span>
            Also archive them
          </label>
        </div>
      )}
      <div className="ca-dialog-body">{body}</div>
    </Dialog>
  );
}
