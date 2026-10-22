// Making a board, or changing one: its name and what it takes in by itself (boards/model.ts): its
// project's everything, what a rule says (RuleFields.tsx), or only what you add. A new board picks
// its Kanban's columns too: the four stages, one per label, or one to make your own beside. A
// column of a board's Kanban is made or changed here as well (`ColumnDialog`): its name, what it
// holds (what a rule says, or the cards you put in it) and its order.

import { useEffect, useRef, useState } from 'react';
import { cleanRule, presetColumns, ruleWords, takesAll, type BoardColumn, type ColumnsPreset, type Rule, type Sort } from '../boards/model';
import { Button, Dialog, Seg } from './primitives';
import { RuleFields, type RuleChoices } from './RuleFields';

type Takes = 'all' | 'rule' | 'hand';

const TAKES: { id: Takes; label: string; title: string }[] = [
  { id: 'all', label: 'Everything', title: 'Everything in the project' },
  { id: 'rule', label: 'What matches', title: 'What the rule below takes in, as it moves on' },
  { id: 'hand', label: 'Only what you add', title: 'Only what you add to it from a menu, or an agent adds' },
];

const PRESETS: { id: ColumnsPreset; label: string; title: string }[] = [
  { id: 'stages', label: 'Stages', title: 'To do, In progress, In review and Done, which follow the work' },
  { id: 'labels', label: 'By label', title: 'A column per label, and Other' },
  { id: 'blank', label: 'Your own', title: 'One column, Inbox, to make your own beside' },
];

const takesOf = (r: Rule | null): Takes => (r === null ? 'hand' : takesAll(r) ? 'all' : 'rule');

export interface BoardDraft {
  name: string;
  rule: Rule | null;
  /** For a new board, the columns it starts with. */
  columns?: BoardColumn[] | null;
}

interface BoardDialogProps extends RuleChoices {
  /** The board to change, or none for a new one. */
  board: { name: string; rule: Rule | null } | null;
  project: string;
  /** A new board's name to start from. */
  suggestion?: string;
  onSave: (draft: BoardDraft) => void;
  onClose: () => void;
}

export function BoardDialog({ board, project, suggestion = '', onSave, onClose, ...choices }: BoardDialogProps) {
  const [name, setName] = useState(board?.name ?? suggestion);
  const [takes, setTakes] = useState<Takes>(board ? takesOf(board.rule) : 'hand');
  const [rule, setRule] = useState<Rule>(board?.rule ?? {});
  const [preset, setPreset] = useState<ColumnsPreset>('stages');
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.select(), []);

  const ruleNow = takes === 'hand' ? null : takes === 'all' ? {} : cleanRule(rule);
  const save = () => {
    if (!name.trim()) return;
    // A board by label starts with the labels its rule takes in, else every one.
    const labels = ruleNow?.labels?.length ? choices.labels.filter((l) => ruleNow.labels?.includes(l.id)) : choices.labels;
    onSave({ name: name.trim(), rule: ruleNow, ...(board ? {} : { columns: presetColumns(preset, labels) }) });
    onClose();
  };

  return (
    <Dialog
      className="ca-boarddlg"
      title={board ? 'Board settings' : 'New board'}
      lede={board ? `A board in ${project}. What it takes in shows on its Kanban and its canvas.` : `A board in ${project}, shown as Kanban or as Canvas. Add to it from any row’s menu, or have it take in what matches.`}
      onClose={onClose}
      onSubmit={save}
      aside={<span className="ca-boarddlg-sum">{ruleWords(ruleNow, choices.labels)}</span>}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" disabled={!name.trim()}>
            {board ? 'Save' : 'Make board'}
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        <label className="ca-field">
          <span>Name</span>
          <input ref={input} value={name} autoFocus spellCheck={false} onChange={(e) => setName(e.target.value)} />
        </label>
        <div className="ca-field">
          <span>Takes in</span>
          <Seg label="Takes in" options={TAKES} value={takes} onPick={setTakes} />
        </div>
        {takes === 'rule' && <RuleFields rule={rule} onChange={setRule} {...choices} />}
        {!board && (
          <div className="ca-field">
            <span>Kanban columns</span>
            <Seg label="Kanban columns" options={choices.labels.length > 0 ? PRESETS : PRESETS.filter((p) => p.id !== 'labels')} value={preset} onPick={setPreset} />
          </div>
        )}
      </div>
    </Dialog>
  );
}

const SORTS: { id: Sort; label: string; title: string }[] = [
  { id: 'hand', label: 'Your order', title: 'As you drag them' },
  { id: 'move', label: 'Whose move', title: 'What needs you first, then your turns, then what’s working' },
  { id: 'latest', label: 'Latest first', title: 'What moved on last first' },
];

const HOLDS: { id: 'hand' | 'rule'; label: string; title: string }[] = [
  { id: 'hand', label: 'What you put here', title: 'Cards you drag here, and ones no column’s rule takes' },
  { id: 'rule', label: 'What matches', title: 'Cards the rule below takes, as they move on' },
];

interface ColumnDialogProps extends RuleChoices {
  /** The column to change, or none for a new one. */
  column: BoardColumn | null;
  onSave: (column: Omit<BoardColumn, 'id'>) => void;
  onClose: () => void;
}

export function ColumnDialog({ column, onSave, onClose, ...choices }: ColumnDialogProps) {
  const [name, setName] = useState(column?.name ?? '');
  const [holds, setHolds] = useState<'hand' | 'rule'>(column?.rule ? 'rule' : 'hand');
  const [rule, setRule] = useState<Rule>(column?.rule ?? {});
  const [sort, setSort] = useState<Sort>(column?.sort ?? 'hand');
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.select(), []);

  const save = () => {
    if (!name.trim()) return;
    onSave({ name: name.trim(), rule: holds === 'rule' ? cleanRule(rule) : null, sort, ...(column?.folded ? { folded: true } : {}) });
    onClose();
  };

  return (
    <Dialog
      className="ca-boarddlg"
      title={column ? 'Column settings' : 'New column'}
      lede="A card goes in the first column whose rule takes it, else in the column you put it in, else in the first column without a rule."
      onClose={onClose}
      onSubmit={save}
      aside={holds === 'rule' ? <span className="ca-boarddlg-sum">{ruleWords(cleanRule(rule), choices.labels)}</span> : undefined}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" disabled={!name.trim()}>
            {column ? 'Save' : 'Add column'}
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body">
        <label className="ca-field">
          <span>Name</span>
          <input ref={input} value={name} autoFocus spellCheck={false} onChange={(e) => setName(e.target.value)} />
        </label>
        <div className="ca-field">
          <span>Holds</span>
          <Seg label="Holds" options={HOLDS} value={holds} onPick={setHolds} />
        </div>
        {holds === 'rule' && <RuleFields rule={rule} onChange={setRule} {...choices} />}
        <div className="ca-field">
          <span>Order</span>
          <Seg label="Order" options={SORTS} value={sort} onPick={setSort} />
        </div>
      </div>
    </Dialog>
  );
}

