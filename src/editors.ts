// The editors a worktree's folder opens in (editors.rs): yours, which `l|editor` opens, and the
// others installed on this Mac. Picking another makes it yours, as Codex's Open button does.

import type { Board, Lane, Profile, Target } from './types';

type Apps = Pick<Board['apps'], 'editor' | 'editorId' | 'editors'>;

/** The target that opens a worktree's folder in the editor `id`. */
export const editorTarget = (id: string) => `l|editor|${id}`;

/** Whether the worktree's folder is on this Mac, where an editor can open it. */
export const opensInEditor = (lane: Lane) => !lane.device && lane.targets.some((t) => t.id === 'l|editor');

/** The editors installed here besides yours, in menu order. */
export function otherEditors(apps: Apps): Profile[] {
  return (apps.editors ?? []).filter((e) => e.id !== apps.editorId);
}

/** Yours first, then the others. */
export function allEditors(apps: Apps): Profile[] {
  return [{ id: apps.editorId, name: apps.editor }, ...otherEditors(apps)].filter((e) => e.id);
}

function target(e: Profile, words: string): Target {
  return { id: editorTarget(e.id), icon: 'code', label: `${words} ${e.name}`, detail: null, kbd: null, disabled: false, group: 'lane' };
}

/** A worktree's targets with the other editors after Open in <yours>, for its Open menu. */
export function withOtherEditors(targets: Target[], apps: Apps): Target[] {
  const at = targets.findIndex((t) => t.id === 'l|editor');
  const more = otherEditors(apps);
  if (at < 0 || more.length === 0) return targets;
  return [...targets.slice(0, at + 1), ...more.map((e) => target(e, 'Open in')), ...targets.slice(at + 1)];
}

/** Every editor as Open folder in <editor>, for a session's Open menu, whose other targets open
 *  the session itself. */
export function folderTargets(apps: Apps): Target[] {
  return allEditors(apps).map((e) => target(e, 'Open folder in'));
}
