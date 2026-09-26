// The canvas's two panels above its toolbar. Bring in says what comes onto the canvas by itself:
// which kinds of things, and how far back. Tidy lays the canvas out again, taking off what's
// older than that, kept or not (unarchived sessions pile up; this is where they leave), and
// says first what would come off and on. Clear canvas starts it over.

import { useMemo, useState } from 'react';
import { agoPhrase } from '../format';
import { PR_SCOPES, recentRefs, SESSION_AGES, TODO_AGES, tidyPlan, unclear, type CanvasDoc, type Entities, type Include, type RefItem, type RefKind, type Scope, type TidyMode, type TidyPlan } from '../canvas/model';
import { Icon } from './primitives';

const LAYOUTS: { id: TidyMode; label: string; title: string }[] = [
  { id: 'worktree', label: 'By worktree', title: 'A frame per repo, a column per worktree' },
  { id: 'time', label: 'By day', title: 'A frame per day of last activity, a column per worktree' },
];

const WORD: Record<RefKind, [string, string]> = { session: ['session', 'sessions'], todo: ['todo', 'todos'], worktree: ['worktree', 'worktrees'], pr: ['PR', 'PRs'] };

function counted(items: RefItem[]): string {
  const n: Partial<Record<RefKind, number>> = {};
  for (const it of items) n[it.ref] = (n[it.ref] ?? 0) + 1;
  const parts = (['session', 'todo', 'worktree', 'pr'] as RefKind[]).filter((k) => n[k]).map((k) => `${n[k]} ${WORD[k][n[k] === 1 ? 0 : 1]}`);
  return parts.length <= 1 ? (parts[0] ?? '') : `${parts.slice(0, -1).join(', ')} and ${parts[parts.length - 1]}`;
}

/** What a tidy did or would do, in a sentence. */
export function tidySummary(plan: TidyPlan): string {
  const off = counted(plan.dropped);
  const on = counted(plan.added);
  if (!off && !on) return 'Nothing comes off or on; the cards are laid out again.';
  return [off && `Takes off ${off}`, on && `brings on ${on}`].filter(Boolean).join('; ').replace(/^b/, 'B') + '.';
}

function Seg<T extends string>({ options, value, onPick, label }: { options: { id: T; label: string; title: string }[]; value: T; onPick: (v: T) => void; label: string }) {
  return (
    <div className="ca-segctl" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button key={o.id} type="button" role="radio" aria-checked={value === o.id} className={value === o.id ? 'is-on' : undefined} title={o.title} onClick={() => onPick(o.id)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

interface TidyPanelProps {
  doc: CanvasDoc;
  entities: Entities;
  now: number;
  pinned: Set<string>;
  onTidy: (scope: Scope, plan: TidyPlan) => void;
  /** Takes everything off; only what moves on afterwards comes back. */
  onClear: () => void;
  onClose: () => void;
}

interface TidyPanelProps {
  doc: CanvasDoc;
  entities: Entities;
  now: number;
  pinned: Set<string>;
  onTidy: (scope: Scope, plan: TidyPlan) => void;
  /** Takes everything off; only what moves on afterwards comes back. */
  onClear: () => void;
  onClose: () => void;
}

export function TidyPanel({ doc, entities, now, pinned, onTidy, onClear, onClose }: TidyPanelProps) {
  const [scope, setScope] = useState<Scope>(doc.scope);
  const plan = useMemo(() => tidyPlan(doc, entities, now, pinned, scope), [doc, entities, now, pinned, scope]);
  return (
    <div className="ca-cv-tidy" role="dialog" aria-label="Tidy">
      <div className="ca-cv-tidy-grid">
        <span className="ca-label">Layout</span>
        <Seg label="Layout" options={LAYOUTS} value={scope.layout} onPick={(layout) => setScope((s) => ({ ...s, layout }))} />
      </div>
      {scope.clearedAt != null && (
        <p className="ca-cv-tidy-cleared">
          <span>Cleared {agoPhrase(scope.clearedAt, now)}: only what’s moved on since comes on.</span>
          <button type="button" className="ca-linkbtn" onClick={() => setScope(unclear)}>
            Include older work
          </button>
        </p>
      )}
      <p className="ca-cv-tidy-sum">{tidySummary(plan)}</p>
      <p className="ca-cv-tidy-hint">What’s on the canvas is what Bring in says; cards older than that come off, even ones you moved. Frames you drew keep what’s in them.</p>
      <div className="ca-cv-tidy-actions">
        <button
          type="button"
          className="ca-btn ca-btn--ghost ca-btn--sm ca-cv-tidy-clear"
          title="Take everything off the canvas, notes and frames too. Sessions and todos come back as they move on: a new prompt, a finished turn, a change."
          onClick={onClear}
        >
          Clear canvas
        </button>
        <button type="button" className="ca-btn ca-btn--ghost ca-btn--sm" onClick={onClose}>
          Cancel
        </button>
        <button type="button" className="ca-btn ca-btn--primary ca-btn--sm" autoFocus onClick={() => onTidy(scope, plan)}>
          Tidy
        </button>
      </div>
    </div>
  );
}

function Check({ label, on, onChange, title }: { label: string; on: boolean; onChange: (on: boolean) => void; title: string }) {
  return (
    <label className="ca-check" title={title}>
      <input type="checkbox" checked={on} onChange={(e) => onChange(e.target.checked)} />
      <span className="ca-check-box" aria-hidden="true">
        <Icon name="check" size={11} stroke={2.5} />
      </span>
      {label}
    </label>
  );
}

interface BringInPanelProps {
  doc: CanvasDoc;
  entities: Entities;
  now: number;
  pinned: Set<string>;
  /** Changes take effect at once. */
  onChange: (scope: Scope) => void;
}

/** What comes onto the canvas by itself: each kind on or off, and how far back. */
export function BringInPanel({ doc, entities, now, pinned, onChange }: BringInPanelProps) {
  const scope = doc.scope;
  const inc = scope.include;
  const set = (patch: Partial<Include>) => onChange({ ...scope, include: { ...inc, ...patch } });
  const now_ = useMemo(() => {
    const on = recentRefs(entities, now, pinned, scope).map((r): RefItem => ({ kind: 'ref', ref: r.ref, key: r.key, x: 0, y: 0 }));
    return counted(on);
  }, [entities, now, pinned, scope]);
  return (
    <div className="ca-cv-tidy ca-cv-bring" role="dialog" aria-label="Bring in">
      <div className="ca-cv-tidy-head">Bring in by themselves</div>
      <div className="ca-cv-tidy-grid">
        <Check label="Sessions" on={inc.sessions} onChange={(sessions) => set({ sessions })} title="Sessions active in the time chosen, and any that's working or needs you" />
        {inc.sessions ? <Seg label="Sessions active in" options={SESSION_AGES} value={scope.sessions} onPick={(sessions) => onChange({ ...scope, sessions })} /> : <span />}
        <Check label="Todos" on={inc.todos} onChange={(todos) => set({ todos })} title="Open todos changed or worked on in the time chosen" />
        {inc.todos ? <Seg label="Todos" options={TODO_AGES} value={scope.todos} onPick={(todos) => onChange({ ...scope, todos })} /> : <span />}
        <Check label="Worktrees" on={inc.worktrees} onChange={(worktrees) => set({ worktrees })} title="Recent worktrees even with nothing of theirs on the canvas" />
        <span className="ca-cv-tidy-note">{inc.worktrees ? 'Recent ones, even with nothing on them' : 'Only above their sessions, todos and PRs'}</span>
        <span className="ca-cv-tidy-rowlabel">PRs</span>
        <Seg label="PRs" options={PR_SCOPES} value={inc.prs} onPick={(prs) => set({ prs })} />
      </div>
      <p className="ca-cv-tidy-sum">{now_ ? `Brings in ${now_} as it stands.` : 'Nothing comes on by itself.'}</p>
      <p className="ca-cv-tidy-hint">New work lands beside its worktree. Cards you moved or added stay; Tidy takes off older ones too.</p>
    </div>
  );
}
