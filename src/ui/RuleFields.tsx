// A rule's parts, to pick (boards/model.ts `Rule`): the kinds of things, their stages and
// statuses, todo labels, repos, accounts, how recently they moved on, and words in their titles.
// Every part picked must hold, and within one any pick does. The board dialog takes in what a
// rule says (BoardDialog.tsx), and a column holds what its rule says (ColumnDialog.tsx).

import { useState, type ReactNode } from 'react';
import { ACTIVES, KINDS, KIND_WORD, STAGES, STAGE_WORD, STATUS_PICKS, STATUS_WORD, type Active, type Rule } from '../boards/model';
import type { Label } from '../types';
import { PickChip, Seg } from './primitives';
import { LabelDot } from './TodoBits';

export interface RuleChoices {
  labels: Label[];
  /** The project's repos, as rules name them (rules.ts `repoOf`), with what the window calls each. */
  repos: { id: string; label: string }[];
  accounts: string[];
}

interface RuleFieldsProps extends RuleChoices {
  rule: Rule;
  onChange: (rule: Rule) => void;
}

const ACTIVE_OPTIONS: { id: Active | 'any'; label: string; title: string }[] = [
  { id: 'any', label: 'Any time', title: 'However long ago it last moved on' },
  ...ACTIVES.map((a) => ({ id: a, label: a === '1d' ? '24h' : a, title: `Moved on in the last ${a === '1d' ? '24 hours' : a}` })),
];

/** How many of a long list's picks show before "more", as the sidebar's facets show theirs. */
const SHOWN = 8;

/** Picks one, or unpicks it. */
function toggled<T>(list: T[] | undefined, x: T): T[] {
  const l = list ?? [];
  return l.includes(x) ? l.filter((y) => y !== x) : [...l, x];
}

/** A long list's picks: the first few and the ones picked, then "more" for the rest. */
function LongPicks<T>({ items, picked, children }: { items: T[]; picked: (item: T) => boolean; children: (item: T) => ReactNode }) {
  const [all, setAll] = useState(false);
  const shown = all ? items : items.filter((x, i) => i < SHOWN || picked(x));
  return (
    <div className="ca-rule-picks">
      {shown.map(children)}
      {(all || shown.length < items.length) && (
        <button type="button" className="ca-linkbtn ca-rule-more" onClick={() => setAll(!all)}>
          {all ? 'Fewer' : `${items.length - shown.length} more`}
        </button>
      )}
    </div>
  );
}

export function RuleFields({ rule, onChange, labels, repos, accounts }: RuleFieldsProps) {
  const set = (patch: Partial<Rule>) => onChange({ ...rule, ...patch });
  return (
    <div className="ca-rule">
      <span className="ca-rule-label">Kinds</span>
      <div className="ca-rule-picks">
        {KINDS.map((k) => (
          <PickChip key={k} on={!!rule.kinds?.includes(k)} onPick={() => set({ kinds: toggled(rule.kinds, k) })}>
            {KIND_WORD[k]}
          </PickChip>
        ))}
      </div>
      <span className="ca-rule-label">Stage</span>
      <div className="ca-rule-picks">
        {STAGES.map((s) => (
          <PickChip key={s} on={!!rule.stages?.includes(s)} onPick={() => set({ stages: toggled(rule.stages, s) })}>
            {STAGE_WORD[s]}
          </PickChip>
        ))}
      </div>
      <span className="ca-rule-label">Status</span>
      <div className="ca-rule-picks">
        {STATUS_PICKS.map((s) => (
          <PickChip key={s} on={!!rule.statuses?.includes(s)} title="An agent’s status; a todo’s or a worktree’s by its sessions" onPick={() => set({ statuses: toggled(rule.statuses, s) })}>
            {STATUS_WORD[s]}
          </PickChip>
        ))}
      </div>
      {labels.length > 0 && (
        <>
          <span className="ca-rule-label">Labels</span>
          <LongPicks items={labels} picked={(l) => !!rule.labels?.includes(l.id)}>
            {(l) => (
              <PickChip key={l.id} on={!!rule.labels?.includes(l.id)} title="Todos with the label, and the sessions and worktrees on them" onPick={() => set({ labels: toggled(rule.labels, l.id) })}>
                <LabelDot label={l} />
                <span>{l.name}</span>
              </PickChip>
            )}
          </LongPicks>
        </>
      )}
      {repos.length > 1 && (
        <>
          <span className="ca-rule-label">Repos</span>
          <LongPicks items={repos} picked={(r) => !!rule.repos?.includes(r.id)}>
            {(r) => (
              <PickChip key={r.id} on={!!rule.repos?.includes(r.id)} title={r.id} onPick={() => set({ repos: toggled(rule.repos, r.id) })}>
                {r.label}
              </PickChip>
            )}
          </LongPicks>
        </>
      )}
      {accounts.length > 1 && (
        <>
          <span className="ca-rule-label">Accounts</span>
          <div className="ca-rule-picks">
            {accounts.map((a) => (
              <PickChip key={a} on={!!rule.accounts?.includes(a)} title="The account’s sessions, and the todos and worktrees they’re on" onPick={() => set({ accounts: toggled(rule.accounts, a) })}>
                {a}
              </PickChip>
            ))}
          </div>
        </>
      )}
      <span className="ca-rule-label">Active</span>
      <Seg label="Active in" options={ACTIVE_OPTIONS} value={rule.active ?? 'any'} onPick={(a) => set({ active: a === 'any' ? undefined : a })} />
      <span className="ca-rule-label">Title has</span>
      <label className="ca-field ca-rule-text">
        <input value={rule.text ?? ''} placeholder="Words in its title" aria-label="Words in its title" spellCheck={false} onChange={(e) => set({ text: e.target.value })} />
      </label>
    </div>
  );
}
