// File › Close idle terminals…: the Ghostty, iTerm2 and Terminal tabs of sessions that sat idle
// and want nothing from you, each with why, ticked to close. It says first what would close and
// closes only when you press the button; the engine checks each one again before closing it
// (idle.rs). The agent's process ends, and its conversation stays to resume.

import { useCallback, useEffect, useMemo, useState } from 'react';
import { closeIdleTerminals, idleTerminals } from '../api';
import type { CloseOutcome, IdlePlan, KeptTerminal } from '../types';
import { Button, Dialog, Icon, Seg } from './primitives';

const HOUR = 3600e3;
export const WAITS: { ms: number; label: string }[] = [
  { ms: 12 * HOUR, label: '12 hours' },
  { ms: 24 * HOUR, label: '1 day' },
  { ms: 72 * HOUR, label: '3 days' },
  { ms: 168 * HOUR, label: '1 week' },
];

/** The terminals that stay open, counted by reason: "3 working, 2 active recently". */
export function keptSummary(kept: KeptTerminal[]): string {
  const counts = new Map<string, number>();
  for (const k of kept) {
    const why = k.why.startsWith('active ') ? 'active recently' : k.why;
    counts.set(why, (counts.get(why) ?? 0) + 1);
  }
  return [...counts].map(([why, n]) => `${n} ${why}`).join(', ');
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
  const [plan, setPlan] = useState<IdlePlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [off, setOff] = useState<Set<string>>(() => new Set());
  const [closing, setClosing] = useState(false);
  const [failed, setFailed] = useState<CloseOutcome[] | null>(null);

  const load = useCallback((ms: number) => {
    setPlan(null);
    setError(null);
    idleTerminals(ms)
      .then(setPlan)
      .catch((e: unknown) => setError(String(e)));
  }, []);
  useEffect(() => load(after), [after, load]);

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
      const out = await closeIdleTerminals(chosen.map((c) => c.key), after);
      const closed = out.filter((o) => o.closed).length;
      const notClosed = out.filter((o) => !o.closed);
      const said = `Closed ${plural(closed, 'terminal', 'terminals')}.`;
      if (notClosed.length === 0) {
        onDone(said);
        onClose();
        return;
      }
      setFailed(notClosed);
      if (closed > 0) onDone(said);
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
        <p className="ca-idle-note">{plural(failed.length, 'terminal stayed', 'terminals stayed')} open:</p>
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
    body = <p className="ca-empty">Looking at your terminals…</p>;
  } else {
    body = (
      <>
        {plan.close.length === 0 ? (
          <p className="ca-empty">No terminal has sat idle that long with nothing asked of you.</p>
        ) : (
          <ul className="ca-idle-list" aria-label="Terminals to close">
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
      title="Close idle terminals"
      lede="Terminal tabs in Ghostty, iTerm2 or Terminal whose sessions are idle and whose last reply asks you nothing. Closing ends the agent’s process; the conversation stays and resumes like any other."
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
            <Button variant="primary" size="sm" onClick={() => void run()} disabled={closing || chosen.length === 0}>
              {closing ? 'Closing…' : `Close ${plural(chosen.length, 'terminal', 'terminals')}`}
            </Button>
          </>
        )
      }
    >
      {!failed && (
        <div className="ca-idle-wait">
          <span>Idle for at least</span>
          <Seg label="Idle for at least" options={WAITS.map((w) => ({ id: w.ms, label: w.label }))} value={after} disabled={closing} onPick={setAfter} />
        </div>
      )}
      <div className="ca-dialog-body">{body}</div>
    </Dialog>
  );
}
