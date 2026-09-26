// Tidy's panel, above the canvas's toolbar: how to lay the canvas out, and how far back its
// sessions and todos go. Unarchived sessions pile up; this is where the old ones come off.
// Before you tidy, it says what would come off and what would come on.

import { useMemo, useState } from 'react';
import { agoPhrase } from '../format';
import { SESSION_AGES, TODO_AGES, tidyPlan, unclear, type CanvasDoc, type Entities, type RefItem, type RefKind, type Scope, type TidyMode, type TidyPlan } from '../canvas/model';

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

export function TidyPanel({ doc, entities, now, pinned, onTidy, onClear, onClose }: TidyPanelProps) {
  const [scope, setScope] = useState<Scope>(doc.scope);
  const plan = useMemo(() => tidyPlan(doc, entities, now, pinned, scope), [doc, entities, now, pinned, scope]);
  return (
    <div
      className="ca-cv-tidy"
      role="dialog"
      aria-label="Tidy"
    >
      <div className="ca-cv-tidy-grid">
        <span className="ca-label">Layout</span>
        <Seg label="Layout" options={LAYOUTS} value={scope.layout} onPick={(layout) => setScope((s) => ({ ...s, layout }))} />
        <span className="ca-label">Sessions</span>
        <Seg label="Sessions active in" options={SESSION_AGES} value={scope.sessions} onPick={(sessions) => setScope((s) => ({ ...s, sessions }))} />
        <span className="ca-label">Todos</span>
        <Seg label="Todos" options={TODO_AGES} value={scope.todos} onPick={(todos) => setScope((s) => ({ ...s, todos }))} />
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
      <p className="ca-cv-tidy-hint">Working sessions and ones that need you stay, whatever their age. Frames you drew keep what’s in them. New work comes on within these ages too.</p>
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
