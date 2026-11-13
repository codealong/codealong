// What a session's agent left running in the background (Session.tasks), in words: the chip in
// its header, each command's state on the list under it, and the address a server printed.

import { agoPhrase, duration } from './format';
import type { BackgroundTask, Session } from './types';

/** How many of a session's background commands run, and how many ended a little while ago. */
export function taskCounts(tasks: BackgroundTask[] | undefined): { running: number; ended: number } {
  const all = tasks ?? [];
  const running = all.filter((t) => t.endedAt == null).length;
  return { running, ended: all.length - running };
}

/** The header chip's words: what runs (`2 running`), else what ended (`1 ended`); null with none. */
export function taskChipWords(s: Pick<Session, 'tasks'>): string | null {
  const { running, ended } = taskCounts(s.tasks);
  if (running > 0) return `${running} running`;
  return ended > 0 ? `${ended} ended` : null;
}

/** The chip's tooltip: the commands, a line each, and what a click does. */
export function taskChipTitle(s: Pick<Session, 'tasks'>, now: number): string {
  const lines = (s.tasks ?? []).map((t) => `${taskName(t)} · ${taskState(t, now)}`);
  return `Running in the background, as its harness lists it:\n${lines.join('\n')}`;
}

/** What a command is called on the list: the command, else what the agent said it's for. */
export function taskName(t: BackgroundTask): string {
  return t.command.trim() || t.about?.trim() || 'A command its harness didn’t record';
}

/** Where a command stands: `Running · 12m`, `Done · 3m ago`, `Failed · exit 1 · just now`,
 *  `Stopped · 4m ago`. */
export function taskState(t: BackgroundTask, now: number): string {
  if (t.endedAt == null) return `Running · ${duration(t.startedAt, now)}`;
  const when = agoPhrase(t.endedAt, now);
  switch (t.status) {
    case 'killed':
      return `Stopped · ${when}`;
    case 'stopped':
      return `Stopped as its session ended · ${when}`;
    case 'failed':
      return t.exitCode != null ? `Failed · exit ${t.exitCode} · ${when}` : `Failed · ${when}`;
    default:
      return t.exitCode != null && t.exitCode !== 0 ? `Ended · exit ${t.exitCode} · ${when}` : `Done · ${when}`;
  }
}

/** The last address on this machine a command printed (a dev server's `http://localhost:4321/`),
 *  for a link beside its output; null when it printed none. */
export function localAddress(output: string): string | null {
  const found = output.match(/https?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\])(?::\d+)?(?:\/[^\s"'<>)\]]*)?/g);
  const last = found?.[found.length - 1];
  return last ? last.replace('0.0.0.0', 'localhost').replace(/[.,;:]+$/, '') : null;
}

/** The last `n` lines of a command's output, without the blank ones it ends on. */
export function lastLines(output: string, n: number): string {
  const lines = output.replace(/\s+$/, '').split('\n');
  return lines.slice(-n).join('\n');
}
