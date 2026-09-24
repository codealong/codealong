// New session: start an agent in a worktree, in the terminal or in the Claude or Codex app. You
// pick the account, or Auto. Codealong only launches it: the harness runs it, and it shows on
// the board like any other session.

import { useEffect, useMemo, useState } from 'react';
import { startOptions, startSession } from '../api';
import { middleTruncate, tildePath } from '../format';
import { accountFor, autoHarness, autoPicks, choices, fullest, initialChoice, initialPlace, placesFor, remember, remembered, type Choice } from '../start';
import type { Harness, Lane, StartOptions, StartPlace, Started, Usage } from '../types';
import { Button, Icon } from './primitives';
import { UsageMeter } from './UsageBar';

const HARNESS_WORD: Record<Harness, string> = { claude: 'Claude Code', codex: 'Codex' };

/** Worktrees grouped by repo, the most recently active repo and worktree first. */
function laneGroups(lanes: Lane[]): { label: string; lanes: Lane[] }[] {
  const projects = new Set(lanes.map((l) => l.project)).size;
  const groups = new Map<string, Lane[]>();
  for (const l of lanes) {
    const label = projects > 1 ? `${l.project} · ${l.repo}` : l.repo;
    groups.set(label, [...(groups.get(label) ?? []), l]);
  }
  const latest = (ls: Lane[]) => Math.max(...ls.map((l) => l.lastActivity));
  return [...groups.entries()]
    .map(([label, ls]) => ({ label, lanes: [...ls].sort((a, b) => b.lastActivity - a.lastActivity) }))
    .sort((a, b) => latest(b.lanes) - latest(a.lanes));
}

function laneLabel(l: Lane): string {
  const branch = l.branch ?? (l.detached ? `detached @ ${l.detached}` : null);
  return branch && branch !== l.callsign ? `${l.callsign} · ${middleTruncate(branch, 44)}` : l.callsign;
}

/** Why an account shows no meters. */
function usageNote(u: Usage | undefined): string {
  if (!u) return 'No usage yet';
  if (u.needsSignIn) return 'Not signed in';
  if (u.loading) return 'Reading…';
  return u.harness === 'codex' ? 'No turns yet' : 'Usage unavailable';
}

interface NewSessionSheetProps {
  /** Where a session can start, the preselected one among them. */
  lanes: Lane[];
  laneId: string | null;
  usage: Usage[];
  now: number;
  onClose: () => void;
  /** After the harness has it; `started.key` names the session when Codealong chose its ID. */
  onStarted: (started: Started | null, lane: Lane) => void;
}

export function NewSessionSheet({ lanes, laneId: firstLane, usage, now, onClose, onStarted }: NewSessionSheetProps) {
  const groups = useMemo(() => laneGroups(lanes), [lanes]);
  const [options, setOptions] = useState<StartOptions | null>(null);
  // Without a selected worktree, the most recently active one.
  const [laneId, setLaneId] = useState(firstLane ?? groups[0]?.lanes[0]?.id ?? '');
  const [choice, setChoice] = useState<Choice | null>(null);
  const [place, setPlace] = useState<StartPlace | null>(null);
  const [prompt, setPrompt] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const accounts = useMemo(() => options?.accounts ?? [], [options]);
  // Auto follows usage while the sheet is open, and resolves when you press Start.
  const picks = useMemo(() => autoPicks(accounts, usage), [accounts, usage]);
  const lane = lanes.find((l) => l.id === laneId) ?? null;
  const places = placesFor(choice, accounts);
  const account = accountFor(choice, picks);

  useEffect(() => {
    startOptions()
      .then((o) => {
        setOptions(o);
        const last = remembered();
        const c = initialChoice(o.accounts, autoPicks(o.accounts, usage), last.choice);
        setChoice(c);
        setPlace(initialPlace(c, o.accounts, c ? last.places?.[c] : undefined));
      })
      .catch((e: unknown) => setError(String(e).replace(/^Error: /, '')));
  }, []);

  const choose = (c: Choice) => {
    setChoice(c);
    setPlace(initialPlace(c, accounts, remembered().places?.[c]));
  };

  const ready = !!lane && !!account && !!place && !busy;
  const submit = async () => {
    if (!ready || !lane || !account || !place) return;
    setBusy(true);
    setError(null);
    try {
      const started = await startSession({ laneId: lane.id, account, place, prompt });
      if (choice) remember(choice, place);
      onStarted(started, lane);
    } catch (e) {
      setError(String(e).replace(/^Error: /, ''));
      setBusy(false);
    }
  };

  // ⌘⏎ starts and Escape closes, wherever the focus is. Re-bound each render so Start sees the
  // current choices.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      } else if (e.key === 'Enter' && e.metaKey) {
        e.preventDefault();
        void submit();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  const rows = choices(accounts, picks).map((c) => {
    const h = autoHarness(c);
    if (h) {
      const pick = picks[h] ?? '';
      const measured = fullest(usage.find((u) => u.account === pick)) !== null;
      return {
        id: c,
        auto: true,
        name: 'Auto',
        detail: `${HARNESS_WORD[h]} · ${measured ? `${pick} has the most room` : `starts ${pick}`}`,
        usage: undefined,
        disabled: false,
        title: `Starts the ${HARNESS_WORD[h]} account whose fullest limit is least full when you press Start. Right now that's ${pick}.`,
      };
    }
    const a = accounts.find((x) => x.name === c);
    const u = usage.find((x) => x.account === c);
    return {
      id: c,
      auto: false,
      name: c,
      detail: [a ? HARNESS_WORD[a.harness] : null, u?.plan].filter(Boolean).join(' · '),
      usage: u,
      disabled: !a || a.places.length === 0,
      title: a?.unavailable ?? undefined,
    };
  });

  const where = lane?.callsign ?? 'the worktree';
  const prompted = prompt.trim().length > 0;
  const placeLabel = places.find((p) => p.place === place)?.label ?? '';
  const happens =
    place === 'terminal'
      ? `${placeLabel} opens a new window in ${where} and starts ${account ?? 'the account'}${prompted ? ' on this prompt' : ''}.`
      : place === 'claude-app'
        ? `The Claude app opens a new session in ${where}${prompted ? ' with this prompt filled in' : ''}. You send it from there.`
        : place === 'codex-app'
          ? `Codex opens a new thread in ${where}${prompted ? ' with this prompt filled in' : ''}. You send it from there.`
          : '';
  const newToCodex = place === 'codex-app' && !!lane && !!options && !options.codexProjects.includes(lane.path);

  return (
    <div
      className="ca-scrim"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <form
        className="ca-dialog ca-start"
        role="dialog"
        aria-modal="true"
        aria-labelledby="ca-start-title"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
        onKeyDown={(e) => {
          // Return in the account list doesn't start; only ⌘⏎ or the Start button does.
          if (e.key === 'Enter' && !e.metaKey && e.target instanceof HTMLInputElement) e.preventDefault();
        }}
      >
        <header className="ca-dialog-head">
          <h2 id="ca-start-title">New session</h2>
          <Button variant="ghost" size="sm" icon="x" aria-label="Close" onClick={onClose} />
        </header>
        <div className="ca-dialog-body">
          <label className="ca-field">
            <span>Worktree</span>
            <span className="ca-start-select">
              <select value={laneId} onChange={(e) => setLaneId(e.target.value)} disabled={lanes.length === 0}>
                {groups.map((g) => (
                  <optgroup key={g.label} label={g.label}>
                    {g.lanes.map((l) => (
                      <option key={l.id} value={l.id}>
                        {laneLabel(l)}
                      </option>
                    ))}
                  </optgroup>
                ))}
              </select>
              <Icon name="chevron-down" size={14} />
            </span>
            <small className="ca-code">{lane ? tildePath(lane.path) : 'No worktrees in this project yet.'}</small>
          </label>
          <div className="ca-field" role="radiogroup" aria-labelledby="ca-start-account">
            <span id="ca-start-account">Account</span>
            {options === null && !error ? (
              <small>Reading accounts…</small>
            ) : rows.length === 0 ? (
              <small>No accounts yet. Add one in Accounts (⌘,).</small>
            ) : (
              <div className="ca-picks">
                {rows.map((r) => (
                  <label key={r.id} className={['ca-pick', r.auto && 'is-auto', choice === r.id && 'is-on', r.disabled && 'is-disabled'].filter(Boolean).join(' ')} title={r.title}>
                    <input type="radio" name="ca-start-account" value={r.id} checked={choice === r.id} disabled={r.disabled} onChange={() => choose(r.id)} />
                    <span className="ca-pick-name">{r.name}</span>
                    <span className="ca-pick-detail">{r.detail}</span>
                    {!r.auto && (
                      <span className="ca-pick-meters">
                        {r.disabled ? (
                          <span className="ca-pick-note">Can't start from here</span>
                        ) : r.usage && r.usage.meters.length > 0 ? (
                          r.usage.meters.map((m) => <UsageMeter key={m.key} meter={m} now={now} />)
                        ) : (
                          <span className="ca-pick-note">{usageNote(r.usage)}</span>
                        )}
                      </span>
                    )}
                  </label>
                ))}
              </div>
            )}
          </div>
          {places.length > 0 && (
            <div className="ca-field">
              <span id="ca-start-place">Opens in</span>
              <div className="ca-segctl ca-start-places" role="radiogroup" aria-labelledby="ca-start-place">
                {places.map((p) => (
                  <button key={p.place} type="button" role="radio" aria-checked={place === p.place} className={place === p.place ? 'is-on' : undefined} onClick={() => setPlace(p.place)}>
                    <Icon name={p.place === 'terminal' ? 'terminal' : 'app'} size={14} />
                    {p.label}
                  </button>
                ))}
              </div>
            </div>
          )}
          <label className="ca-field">
            <span>Prompt</span>
            <textarea value={prompt} rows={5} autoFocus spellCheck placeholder="What should it work on? You can also leave this empty and type in the session." onChange={(e) => setPrompt(e.target.value)} />
            {happens && (
              <small>
                {happens}
                {newToCodex && ' Codex adds this folder as a project if it isn’t one yet.'}
              </small>
            )}
          </label>
          {error && (
            <p className="ca-form-error" role="alert">
              {error}
            </p>
          )}
        </div>
        <footer className="ca-dialog-foot ca-start-foot">
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" disabled={!ready} kbd={['⌘', '⏎']}>
            {busy ? 'Starting…' : 'Start'}
          </Button>
        </footer>
      </form>
    </div>
  );
}
