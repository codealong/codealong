// New session: start an agent in a worktree, in the terminal or in the Claude or Codex app. You
// pick the account, or Auto. Codealong only launches it: the harness runs it, and it shows on
// the board like any other session.

import { useEffect, useMemo, useState } from 'react';
import { startOptions } from '../api';
import { middleTruncate, resetsPhrase, tildePath } from '../format';
import { accountFor, autoHarness, autoPicks, choices, initialChoice, initialPlace, placesFor, remember, remembered, type AutoPick, type Choice } from '../start';
import type { Harness, Lane, StartOptions, StartPlace, StartSpec, Usage } from '../types';
import { Button, Icon } from './primitives';
import { UsageMeter } from './UsageBar';

const HARNESS_WORD: Record<Harness, string> = { claude: 'Claude Code', codex: 'Codex' };

/** Pinned worktrees first, in your order, then the rest grouped by repo, the most recently
 *  active repo first. In each repo its main checkout leads, then the most recently active
 *  worktrees: the main checkout can be on any branch, so its place is what finds it. */
function laneGroups(lanes: Lane[], pinned: string[]): { label: string; lanes: Lane[] }[] {
  const byId = new Map(lanes.map((l) => [l.id, l]));
  const pins = pinned.flatMap((id) => byId.get(id) ?? []);
  const pinnedIds = new Set(pins.map((l) => l.id));
  const projects = new Set(lanes.map((l) => l.project)).size;
  const groups = new Map<string, Lane[]>();
  for (const l of lanes) {
    if (pinnedIds.has(l.id)) continue;
    const label = projects > 1 ? `${l.project} · ${l.repo}` : l.repo;
    groups.set(label, [...(groups.get(label) ?? []), l]);
  }
  const latest = (ls: Lane[]) => Math.max(...ls.map((l) => l.lastActivity));
  const repos = [...groups.entries()]
    .map(([label, ls]) => ({ label, lanes: [...ls].sort((a, b) => Number(b.origin === 'main') - Number(a.origin === 'main') || b.lastActivity - a.lastActivity) }))
    .sort((a, b) => latest(b.lanes) - latest(a.lanes));
  return pins.length > 0 ? [{ label: 'Pinned', lanes: pins }, ...repos] : repos;
}

/** Why Auto starts the account it does, after the harness's name. */
function autoReason(p: AutoPick, now: number): string {
  if (p.reason === 'reset' && p.resetsAt) return `${p.account}'s week resets ${resetsPhrase(p.resetsAt, now)}`;
  return p.reason === 'room' ? `${p.account} has the most room` : `starts ${p.account}`;
}

function laneLabel(l: Lane): string {
  const name = l.origin === 'main' ? `${l.callsign} (main checkout)` : l.callsign;
  const branch = l.branch ?? (l.detached ? `detached @ ${l.detached}` : null);
  return branch && branch !== l.callsign ? `${name} · ${middleTruncate(branch, 44)}` : name;
}

/** Why an account shows no meters. */
function usageNote(u: Usage | undefined): string {
  if (!u) return 'No usage yet';
  if (u.needsSignIn) return 'Not signed in';
  if (u.loading) return 'Reading…';
  return u.harness === 'codex' ? 'No turns yet' : 'Usage unavailable';
}

/** What you chose and wrote, kept so a failed start can bring the sheet back as you left it. */
export interface SheetDraft {
  choice: Choice | null;
  place: StartPlace | null;
  prompt: string;
  model: string | null;
  worktree: boolean;
}

interface NewSessionSheetProps {
  /** Where a session can start, the preselected one among them. */
  lanes: Lane[];
  laneId: string | null;
  /** After a failed start: what you had chosen and written, and why it failed. */
  draft?: SheetDraft;
  error?: string;
  /** Pinned worktree ids, in your order: they head the list. */
  pinned: string[];
  usage: Usage[];
  now: number;
  onClose: () => void;
  /** Start: the sheet's owner closes it, runs the start and shows `label` while it waits. */
  onStart: (spec: StartSpec, draft: SheetDraft, label: string) => void;
}

export function NewSessionSheet({ lanes, laneId: firstLane, draft, error: failed, pinned, usage, now, onClose, onStart }: NewSessionSheetProps) {
  const groups = useMemo(() => laneGroups(lanes, pinned), [lanes, pinned]);
  const [options, setOptions] = useState<StartOptions | null>(null);
  // Without a selected worktree, the first on the list: a pinned one, else the latest.
  const [laneId, setLaneId] = useState(firstLane ?? groups[0]?.lanes[0]?.id ?? '');
  const [choice, setChoice] = useState<Choice | null>(null);
  const [place, setPlace] = useState<StartPlace | null>(null);
  const [prompt, setPrompt] = useState(draft?.prompt ?? '');
  // A model for a terminal start; null is the account's own default. Not remembered: the
  // default stays the default.
  const [model, setModel] = useState<string | null>(draft?.model ?? null);
  // A new worktree for a terminal start: Claude Code makes it (its folder, branch and base are
  // its own business). The apps offer their own worktree choice on their new-session pages.
  const [newWorktree, setNewWorktree] = useState(draft?.worktree ?? false);
  const [error, setError] = useState<string | null>(failed ?? null);

  const accounts = useMemo(() => options?.accounts ?? [], [options]);
  // Auto follows usage while the sheet is open, and resolves when you press Start.
  const picks = useMemo(() => autoPicks(accounts, usage, now), [accounts, usage, now]);
  const lane = lanes.find((l) => l.id === laneId) ?? null;
  const places = placesFor(choice, accounts);
  const account = accountFor(choice, picks);

  useEffect(() => {
    startOptions()
      .then((o) => {
        setOptions(o);
        const last = remembered();
        const c = initialChoice(o.accounts, autoPicks(o.accounts, usage, now), draft?.choice ?? last.choice);
        setChoice(c);
        setPlace(initialPlace(c, o.accounts, draft?.place ?? (c ? last.places?.[c] : undefined)));
      })
      .catch((e: unknown) => setError(String(e).replace(/^Error: /, '')));
  }, []);

  const choose = (c: Choice) => {
    setChoice(c);
    setPlace(initialPlace(c, accounts, remembered().places?.[c]));
  };

  // The apps' links take no model, so only a terminal start asks for one.
  const chosenModel = place === 'terminal' ? model : null;
  const inNewWorktree = place === 'terminal' && newWorktree;
  const modelLabel = options?.claudeModels.find((m) => m.id === chosenModel)?.label.split(':')[0] ?? null;
  const defaultModel = accounts.find((a) => a.name === account)?.defaultModel ?? null;

  const ready = !!lane && !!account && !!place;
  const submit = () => {
    if (!lane || !account || !place) return;
    if (choice) remember(choice, place);
    const label =
      place === 'terminal'
        ? `Starting ${account}${modelLabel ? ` on ${modelLabel}` : ''} in ${inNewWorktree ? `a new worktree of ${lane.repo}` : lane.callsign}…`
        : place === 'claude-app'
          ? `Opening a new session in the Claude app…`
          : `Opening a new thread in Codex…`;
    onStart({ laneId: lane.id, account, place, prompt, model: chosenModel, worktree: inNewWorktree }, { choice, place, prompt, model, worktree: newWorktree }, label);
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
        submit();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  const rows = choices(accounts, picks).map((c) => {
    const h = autoHarness(c);
    const p = h ? picks[h] : undefined;
    if (h && p) {
      return {
        id: c,
        auto: true,
        name: 'Auto',
        detail: `${HARNESS_WORD[h]} · ${autoReason(p, now)}`,
        usage: undefined,
        disabled: false,
        title:
          `Starts the ${HARNESS_WORD[h]} account whose weekly limit resets soonest within 48 hours, so its room isn't lost, ` +
          `else the one whose fullest limit is least full. Accounts at 90% of a limit are passed over while another has room. ` +
          `Decided when you press Start; right now it's ${p.account}.`,
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
  const onModel = modelLabel ? ` on ${modelLabel}` : '';
  const withPrompt = prompted ? ' with this prompt' : '';
  const happens =
    place === 'terminal'
      ? inNewWorktree && lane
        ? `${placeLabel} opens a new window, and Claude Code makes a new worktree of ${lane.repo} and starts ${account ?? 'the account'}${onModel} there${withPrompt}.`
        : `${placeLabel} opens a new window in ${where} and starts ${account ?? 'the account'}${onModel}${withPrompt}.`
      : place === 'claude-app'
        ? `The Claude app opens a new session in ${where}${prompted ? ' with this prompt filled in' : ''}. You send it from there, and can choose a new worktree there too.`
        : place === 'codex-app'
          ? `Codex opens a new thread in ${where}${prompted ? ' with this prompt filled in' : ''}. You send it from there, and can choose a new worktree there too.`
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
          submit();
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
          <div className="ca-field">
            <span id="ca-start-lane">Worktree</span>
            <div className="ca-start-lane">
              <span className="ca-start-select">
                <select aria-labelledby="ca-start-lane" value={laneId} onChange={(e) => setLaneId(e.target.value)} disabled={lanes.length === 0}>
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
              <label
                className={['ca-check', place !== 'terminal' && 'is-disabled'].filter(Boolean).join(' ')}
                title={place === 'terminal' ? 'Claude Code makes the worktree: its folder, branch and base.' : 'The app has its own worktree choice; pick it there before you send.'}
              >
                <input type="checkbox" checked={inNewWorktree} disabled={place !== 'terminal'} onChange={(e) => setNewWorktree(e.target.checked)} />
                <span className="ca-check-box" aria-hidden="true">
                  <Icon name="check" size={11} stroke={2.5} />
                </span>
                New worktree
              </label>
            </div>
            {!lane ? (
              <small>No worktrees in this project yet.</small>
            ) : inNewWorktree ? (
              <small>Claude Code makes a new worktree of {lane.repo} for this session.</small>
            ) : (
              <small className="ca-code">{tildePath(lane.path)}</small>
            )}
          </div>
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
            <div className="ca-start-row">
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
              <label className="ca-field">
                <span>Model</span>
                <span className="ca-start-select">
                  {place === 'terminal' ? (
                    <select value={model ?? ''} onChange={(e) => setModel(e.target.value || null)}>
                      <option value="">{defaultModel ? `Account default (${defaultModel})` : 'Account default'}</option>
                      {(options?.claudeModels ?? []).map((m) => (
                        <option key={m.id} value={m.id}>
                          {m.label}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <select value="" disabled title="The app's new-session link can't pick a model; choose it there before you send.">
                      <option value="">{place === 'claude-app' ? 'Chosen in the Claude app' : 'Chosen in Codex'}</option>
                    </select>
                  )}
                  <Icon name="chevron-down" size={14} />
                </span>
              </label>
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
            Start
          </Button>
        </footer>
      </form>
    </div>
  );
}
