// The main window's keys, wherever the keyboard is in it but a dialog: what each does is
// below, by the view it's in and the row selected. The window passes everything the keys read
// or do (`MainKeys`); the listener reads it as the latest render has it.
//
// Keyboard map: the design system's Surfaces page.

import { useEffect, type Dispatch, type MutableRefObject, type RefObject, type SetStateAction } from 'react';
import type { ListRow } from './boardLists';
import { historyChord } from './history';
import { AGENT_BAY, rowItems, seenable, type WorktreeRow } from './lanes';
import { useLatest } from './latest';
import * as menus from './menus';
import type { Pins } from './pins';
import { chordPressed, queueKey, type QueueAction } from './queue';
import type { TodoItem } from './todos';
import type { Bay, Lane, ProjectSummary, Session } from './types';
import { openSwitcher, projectForDigit } from './ui/ProjectSwitcher';
import type { PrRowItem } from './ui/PrsView';
import type { SessionItem } from './ui/SessionsView';
import { zoomStep, type ZoomStep } from './zoom';

/** The chords Sessions' box hands the list: ⌥↑ ⌥↓ (with ⇧, to what waits on you), ⌃⌘ with a
 *  menu's letter or ↑ ↓, ⇧⌘F and ⌃Tab. A text field has no use of its own for any of them but
 *  ⌥↑ ⌥↓, which jump to a paragraph's start or end. */
function listChord(e: KeyboardEvent): boolean {
  const vertical = e.key === 'ArrowUp' || e.key === 'ArrowDown';
  if (e.altKey && !e.metaKey && !e.ctrlKey) return vertical;
  if (e.ctrlKey && e.metaKey && !e.altKey && !e.shiftKey) return vertical || menus.chordLetter(e.key);
  if (e.shiftKey && e.metaKey && !e.ctrlKey && !e.altKey) return e.key.toLowerCase() === 'f';
  return e.ctrlKey && !e.metaKey && !e.altKey && e.key === 'Tab';
}

/** Whether the keyboard is in a text field (other than search, which has keys of its own). */
function inField(el: Element | null, search: HTMLInputElement | null): boolean {
  if (!el || el === search) return false;
  return el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement || (el as HTMLElement).isContentEditable;
}

/** What the keys read and do, as the window has it. `V` is the window's views. */
export interface MainKeys<V extends string> {
  view: V;
  /** The views in the order ⌃Tab and v go through them. */
  views: readonly { id: V }[];
  /** A sheet that has the keyboard while it's open: Settings, the preview URL's, rename. */
  settings: object | null;
  previewFor: string | null;
  renaming: string | null;
  /** A session's panel takes the whole window. */
  panelMax: boolean;
  selected: ListRow | null;
  visible: ListRow[];
  pickedRows: TodoItem[];
  selectedLane: Lane | null;
  /** Where ⌘N starts a session. */
  startLaneId: string | null;
  /** Where ⌃` opens a terminal: the session whose conversation shows, or whose line is selected,
   *  else the worktree ⌘N starts in. */
  terminalAt: { lane: Lane; session: Session | null } | null;
  projects: ProjectSummary[];
  pinnedRows: SessionItem[];
  pinnedLaneRows: WorktreeRow[];
  pinnedPrRows: PrRowItem[];
  shownBays: { bay: Bay; rows: WorktreeRow[] }[];
  sessionPins: Pins;
  lanePins: Pins;
  prPins: Pins;
  searchRef: RefObject<HTMLInputElement | null>;
  listRef: RefObject<HTMLDivElement | null>;
  /** The canvas's own keys, which say whether they took one. */
  canvasKeys: { readonly current: (e: KeyboardEvent) => boolean };
  /** A g pressed a moment ago, for g n, g w, g t and g r. */
  gPending: MutableRefObject<boolean>;
  /** The Todo view's own keys, which say whether they took one. */
  todoKeys: { readonly current: (e: KeyboardEvent, key: string) => boolean };
  /** Back or forward through the sessions looked at, and whether the next one's box takes the cursor. */
  travel: { readonly current: (dir: 'back' | 'forward', box: boolean) => void };
  /** The selected row's menu actions, which P, A, R and the ⌃⌘ chords run. */
  actionsRef: { readonly current: { session: (lane: Lane, s: Session, pin: 'session' | 'worktree') => menus.SessionActions; lane: (lane: Lane) => menus.LaneActions } };
  openSettings: () => void;
  runQueue: (action: QueueAction, box?: boolean) => boolean;
  toggleSidebar: () => void;
  pickProject: (name: string) => void;
  startForTodos: (rows: TodoItem[]) => void;
  startForTodo: (row: TodoItem) => void;
  newSession: (at: { laneId: string | null }) => void;
  setQuery: (query: string) => void;
  setCanvasFit: Dispatch<SetStateAction<number>>;
  openRow: (row: ListRow | null) => void;
  setView: (view: V) => void;
  togglePanelMax: () => void;
  movePinnedSession: (key: string, index: number) => void;
  movePinnedLane: (id: string, index: number) => void;
  move: (delta: number) => void;
  /** Puts the cursor in the session panel's box, once the click that selected it is done. */
  focusBox: () => void;
  open: (lane: Lane | null, targetId: string | null | undefined) => void;
  copy: (text: string | null | undefined, what: string) => void;
  /** Says why a key did nothing, in the window's toast. */
  say: (text: string) => void;
  markSeen: (keys: string[]) => void;
  setSelectedKey: (key: string) => void;
  /** Zooms every board window a step (zoom.ts). */
  zoom: (step: ZoomStep) => void;
}

/** Does what a key pressed in the main window does. */
export function onMainKey<V extends string>(e: KeyboardEvent, keys: MainKeys<V>): void {
  const { view, views: VIEWS, settings, previewFor, renaming, panelMax, selected, visible, pickedRows, selectedLane, startLaneId, terminalAt, projects, pinnedRows, pinnedLaneRows, pinnedPrRows, shownBays, sessionPins, lanePins, prPins, searchRef, listRef, canvasKeys, gPending, todoKeys, travel, actionsRef, openSettings, runQueue, toggleSidebar, pickProject, startForTodos, startForTodo, newSession, setQuery, setCanvasFit, openRow, setView, togglePanelMax, movePinnedSession, movePinnedLane, move, focusBox, open, copy, say, markSeen, setSelectedKey, zoom } = keys;
  const typing = document.activeElement === searchRef.current;
  const mod = e.metaKey;
  const key = e.key.toLowerCase();
  const inBox = view === 'sessions' && !!document.activeElement?.closest('.ca-sessinsp .ca-composer');
  // Sessions takes no plain letters: a click on a row puts the cursor in the panel's box, and a
  // word typed on the list by mistake would otherwise pin, archive or rename. Its keys are
  // chords, which work from the box too (`listChord`) and in the other views as well.
  const letters = view !== 'sessions';
  // ⌃⌘, with a menu's letter or ↑ ↓.
  const chord = e.ctrlKey && mod && !e.altKey && !e.shiftKey;
  const vertical = e.key === 'ArrowUp' || e.key === 'ArrowDown';
  const toTerminal = chordPressed(menus.CHORDS.terminal ?? '', e);
  // ⌘= and ⌘- zoom the window and ⌘0 puts it back, as in VS Code, from any field or sheet. The
  // canvas has them for its own zoom wherever its keys work: not in a field or the panel beside it.
  const zoomTo = zoomStep(e);
  const canvasZooms = view === 'canvas' && !inField(document.activeElement, null) && !document.activeElement?.closest('.ca-insp');
  if (zoomTo && !canvasZooms) {
    e.preventDefault();
    zoom(zoomTo);
    return;
  }
  // Escape in a session panel's box gives the list its keys back (a click on a row put them
  // there), unless it stopped the session's turn. With the panel expanded there's no list to
  // give them to, so the window has them.
  if (e.key === 'Escape' && !e.isComposing && !e.defaultPrevented && inBox) {
    e.preventDefault();
    if (panelMax) (document.activeElement as HTMLElement).blur();
    else listRef.current?.focus();
    return;
  }
  // A text field, a todo's title or description say, keeps its keys but for the window's own ⌘
  // ones, ⌃- and ⌃`, and Sessions' box for the list's chords.
  if (inField(document.activeElement, searchRef.current) && !(inBox && listChord(e)) && !historyChord(e) && !toTerminal && !(mod && (key === 'k' || key === 'p' || e.key === ',' || ((e.code === 'BracketRight' || key === 'e') && view === 'sessions') || (e.altKey && /^Digit\d$/.test(e.code))))) return;
  if (mod && e.key === ',') {
    e.preventDefault();
    openSettings();
    return;
  }
  // Settings has the keyboard while it's open, as the preview URL and rename sheets do.
  if (settings || previewFor || renaming) return;
  // ⌃` opens a new terminal in the folder of what the window shows, as it opens VS Code's
  // terminal, from any view and any field: the session's own folder, else the worktree's.
  if (toTerminal) {
    e.preventDefault();
    const at = terminalAt;
    const act = at?.session ? actionsRef.current.session(at.lane, at.session, 'session').terminal : at ? actionsRef.current.lane(at.lane).terminal : undefined;
    if (act) act.run();
    else say(at ? `${at.lane.callsign} is on another device: a terminal opens only on this Mac.` : 'Nothing selected has a folder to open a terminal in.');
    return;
  }
  // ⌃- goes back to the session you were looking at before and ⌃⇧- forward again, from any
  // view and any field. From a session's box the cursor goes on to the next one's.
  if (historyChord(e)) {
    e.preventDefault();
    travel.current(e.shiftKey ? 'forward' : 'back', !!document.activeElement?.closest('.ca-sessinsp .ca-composer'));
    return;
  }
  // The queue's keys in Sessions (queue.ts `QUEUE_CHORDS`), from the panel's box too: ⌘] or
  // ⌥⇧↓ skips to the next session that waits on you and ⌥⇧↑ goes back, with the cursor in
  // its box; ⌘E marks a finished turn seen and goes on, the keyboard staying where it was,
  // and from a session that doesn't wait on you goes on as ⌘] does. Elsewhere ⌘E marks seen
  // as `e` does (below). In the search field the arrows stay the field's own, whatever is
  // held with them: ↑ moves and selects in its text, and ↓ gives the list the keyboard (below).
  const queueStep = view === 'sessions' && !(typing && vertical) ? queueKey(e) : null;
  if (queueStep === 'next' || queueStep === 'back' || queueStep === 'seen') {
    e.preventDefault();
    runQueue(queueStep, queueStep === 'seen' ? inBox : true);
    return;
  }
  if (mod && key === 'k') {
    e.preventDefault();
    searchRef.current?.focus();
    searchRef.current?.select();
    return;
  }
  if (mod && !e.ctrlKey && key === 'p') {
    e.preventDefault();
    openSwitcher();
    return;
  }
  if (mod && e.ctrlKey && key === 's') {
    e.preventDefault();
    toggleSidebar();
    return;
  }
  // ⌥⌘ digits switch projects. ⌥ changes the character, so they go by the key's code.
  const digit = /^Digit(\d)$/.exec(e.code);
  if (mod && e.altKey && !e.shiftKey && digit) {
    e.preventDefault();
    const name = projectForDigit(Number(digit[1]), projects);
    if (name) pickProject(name);
    return;
  }
  // ⌘⇧N is File › New window's, so it works with no window open too.
  if (mod && !e.shiftKey && key === 'n') {
    e.preventDefault();
    // On a todo, the session is for the todo; on several picked, one session for all of them.
    if (pickedRows.length > 0) startForTodos(pickedRows);
    else if (selected?.kind === 'todo') startForTodo(selected);
    else newSession({ laneId: startLaneId });
    return;
  }
  if (typing) {
    if (e.key === 'Escape') {
      setQuery('');
      searchRef.current?.blur();
    } else if (e.key === 'ArrowDown' || e.key === 'Enter') {
      e.preventDefault();
      searchRef.current?.blur();
      if (e.key === 'Enter') {
        if (view === 'canvas') setCanvasFit((n) => n + 1);
        else openRow(selected);
      }
    }
    return;
  }
  // The canvas's keys, unless the keyboard is in the panel beside it.
  if (view === 'canvas' && !document.activeElement?.closest('.ca-insp') && canvasKeys.current(e)) return;
  if (gPending.current) {
    gPending.current = false;
    const bay = ({ n: 'needs', w: 'working', t: 'unread', r: 'review' } as Record<string, Bay | undefined>)[key];
    const first =
      view === 'worktrees'
        ? [...pinnedLaneRows, ...shownBays.flatMap((b) => b.rows)].filter((r) => r.lane.bay === bay).flatMap(rowItems)[0]
        : visible.find((r) => r.kind === 'agent' && AGENT_BAY[r.session.status] === bay);
    if (first) setSelectedKey(first.key);
    return;
  }
  const lane = selectedLane;
  // ⌃Tab and ⌃⇧Tab go through the views as they go through tabs; so does v outside Sessions.
  const tab = e.key === 'Tab' && e.ctrlKey && !mod && !e.altKey;
  if (tab || (key === 'v' && !mod && letters)) {
    e.preventDefault();
    const i = VIEWS.findIndex((x) => x.id === view);
    setView(VIEWS[(i + (tab && e.shiftKey ? VIEWS.length - 1 : 1)) % VIEWS.length].id);
    return;
  }
  if (todoKeys.current(e, key)) return;
  if (mod && e.shiftKey && !e.ctrlKey && !e.altKey && key === 'f' && view === 'sessions' && selected?.kind === 'agent') {
    e.preventDefault();
    togglePanelMax();
    return;
  }
  // P, A and R run the selected row's menu actions (menus.ts `KEYS`), plain outside Sessions and
  // with ⌃⌘ anywhere, so each applies where the menu's would; ⌃⌘W closes a session's terminal
  // and ⌃⌘1 to ⌃⌘4 snooze it for one of its menu's times, which no letter alone does. P pins what the view lists: worktrees on the board, sessions
  // in Sessions. E, below, marks seen more than one session at once.
  if (((!mod && letters) || chord) && selected && selected.kind !== 'pr' && selected.kind !== 'todo') {
    const acts = selected.kind === 'agent' ? actionsRef.current.session(selected.lane, selected.session, view === 'worktrees' ? 'worktree' : 'session') : actionsRef.current.lane(selected.lane);
    const found = menus.keyed(acts, key, chord);
    if (found && found.id !== 'seen') {
      // Not typed into a field the action opens (Rename's).
      e.preventDefault();
      found.action.run();
      return;
    }
  }
  // ⌃⌘1 to ⌃⌘4 snooze, and nothing else: on a row that can't be snoozed they don't fall
  // through to ⌘1 to ⌘4, which open things.
  if (chord && /^[1-4]$/.test(key)) {
    e.preventDefault();
    return;
  }
  // P pins the PR selected in PRs, as it pins what the other views list.
  if (((!mod && letters) || chord) && key === 'p' && view === 'prs' && selected?.kind === 'pr') {
    e.preventDefault();
    prPins.toggle(selected.pr.key);
    return;
  }
  // ⌃⌘↑ ⌃⌘↓ move a pinned session, worktree or PR among the pins (a todo in Todos, above).
  if (chord && vertical) {
    e.preventDefault();
    // Insertion points: one up is the row's own index less one; one down is two past it.
    const step = (i: number) => (e.key === 'ArrowUp' ? i - 1 : i + 2);
    if (view === 'sessions' && selected?.kind === 'agent' && sessionPins.set.has(selected.session.key)) {
      movePinnedSession(selected.session.key, step(pinnedRows.findIndex((r) => r.key === selected.key)));
    } else if (view === 'worktrees' && selected && selected.kind !== 'pr' && selected.kind !== 'todo' && lanePins.set.has(selected.lane.id)) {
      movePinnedLane(selected.lane.id, step(pinnedLaneRows.findIndex((r) => r.key === selected.lane.id)));
    } else if (view === 'prs' && selected?.kind === 'pr' && prPins.set.has(selected.pr.key)) {
      prPins.move(pinnedPrRows.map((r) => r.pr.key), selected.pr.key, step(pinnedPrRows.findIndex((r) => r.key === selected.key)));
    }
    return;
  }
  if (e.key === '/') {
    e.preventDefault();
    searchRef.current?.focus();
  } else if (e.key === 'ArrowDown' || (key === 'j' && !mod && letters)) {
    // The arrows, and ⌥↑ ⌥↓ from Sessions' box, where the cursor goes on to the next one's.
    e.preventDefault();
    move(1);
    if (inBox && selected !== visible[visible.length - 1]) focusBox();
  } else if (e.key === 'ArrowUp' || (key === 'k' && !mod && letters)) {
    e.preventDefault();
    move(-1);
    if (inBox && selected !== visible[0]) focusBox();
  } else if (e.key === 'Enter' && !mod) {
    e.preventDefault();
    openRow(selected);
  } else if (e.key === 'Enter' && mod) {
    e.preventDefault();
    const primary = selected?.kind === 'agent' ? selected.session.openTarget : lane?.defaultTarget;
    const alt = lane?.targets.find(
      (t) => t.group === 'session' && !t.disabled && t.id !== primary && (selected?.kind !== 'agent' || t.id.startsWith(`s|${selected.session.key}|`)),
    );
    open(lane, alt?.id ?? 'l|editor');
  } else if (mod && key === '1') {
    e.preventDefault();
    open(lane, 'l|terminal');
  } else if (mod && key === '2') {
    e.preventDefault();
    open(lane, 'l|editor');
  } else if (mod && key === '3') {
    e.preventDefault();
    open(lane, lane?.targets.find((t) => t.id.endsWith('|claude-app') && !t.disabled)?.id);
  } else if (mod && key === '4') {
    e.preventDefault();
    open(lane, lane?.targets.find((t) => t.id.endsWith('|codex') && !t.disabled)?.id);
  } else if (mod && key === '5') {
    e.preventDefault();
    if (lane?.preview) open(lane, 'l|preview');
  } else if (mod && key === 'o') {
    e.preventDefault();
    if (selected?.kind === 'pr') openRow(selected);
    else if (lane?.pr) open(lane, 'p|github');
  } else if (mod && e.shiftKey && key === 'c') {
    e.preventDefault();
    copy(lane?.path, 'worktree path');
  } else if (mod && key === 'c' && !window.getSelection()?.toString()) {
    e.preventDefault();
    copy(lane?.branch, 'branch name');
  } else if (key === 'e' && ((!mod && letters) || (mod && !e.ctrlKey && !e.shiftKey && !e.altKey))) {
    // ⌘E too, as in Sessions (above).
    const sessions = selected?.kind === 'agent' ? [selected.session] : selected?.kind === 'todo' ? (pickedRows.length > 0 ? pickedRows : [selected]).flatMap((r) => r.sessions.map((x) => x.session)) : (lane?.sessions ?? []);
    markSeen(sessions.filter(seenable).map((s) => s.key));
  } else if (key === 'g' && !mod && letters) {
    gPending.current = true;
    window.setTimeout(() => (gPending.current = false), 1200);
  }
}

/** Listens for the main window's keys. The listener is added again whenever what the keys act on
 *  changes, as it always was: a listener a session's panel adds in the same change (its ⌘⏎, its
 *  ⌘.) keeps hearing a key before this one does. */
export function useMainKeys<V extends string>(keys: MainKeys<V>): void {
  const latest = useLatest(keys);
  const { visible, selected, pickedRows, selectedLane, startLaneId, terminalAt, move, open, openRow, copy, say, settings, previewFor, renaming, openSettings, newSession, startForTodo, startForTodos, view, setView, runQueue, markSeen, sessionPins, lanePins, prPins, pinnedRows, pinnedLaneRows, pinnedPrRows, shownBays, movePinnedSession, movePinnedLane, projects, pickProject, toggleSidebar, panelMax, togglePanelMax, actionsRef, todoKeys, travel } = keys;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => onMainKey(e, latest.current);
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [latest, visible, selected, pickedRows, selectedLane, startLaneId, terminalAt, move, open, openRow, copy, say, settings, previewFor, renaming, openSettings, newSession, startForTodo, startForTodos, view, setView, runQueue, markSeen, sessionPins, lanePins, prPins, pinnedRows, pinnedLaneRows, pinnedPrRows, shownBays, movePinnedSession, movePinnedLane, projects, pickProject, toggleSidebar, panelMax, togglePanelMax, actionsRef, todoKeys, travel]);
}
