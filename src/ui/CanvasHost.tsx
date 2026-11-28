// Host for a board's two spatial views: the manual Canvas (CanvasView.tsx) and the dynamic
// session-first Map (WorkingSetCanvas.tsx). Switching between Kanban, Map and Canvas lives in
// BoardBar.tsx, so neither view carries a duplicate mode toggle.

import type { MutableRefObject } from 'react';
import type { BoardDoc } from '../boards/model';
import type { MenuSpec } from '../contextMenu';
import type { CanvasDoc, Entities, OnBoard, RefItem } from '../canvas/model';
import type { Label, Pr, PrItem, ScheduleItem, Theme, Todo } from '../types';
import { CanvasView } from './CanvasView';
import { WorkingSetCanvas } from './WorkingSetCanvas';

export type CanvasMode = 'board' | 'working-set';

export interface CanvasHostProps {
  mode: CanvasMode;
  board: BoardDoc;
  doc: CanvasDoc | null;
  update: (fn: (doc: CanvasDoc) => CanvasDoc) => void;
  entities: Entities;
  holds: OnBoard;
  schedules: ScheduleItem[];
  prs: PrItem[];
  onTakeOff: (cards: string[]) => void;
  onPutOn: (cards: string[]) => void;
  onSettings: () => void;
  labels: Map<string, Label>;
  todos: Todo[];
  now: number;
  terminal: string;
  theme: Theme;
  project: string;
  focus: { ids: string[]; n: number } | null;
  onOpen: (it: RefItem) => void;
  onShowPr: (pr: Pr) => void;
  onShowSchedule?: (id: string) => void;
  menuFor: (it: RefItem) => MenuSpec;
  onToast: (message: string) => void;
  onUndo: (message: string, undo: () => void, icon?: string) => void;
  keys: MutableRefObject<(e: KeyboardEvent) => boolean>;
  query: string;
  filtering: boolean;
  matches: (it: RefItem) => boolean;
  fitMatches: number;
  onSelectItem: (it: RefItem | null) => void;
}

export function CanvasHost(props: CanvasHostProps) {
  if (props.mode === 'working-set') {
    return (
      <WorkingSetCanvas
        board={props.board}
        doc={props.doc}
        update={props.update}
        holds={props.holds}
        entities={props.entities}
        schedules={props.schedules}
        prs={props.prs}
        todos={props.todos}
        labels={props.labels}
        now={props.now}
        project={props.project}
        query={props.query}
        filtering={props.filtering}
        matches={props.matches}
        onTakeOff={props.onTakeOff}
        onPutOn={props.onPutOn}
        onSettings={props.onSettings}
        onSelectItem={props.onSelectItem}
        onOpen={props.onOpen}
        onShowSchedule={props.onShowSchedule}
        menuFor={props.menuFor}
        onToast={props.onToast}
        onUndo={props.onUndo}
        keys={props.keys}
      />
    );
  }

  if (!props.doc) return null;
  return (
    <CanvasView
      doc={props.doc}
      update={props.update}
      entities={props.entities}
      holds={props.holds}
      onTakeOff={props.onTakeOff}
      onPutOn={props.onPutOn}
      onSettings={props.onSettings}
      labels={props.labels}
      todos={props.todos}
      now={props.now}
      terminal={props.terminal}
      theme={props.theme}
      project={props.project}
      focus={props.focus}
      onOpen={props.onOpen}
      onShowPr={props.onShowPr}
      menuFor={props.menuFor}
      onToast={props.onToast}
      onUndo={props.onUndo}
      keys={props.keys}
      query={props.query}
      filtering={props.filtering}
      matches={props.matches}
      fitMatches={props.fitMatches}
      onSelectItem={props.onSelectItem}
    />
  );
}
