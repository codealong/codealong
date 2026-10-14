// The main window's answers to `codealong canvas …` (cli.rs), which the engine hands it
// (canvas.rs `ask`): the window holds the canvases' shape and their layout (model.ts), so what a
// project's canvas holds, and arranging it in the frames an agent names, are worked out here. The
// canvas this window shows changes through its own handle; another project's is read, changed and
// saved, and a window showing that one reads it again (useCanvas.ts).

import { useEffect } from 'react';
import { canvasAnswer, canvasLoad, canvasSave, onCanvasAsk, type CanvasAsk } from '../api';
import { useLatest } from '../latest';
import { ALL_PROJECTS, inProject } from '../quick';
import { errorText } from '../report';
import type { Board, Lane, Todo } from '../types';
import { arrange, emptyDoc, entitiesOf, outline, readDoc, type ArrangeFrame, type ArrangeRest, type Entities } from './model';
import { seed, type CanvasHandle } from './useCanvas';

/** What `codealong canvas …` asks for, as canvas.rs `Op` sends it. */
type Op = { kind: 'show' } | { kind: 'arrange'; frames: ArrangeFrame[]; rest: ArrangeRest };

/** The project a request names, as the window lists it: in any case, `all` for every project, and
 *  the one the window shows for none. */
export function canvasProject(asked: string | null, current: string, projects: { name: string }[]): string {
  const name = asked?.trim().toLowerCase();
  if (!name) return current;
  if (name === 'all' || name === ALL_PROJECTS.toLowerCase()) return ALL_PROJECTS;
  const found = projects.find((p) => p.name.toLowerCase() === name);
  if (!found) throw new Error(`There's no project ${asked}. The projects are ${[...projects.map((p) => p.name), 'all'].join(', ')}.`);
  return found.name;
}

export interface CanvasOps {
  /** The board isn't read yet: a canvas worked out now would leave this Mac's worktrees out. */
  loading: boolean;
  board: Board | null;
  /** Every project's worktrees as this window lists them, and its todos. */
  lanes: Lane[];
  todos: Todo[];
  projects: { name: string }[];
  /** The project this window shows, its canvas and what that canvas can show. */
  current: string;
  canvas: CanvasHandle;
  entities: Entities;
  pinned: Set<string>;
}

/** Answers each `codealong canvas …` request the engine hands this window. */
export function useCanvasOps(ops: CanvasOps): void {
  const latest = useLatest(ops);
  useEffect(
    () =>
      onCanvasAsk((ask) => {
        answer(ask, latest.current)
          .then(
            (result) => canvasAnswer(ask.id, { result }),
            (e: unknown) => canvasAnswer(ask.id, { error: errorText(e) }),
          )
          // The engine stops waiting by itself, and tells the command the window didn't answer.
          .catch(() => {});
      }),
    [latest],
  );
}

async function answer(ask: CanvasAsk, o: CanvasOps): Promise<unknown> {
  if (o.loading) throw new Error('Codealong is still reading the board. Ask again in a moment.');
  const project = canvasProject(ask.project, o.current, o.projects);
  const here = project === o.current;
  if (here && !o.canvas.loaded) throw new Error(`The ${project} canvas is still loading. Ask again in a moment.`);
  const e = here ? o.entities : entitiesOf(o.board, o.lanes.filter((l) => inProject(l, project)), o.todos, (p) => project === ALL_PROJECTS || p === project);
  const doc = here ? o.canvas.doc : await canvasLoad(project).then((raw) => (raw ? readDoc(raw) : null));
  const op = ask.op as Op;
  if (op.kind === 'show') return { project, exists: !!doc, ...outline(doc ?? emptyDoc(), e) };
  if (op.kind === 'arrange') {
    const now = Date.now();
    // A project whose canvas was never shown starts as it would have: tidied.
    const done = arrange(doc ?? seed(e, now, o.pinned), e, op.frames, op.rest, now, o.pinned);
    // Saved before the command hears it's done, so a failure to save is its answer.
    await (here ? o.canvas.put(done.doc) : canvasSave(project, done.doc));
    return { project, frames: done.frames, missing: done.missing, rest: done.rest };
  }
  throw new Error('This Codealong doesn’t know that canvas request: the command is newer than the app.');
}
