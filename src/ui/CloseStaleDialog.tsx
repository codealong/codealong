// File › Close stale terminals…: every Ghostty, iTerm2 and Terminal tab nothing was typed or
// printed in for the wait, whatever runs in it, each saying what it shows. The ones whose closing
// loses nothing (a shell at its prompt, sessions Close idle sessions would close) start ticked;
// a tab running something else, or a session that waits on you, starts unticked and says what
// closing would end. It closes only when you press the button, and the engine looks at each tab
// again first (stale.rs). Its rows are Close idle sessions' (CloseIdleDialog.css).

import { useEffect, useMemo, useState } from 'react';
import { closeStaleTerminals, staleTerminals } from '../api';
import type { StaleOutcome, StalePlan } from '../types';
import { WAITS } from './CloseIdleDialog';
import { Button, CheckBox, Dialog, Icon, Seg } from './primitives';

/** The button's words: "Close 3 tabs". */
export function closeLabel(n: number): string {
  return `Close ${plural(n, 'tab', 'tabs')}`;
}

/** What closing did, for the window's toast: "Closed 3 tabs." Empty when nothing closed. */
export function closedTabs(out: StaleOutcome[]): string {
  const closed = out.filter((o) => o.closed).length;
  return closed > 0 ? `Closed ${plural(closed, 'tab', 'tabs')}.` : '';
}

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

interface Props {
  onClose: () => void;
  /** Called with what closing did, for the window's toast. */
  onDone: (message: string) => void;
}

export function CloseStaleDialog({ onClose, onDone }: Props) {
  const [after, setAfter] = useState(WAITS[1].ms);
  const [plan, setPlan] = useState<StalePlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [picked, setPicked] = useState<Set<string>>(() => new Set());
  const [closing, setClosing] = useState(false);
  const [failed, setFailed] = useState<StaleOutcome[] | null>(null);

  // Each wait asks again, and only the latest answer counts: an earlier one coming in after it
  // would put back its tabs and the ticks you'd taken off.
  useEffect(() => {
    let latest = true;
    setPlan(null);
    setError(null);
    staleTerminals(after)
      .then((p) => {
        if (!latest) return;
        setPlan(p);
        setPicked(new Set(p.tabs.filter((t) => t.ticked).map((t) => t.key)));
      })
      .catch((e: unknown) => {
        if (latest) setError(String(e));
      });
    return () => {
      latest = false;
    };
  }, [after]);

  const chosen = useMemo(() => (plan?.tabs ?? []).filter((t) => picked.has(t.key)), [plan, picked]);
  const toggle = (key: string) =>
    setPicked((s) => {
      const next = new Set(s);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  const run = async () => {
    setClosing(true);
    try {
      const out = await closeStaleTerminals(chosen.map((t) => t.key), after);
      const said = closedTabs(out);
      const notDone = out.filter((o) => !o.closed);
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

  const titleOf = (key: string) => plan?.tabs.find((t) => t.key === key)?.title || key;
  let body: React.ReactNode;
  if (failed) {
    body = (
      <>
        <p className="ca-idle-note">Not closed: {plural(failed.length, 'tab', 'tabs')}.</p>
        <ul className="ca-idle-list">
          {failed.map((o) => (
            <li key={o.key} className="ca-idle-row is-static">
              <Icon name="alert" size={14} />
              <span className="ca-idle-text">
                <span className="ca-idle-title">{o.title || titleOf(o.key)}</span>
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
        {plan.tabs.length === 0 ? (
          <p className="ca-empty">No terminal tab has gone unused that long.</p>
        ) : (
          <ul className="ca-idle-list" aria-label="Tabs to close">
            {plan.tabs.map((t) => (
              <li key={t.key}>
                <label className="ca-idle-row" title={t.cwd}>
                  <CheckBox checked={picked.has(t.key)} onChange={() => toggle(t.key)} />
                  <span className="ca-idle-text">
                    <span className="ca-idle-title">{t.title}</span>
                    <span className="ca-idle-why">{t.why}</span>
                  </span>
                  <span className="ca-idle-does">{t.terminal}</span>
                </label>
              </li>
            ))}
          </ul>
        )}
        {plan.fresh > 0 && <p className="ca-idle-note">{plan.fresh === 1 ? '1 other tab was' : `${plan.fresh} other tabs were`} used more recently.</p>}
      </>
    );
  }

  return (
    <Dialog
      className="ca-idle"
      title="Close stale terminals"
      lede="Ghostty, iTerm2 and Terminal tabs nothing was typed or printed in for a while, whatever runs in them. Closing a tab ends what runs there; a session’s conversation stays and resumes like any other. A tab running something else, or a session that waits on you, starts unticked."
      busy={closing}
      onClose={onClose}
      aside={<span className="ca-idle-count">{!failed && plan && plan.tabs.length > 0 ? `${chosen.length} of ${plan.tabs.length} ticked` : ''}</span>}
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
              {closing ? 'Closing…' : closeLabel(chosen.length)}
            </Button>
          </>
        )
      }
    >
      {!failed && (
        <div className="ca-idle-wait">
          <span>Unused for at least</span>
          <Seg label="Unused for at least" options={WAITS.map((w) => ({ id: w.ms, label: w.label }))} value={after} disabled={closing} onPick={setAfter} />
        </div>
      )}
      <div className="ca-dialog-body">{body}</div>
    </Dialog>
  );
}
