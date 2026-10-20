// The Board view's tools, in the strip over it (ListBar.tsx): the board shown, whose menu picks
// another, makes one or changes this one, the Kanban | Canvas toggle, and what the board takes in.

import { ruleWords, type BoardDoc } from '../boards/model';
import { keysIn } from '../platform';
import type { Label } from '../types';
import { Icon, Seg } from './primitives';

export type BoardLayout = 'kanban' | 'canvas';

const LAYOUTS: { id: BoardLayout; label: string; icon: string; title: string }[] = [
  { id: 'kanban', label: 'Kanban', icon: 'layout-kanban', title: keysIn('Kanban: its todos and sessions in columns (⇧⌘K)') },
  { id: 'canvas', label: 'Canvas', icon: 'layout-grid', title: keysIn('Canvas: its work laid out in space (⇧⌘B)') },
];

interface BoardBarProps {
  board: BoardDoc;
  /** Its project, when the window shows another (All projects): it's that project's work. */
  project: string | null;
  layout: BoardLayout;
  labels: Label[];
  onLayout: (layout: BoardLayout) => void;
  /** The board's menu: the others to show, a new one, its settings. */
  onMenu: () => void;
  /** Its settings: what it takes in. */
  onEdit: () => void;
}

export function BoardBar({ board, project, layout, labels, onLayout, onMenu, onEdit }: BoardBarProps) {
  return (
    <div className="ca-boardbar" role="toolbar" aria-label="Board">
      <button type="button" className="ca-boardbar-name" title="Show another board, or make one" aria-haspopup="menu" onClick={onMenu} onContextMenu={(e) => (e.preventDefault(), onMenu())}>
        <span>{board.name}</span>
        <Icon name="chevron-down" size={12} />
      </button>
      <Seg className="ca-boardbar-layout" label="Show as" look="both" iconSize={14} options={LAYOUTS} value={layout} onPick={onLayout} />
      <button type="button" className="ca-linkbtn ca-boardbar-takes" title="What it takes in by itself: change it" onClick={onEdit}>
        {project && `In ${project}: `}
        {ruleWords(board.rule, labels)}
        {board.added.length > 0 && ` · ${board.added.length} added`}
        {board.removed.length > 0 && ` · ${board.removed.length} taken off`}
      </button>
    </div>
  );
}
