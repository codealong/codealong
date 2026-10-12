// Tips on using Codealong, shown where a window has nothing to show yet but that it's reading
// (ui/Tip.tsx): the main window's board before the engine's first scan, a conversation being
// read, the waiting window and the quick panel before the board comes, a PR's description on its
// way from GitHub. This is the one list of them. A window asks for the tips of its place, and they
// come round in this order, the ones never shown first, then the one shown longest ago, wherever
// it showed: so the list's order is what someone new sees first.
//
// A tip says its keys through the tables the windows run on (menus.ts `CHORDS`, queue.ts
// `QUEUE_CHORDS`, snooze.ts `SNOOZE_CHORDS`, menus.ts `PR_CHORDS`), so it follows them, and a
// shortcut that works in every app as config.toml sets it, so a tip about one that's off doesn't
// show. The words go stale when what they describe changes: change the
// tip here with it. Like all UI copy, a tip is sentence case and plain words, one or two short
// sentences (tips.test.ts holds the length).

import { VIEW_CHORDS } from './mainKeys';
import { CHORDS, PR_CHORDS } from './menus';
import { QUEUE_CHORDS } from './queue';
import { shortcutLabel } from './quick';
import { SNOOZE_CHORDS } from './snooze';

/** Where a tip can show: the main window's board, a session's conversation, the waiting window,
 *  the quick panel, a PR's panel. */
export type TipPlace = 'board' | 'session' | 'waiting' | 'quick' | 'pr';

/** The shortcuts that work in every app, which config.toml sets (`[shortcuts]`), as config.toml
 *  writes them (`Option+Cmd+/`); empty or missing when off or not working. */
export interface TipShortcuts {
  quick?: string;
  waiting?: string;
}

/** A tip's words: text; a key or chord as the chord tables write them (`Shift+Cmd+E`); one of the
 *  shortcuts config.toml sets; or a command or a name, set as code. */
export type TipPart = string | { chord: string } | { shortcut: keyof TipShortcuts } | { code: string };

export interface Tip {
  /** Kept in storage to remember which tips have shown: never reused for another tip. */
  id: string;
  places: readonly TipPlace[];
  says: TipPart[];
}

/** Writes a tip's words as a template: what's put in it is a key, a shortcut or code. */
function says(text: TemplateStringsArray, ...put: Exclude<TipPart, string>[]): TipPart[] {
  const out: TipPart[] = [];
  text.forEach((t, i) => {
    if (t) out.push(t);
    if (i < put.length) out.push(put[i]);
  });
  return out;
}
/** A chord from a table; a missing one is caught by tips.test.ts. */
const key = (chord: string | undefined) => ({ chord: chord ?? '' });
const code = (text: string) => ({ code: text });
const QUICK = { shortcut: 'quick' } as const;
const WAITING = { shortcut: 'waiting' } as const;

/** Every place, for the tests. */
export const TIP_PLACES: readonly TipPlace[] = ['board', 'session', 'waiting', 'quick', 'pr'];
const BOARD: readonly TipPlace[] = ['board'];
const SESSION: readonly TipPlace[] = ['session'];
const QUICK_PANEL: readonly TipPlace[] = ['quick'];
const SESSION_KEYS: readonly TipPlace[] = ['board', 'session', 'waiting'];

export const TIPS: readonly Tip[] = [
  { id: 'quick-panel', places: ['board', 'session', 'waiting', 'pr'], says: says`${QUICK} opens the quick panel from any app: type a few words to find a session, or a prompt to start one.` },
  { id: 'waiting-window', places: ['board', 'session', 'quick', 'pr'], says: says`${WAITING} goes through what waits on you, one at a time, from any app.` },
  { id: 'right-click', places: BOARD, says: says`Right-click a session, worktree, PR or todo for everything it can do, each action with its keys beside it.` },
  { id: 'view-letters', places: BOARD, says: says`${key(VIEW_CHORDS.sessions)} Sessions, ${key(VIEW_CHORDS.worktrees)} Worktrees, ${key(VIEW_CHORDS.prs)} PRs, ${key(VIEW_CHORDS.todo)} Todo and ${key(VIEW_CHORDS.canvas)} Canvas go straight to a view, from a text field too.` },
  { id: 'mark-done', places: SESSION_KEYS, says: says`${key(CHORDS.done)} marks a session done: it's closed where it runs, marked seen and archived.` },
  { id: 'mark-seen', places: SESSION_KEYS, says: says`${key(CHORDS.seen)} marks a finished turn seen, which takes it off the floating bar.` },
  { id: 'snooze', places: SESSION_KEYS, says: says`${key(SNOOZE_CHORDS[0])} to ${key(SNOOZE_CHORDS[SNOOZE_CHORDS.length - 1])} snooze a session for one of the Snooze menu's times. It comes back when the time is up.` },
  { id: 'bar-peek', places: ['board', 'session'], says: says`Rest the pointer on a pill in the floating bar to read that session and answer it, without leaving the app you're in.` },
  { id: 'history', places: SESSION_KEYS, says: says`${key(QUEUE_CHORDS.historyBack[0])} goes back to the session you looked at before, and ${key(QUEUE_CHORDS.historyForward[0])} forward again.` },
  { id: 'terminal', places: ['board', 'session'], says: says`${key(CHORDS.terminal)} opens a new terminal in the folder of what's shown, as VS Code opens its terminal.` },
  { id: 'queue-next', places: ['session', 'waiting'], says: says`${key(QUEUE_CHORDS.next[0])} skips to the next session that waits on you, and ${key(QUEUE_CHORDS.back[0])} goes back.` },
  { id: 'pin', places: BOARD, says: says`${key(CHORDS.pin)} pins a session to the top of Sessions. Drag the pinned ones into the order you want.` },
  { id: 'picks', places: BOARD, says: says`${key('Cmd')}-click or ${key('Shift')}-click picks several rows, and ${key('Cmd+A')} every row: the panel then acts on all of them at once.` },
  { id: 'desktop', places: BOARD, says: says`Drag a row out of the window to pin its card to the desktop, where it floats over every app.` },
  { id: 'side', places: ['session', 'waiting'], says: says`${code('/side')} and a question asks a throwaway copy of a Claude Code or Codex session. The session carries on and never hears of it.` },
  { id: 'slash', places: SESSION, says: says`Type ${code('/')} in a session's box for Codealong's commands and the skills its agent can run.` },
  { id: 'find', places: SESSION, says: says`${key('Cmd+F')} finds words anywhere in a session's conversation, from its first prompt on.` },
  { id: 'prompts', places: SESSION, says: says`${key('Cmd+Shift+Up')} and ${key('Cmd+Shift+Down')} bring your previous or next prompt to the top of the conversation.` },
  { id: 'copy-message', places: SESSION, says: says`Right-click a message in a conversation to copy it as it was written, Markdown and all.` },
  { id: 'fork', places: SESSION, says: says`${code('/fork')} in a session's box copies its conversation into a new session, and you stay where you are.` },
  { id: 'quiet', places: ['board', 'session'], says: says`A session that loops or wakes on a schedule can be made Quiet from its right-click menu: it comes to you only when it needs you.` },
  { id: 'move-account', places: ['board', 'session'], says: says`A Claude Code session stopped by its account's limit can go on in another account: Move to, in its right-click menu.` },
  { id: 'rename', places: SESSION_KEYS, says: says`${key(CHORDS.rename)} renames a session where its harness keeps names, so the harness shows the name too.` },
  { id: 'waiting-send', places: ['waiting'], says: says`${key('Enter')} sends and stays to watch the reply; ${key('Cmd+Enter')} sends and goes on to the next.` },
  { id: 'waiting-allow', places: ['waiting'], says: says`When a session asks to run something, ${key('Cmd+Enter')} allows it and ${key('Cmd+Backspace')} denies it.` },
  { id: 'pr-approve', places: ['waiting', 'pr'], says: says`In the waiting window, ${key(PR_CHORDS.approve)} approves a PR and goes on, and ${key(PR_CHORDS.comment)} comments.` },
  { id: 'pr-ask', places: ['waiting', 'pr'], says: says`In the waiting window, Ask beside Review puts questions about a PR to an agent that reads the whole diff. No one on GitHub sees them.` },
  { id: 'pr-files', places: ['pr'], says: says`${key('Cmd+F')} filters a PR's files: words keep the ones that mention them, and ${code('-word')} leaves some out.` },
  { id: 'pr-merge', places: ['pr'], says: says`A PR's merge box has GitHub's choices: how to merge, auto-merge and the merge queue.` },
  { id: 'auto-review', places: ['pr', 'waiting'], says: says`Settings › Automation can have an agent review each PR that waits on your review. By default its review is a draft only you see.` },
  { id: 'own-review', places: ['pr', 'session'], says: says`Settings › Automation can have an agent on another model review the PRs your sessions open, and hand its review back to the session that opened each.` },
  { id: 'pr-draft', places: ['pr', 'waiting'], says: says`A PR's Review tab shows your draft review as it will go. Edit or remove its comments there, then Approve or Comment to submit it.` },
  { id: 'pr-links', places: ['session', 'pr'], says: says`Point at a PR linked in a conversation to see where it stands: its review, checks and the sessions that worked on it.` },
  { id: 'quick-tab', places: QUICK_PANEL, says: says`${key('Tab')} on a session answers it in the waiting window; ${key('Alt+Enter')} opens it where it runs.` },
  { id: 'quick-start', places: QUICK_PANEL, says: says`${key('Cmd+Enter')} always starts a new session, with what you typed as its prompt.` },
  { id: 'quick-recall', places: QUICK_PANEL, says: says`${key('Up')} brings back what you typed before closing, then the prompts you sent before that.` },
  { id: 'quick-rows', places: QUICK_PANEL, says: says`${key('Cmd+1')} to ${key('Cmd+9')} do a row without going to it, and ${key('Cmd+P')} narrows the panel to one project.` },
  { id: 'todos', places: ['board', 'quick'], says: says`A todo is work you mean to hand an agent. ${key('Enter')} on one in the quick panel gets a session ready for it.` },
  { id: 'projects', places: BOARD, says: says`${key('Cmd+P')} switches project, ${key('Alt+Cmd+1')} to ${key('Alt+Cmd+9')} go straight to one, and ${key('Alt+Cmd+0')} shows them all.` },
  { id: 'new-window', places: BOARD, says: says`${key('Shift+Cmd+N')} opens another window on the project, with a view of its own.` },
  { id: 'saved-views', places: BOARD, says: says`Narrow a view with the sidebar's filters, then save it under a name to come back to.` },
  { id: 'sidebar', places: BOARD, says: says`${key('Ctrl+Cmd+S')} folds the sidebar to a rail of the views, for more room.` },
  { id: 'canvas', places: BOARD, says: says`Canvas lays your work out in space: put each card where you'll remember it.` },
  { id: 'preview', places: BOARD, says: says`Set preview URL…, in a worktree's right-click menu, gives it a globe that opens its app, green while its server runs.` },
  { id: 'close-idle', places: BOARD, says: says`File › Close idle sessions… closes the ones that sat idle for a day or more. Each conversation is kept, to resume.` },
  { id: 'usage', places: BOARD, says: says`The bar across the top shows each account's usage. Point at one for all its limits and when they reset.` },
  { id: 'devices', places: BOARD, says: says`Link your other Macs in Settings › Devices, and their sessions show here beside yours.` },
  { id: 'zoom', places: BOARD, says: says`${key('Cmd+=')} and ${key('Cmd+-')} make the windows bigger or smaller, and ${key('Cmd+0')} puts them back.` },
  { id: 'agents', places: ['board', 'session'], says: says`Agents can run the ${code('codealong')} command to list todos, message other sessions and start work.` },
];

/** Whether a tip can show: a shortcut it names must be on and readable. */
export function tipShows(tip: Tip, shortcuts: TipShortcuts): boolean {
  return tip.says.every((p) => typeof p === 'string' || !('shortcut' in p) || shortcutLabel(shortcuts[p.shortcut]) !== null);
}

/** The tip to show at `place`: the first never shown, else the one shown longest ago. `shown` is
 *  the tips' ids, the latest shown last. None when the place has no tip that can show. */
export function nextTip(place: TipPlace, shown: readonly string[], shortcuts: TipShortcuts, tips: readonly Tip[] = TIPS): Tip | null {
  const here = tips.filter((t) => t.places.includes(place) && tipShows(t, shortcuts));
  const fresh = here.find((t) => !shown.includes(t.id));
  if (fresh) return fresh;
  let oldest: Tip | null = null;
  for (const t of here) if (!oldest || shown.indexOf(t.id) < shown.indexOf(oldest.id)) oldest = t;
  return oldest;
}

/** `shown` with `id` shown now: moved to the end, and only the ids of tips that still exist. */
export function shownNow(shown: readonly string[], id: string, tips: readonly Tip[] = TIPS): string[] {
  return [...shown.filter((x) => x !== id && tips.some((t) => t.id === x)), id];
}

/** A shortcut config.toml sets, as the keys a cap each: `Option+Cmd+/` is ⌥ ⌘ /. */
export function shortcutCaps(text: string | undefined): string[] {
  const label = shortcutLabel(text);
  if (!label) return [];
  const mods = /^[⌃⌥⇧⌘]*/.exec(label)?.[0] ?? '';
  return [...mods, label.slice(mods.length)];
}
