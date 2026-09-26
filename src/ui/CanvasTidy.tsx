// The canvas's two panels above its toolbar, with settings of their own. Tidy lays the canvas out
// once, holding the kinds of things and ages it's given: what's older comes off, kept or not
// (unarchived sessions pile up; this is where they leave), and it says first what would come off
// and on. Clear canvas starts it over. Bring in says which new work comes on by itself after
// that, as it moves on; it never takes anything off.

import { useMemo, useState } from 'react';
import { PR_SCOPES, SESSION_AGES, TODO_AGES, tidyPlan, type CanvasDoc, type Entities, type Filter, type Include, type RefItem, type RefKind, type Scope, type TidyMode, type TidyOptions, type TidyPlan } from '../canvas/model';
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

/** Each kind on or off, and how far back: the rows both panels share. */
function FilterRows<T extends Filter>({ value, onChange }: { value: T; onChange: (v: T) => void }) {
  const inc = value.include;
  const set = (patch: Partial<Include>) => onChange({ ...value, include: { ...inc, ...patch } });
  return (
    <>
      <Check label="Sessions" on={inc.sessions} onChange={(sessions) => set({ sessions })} title="Sessions active in the time chosen, and any that's working or needs you" />
      {inc.sessions ? <Seg label="Sessions active in" options={SESSION_AGES} value={value.sessions} onPick={(sessions) => onChange({ ...value, sessions })} /> : <span />}
      <Check label="Todos" on={inc.todos} onChange={(todos) => set({ todos })} title="Open todos changed or worked on in the time chosen" />
      {inc.todos ? <Seg label="Todos" options={TODO_AGES} value={value.todos} onPick={(todos) => onChange({ ...value, todos })} /> : <span />}
      <Check label="Worktrees" on={inc.worktrees} onChange={(worktrees) => set({ worktrees })} title="Recent worktrees even with nothing of theirs on the canvas" />
      <span className="ca-cv-tidy-note">{inc.worktrees ? 'Recent ones, even with nothing on them' : 'Only above their sessions, todos and PRs'}</span>
      <span className="ca-cv-tidy-rowlabel">PRs</span>
      <Seg label="PRs" options={PR_SCOPES} value={inc.prs} onPick={(prs) => set({ prs })} />
    </>
  );
}

interface TidyPanelProps {
  doc: CanvasDoc;
  entities: Entities;
  now: number;
  pinned: Set<string>;
  onTidy: (opts: TidyOptions, plan: TidyPlan) => void;
  /** Takes everything off; only what moves on afterwards comes back. */
  onClear: () => void;
  onClose: () => void;
}

/** Tidy's own settings, used when you press Tidy and kept for the next one. */
export function TidyPanel({ doc, entities, now, pinned, onTidy, onClear, onClose }: TidyPanelProps) {
  const [opts, setOpts] = useState<TidyOptions>(doc.tidy);
  const plan = useMemo(() => tidyPlan(doc, entities, now, pinned, opts), [doc, entities, now, pinned, opts]);
  return (
    <div className="ca-cv-tidy" role="dialog" aria-label="Tidy">
      <div className="ca-cv-tidy-head">Tidy once, keeping</div>
      <div className="ca-cv-tidy-grid">
        <FilterRows value={opts} onChange={setOpts} />
        <span className="ca-cv-tidy-rowlabel">Layout</span>
        <Seg label="Layout" options={LAYOUTS} value={opts.layout} onPick={(layout) => setOpts((o) => ({ ...o, layout }))} />
      </div>
      <p className="ca-cv-tidy-sum">{tidySummary(plan)}</p>
      <p className="ca-cv-tidy-hint">Cards older than this come off, even ones you moved. Frames you drew keep what’s in them.</p>
      <div className="ca-cv-tidy-actions">
        <button
          type="button"
          className="ca-btn ca-btn--ghost ca-btn--sm ca-cv-tidy-clear"
          title="Take everything off the canvas, notes and frames too. Bring in adds sessions and todos back as they move on: a new prompt, a finished turn, a change."
          onClick={onClear}
        >
          Clear canvas
        </button>
        <button type="button" className="ca-btn ca-btn--ghost ca-btn--sm" onClick={onClose}>
          Cancel
        </button>
        <button type="button" className="ca-btn ca-btn--primary ca-btn--sm" autoFocus onClick={() => onTidy(opts, plan)}>
          Tidy
        </button>
      </div>
    </div>
  );
}

interface BringInPanelProps {
  doc: CanvasDoc;
  /** Changes take effect at once. */
  onChange: (scope: Scope) => void;
}

/** Which new work comes onto the canvas by itself. */
export function BringInPanel({ doc, onChange }: BringInPanelProps) {
  return (
    <div className="ca-cv-tidy ca-cv-bring" role="dialog" aria-label="Bring in">
      <div className="ca-cv-tidy-head">Bring in new work by itself</div>
      <div className="ca-cv-tidy-grid">
        <FilterRows value={doc.scope} onChange={onChange} />
      </div>
      <p className="ca-cv-tidy-hint">
        What moves on after the last {doc.scope.cleared ? 'clear' : 'tidy'} lands beside its worktree: a new prompt, a finished turn, a change. Nothing comes off by itself; Tidy decides what stays.
      </p>
    </div>
  );
}
