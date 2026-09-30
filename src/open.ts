// Opening what a window shows where it lives: a session in its terminal or app, a worktree in
// its editor, Finder or browser. Every window opens the same way and says it in the same words;
// what differs is what it does around that (a panel puts itself away first) and where it says
// what went wrong (its `Report`).

import { openFromBar, openTarget } from './api';
import { errorText, type Report } from './report';
import type { Lane, Session } from './types';

/** A worktree's own entry for one of its targets, as `lane.targets` lists it. */
export interface Listed {
  label: string;
  detail: string | null;
  disabled: boolean;
}

/** What to open: one of a worktree's targets (`s|<key>|focus`, `l|editor`), with the worktree's
 *  entry for it when it lists one. */
export interface Where {
  laneId: string;
  target: string;
  listed?: Listed | null;
}

/** A target of `lane`, with the worktree's entry for it. */
export function where(lane: Pick<Lane, 'id' | 'targets'>, target: string): Where {
  return { laneId: lane.id, target, listed: lane.targets.find((t) => t.id === target) };
}

/** How a session's open target reads when its worktree doesn't list it (worktrees list targets
 *  for their first few sessions only), in board.rs's words for it. */
export function openWords(target: string, terminal: string): string {
  const how: Record<string, string> = {
    focus: `Show in ${terminal}`,
    attach: `Attach in ${terminal}`,
    resume: `Resume in ${terminal}`,
    'claude-app': 'Open in Claude app',
    codex: 'Open in Codex',
    antigravity: 'Open Antigravity',
    hosted: 'Show in Codealong',
  };
  return how[target.split('|')[2]] ?? 'Open';
}

/** How a session opens, in words: its worktree's label for it (Respond in Ghostty), else
 *  `openWords`. */
export function openLabel(lane: Pick<Lane, 'targets'>, s: Pick<Session, 'openTarget'>, terminal: string): string {
  return lane.targets.find((t) => t.id === s.openTarget)?.label ?? openWords(s.openTarget, terminal);
}

export interface Opener {
  /** Where the window says that the target isn't available, or that opening it failed. */
  report: Report;
  /** The main window opens with `open_target`, which also outlines the terminal it brought
   *  forward; a panel that leaves Codealong in the background (the bar, the peek, the quick
   *  panel, a desktop card) with `open_from_bar`, which brings the app forward itself. */
  from: 'main' | 'panel';
  /** Done once it's going to open: a panel puts itself away. */
  before?: () => void;
}

/** Opens a target where it lives, unless its worktree says it can't be now (and why). */
export function openWhere(w: Where, how: Opener): void {
  if (w.listed?.disabled) {
    how.report.error(w.listed.detail ?? `${w.listed.label} isn't available right now.`);
    return;
  }
  how.before?.();
  const work = how.from === 'main' ? openTarget(w.laneId, w.target) : openFromBar(w.laneId, w.target);
  const shown = how.report.busy ? how.report.busy(`${w.listed?.label ?? 'Opening'}…`, work) : work;
  shown.catch((e: unknown) => how.report.error(errorText(e)));
}
