// The main window's answers to `codealong canvas …` (cli.rs), which the engine hands it
// (canvas.rs `ask`): the window holds the canvases' shape and their layout (model.ts), so what a
// project's canvas holds, and arranging it in the frames an agent names, are worked out here. The
// canvas this window shows changes through its own handle; another project's is read, changed and
// saved, and a window showing that one reads it again (useCanvas.ts).
//
// A request is answered from the board as the engine has it when it's asked, not as the window
// last drew it: a window that's hidden, covered or behind a locked screen isn't drawn (WebKit holds
// its page back), so what its last render had can be hours old, or from before the first scan.

import { useEffect } from 'react';
import { canvasAnswer, getBoard, onCanvasAsk, type CanvasAsk } from '../api';
import { projectsOf, unarchived } from '../boardLists';
import { withDevices, type DeviceFilter } from '../devices';
import { useLatest } from '../latest';
import { ALL_PROJECTS, inProject } from '../quick';
import { errorText } from '../report';
import type { Lane } from '../types';
import { arrange, emptyDoc, entitiesOf, outline, type ArrangeFrame, type ArrangeRest } from './model';
import { readCanvas, saveCanvas, seed, type CanvasHandle } from './useCanvas';

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
  /** Other devices' worktrees, and the devices the window shows, which the board adds them to. */
  deviceLanes: Lane[];
  deviceFilter: DeviceFilter;
  /** The project this window shows, and its canvas. */
  current: string;
  canvas: CanvasHandle;
}

/** Answers each `codealong canvas …` request the engine hands this window. */
export function useCanvasOps(ops: CanvasOps): void {
  const latest = useLatest(ops);
  useEffect(
    () =>
      onCanvasAsk((ask) => {
        answer(ask, latest)
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

async function answer(ask: CanvasAsk, latest: { readonly current: CanvasOps }): Promise<unknown> {
  const board = await getBoard();
  if (board.loading) throw new Error('Codealong is still reading the board. Ask again in a moment.');
  const { deviceLanes, deviceFilter } = latest.current;
  const project = canvasProject(ask.project, latest.current.current, projectsOf(board.projects, deviceLanes));
  const lanes = unarchived(withDevices(board.lanes, deviceLanes, deviceFilter)).filter((l) => inProject(l, project));
  const e = entitiesOf(board, lanes, board.todos, (p) => project === ALL_PROJECTS || p === project);
  const pinned = new Set(board.pinnedWorktrees);
  const file = await readCanvas(project);
  // The window's own document once it holds the project's, which may have a change still to save,
  // else the file, read again if the window saved the project meanwhile (`readCanvas`). Taken
  // after every wait, with nothing waited on from here to the save, so a change made in the window
  // meanwhile is in what's arranged, and the project is still the one the window shows if it's
  // put there.
  const o = latest.current;
  const here = project === o.current;
  const doc = here && o.canvas.loaded ? o.canvas.doc : file;
  const op = ask.op as Op;
  if (op.kind === 'show') return { project, exists: !!doc, ...outline(doc ?? emptyDoc(), e) };
  if (op.kind === 'arrange') {
    const now = Date.now();
    // A project whose canvas was never shown starts as it would have: tidied.
    const done = arrange(doc ?? seed(e, now, pinned), e, op.frames, op.rest, now, pinned);
    // Saved before the command hears it's done, so a failure to save is its answer. The project
    // the window shows goes through its handle, so the window holds what was saved.
    await (here ? o.canvas.put(done.doc) : saveCanvas(project, done.doc));
    return { project, frames: done.frames, missing: done.missing, rest: done.rest };
  }
  throw new Error('This Codealong doesn’t know that canvas request: the command is newer than the app.');
}
