// The main window's keys, wherever the keyboard is in it but a dialog: what each does is
// below, by the view it's in and the row selected. The window passes everything the keys read
// or do (`MainKeys`); the listener reads it as the latest render has it.
//
// Keyboard map: the design system's Surfaces page.

import { useEffect, type Dispatch, type MutableRefObject, type RefObject, type SetStateAction } from 'react';
import type { ListRow } from './boardLists';
import type { MenuAction } from './contextMenu';
import { findsAt } from './findKeys';
import { historyChord } from './history';
import { AGENT_BAY, rowItems, seenable, type WorktreeRow } from './lanes';
import { useLatest } from './latest';
import * as menus from './menus';
import type { Pins } from './pins';
import { chordPressed, queueKey, type QueueAction } from './queue';
import type { TodoItem } from './todos';
import type { ListPicksKeys } from './useListPicks';
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

/** Each view's chord: ⇧⌘ and the view's first letter, from any view and any field. Canvas has B
 *  (board) because clipboard managers take ⇧⌘C for themselves (Maccy's popup ships on it). The
 *  sidebar shows a view's chord as you point at it. */
export const VIEW_CHORDS: Record<string, string> = { sessions: 'Shift+Cmd+S', worktrees: 'Shift+Cmd+W', prs: 'Shift+Cmd+P', todo: 'Shift+Cmd+T', canvas: 'Shift+Cmd+B' };

/** The view a key event goes to (`VIEW_CHORDS`), or none. */
export function viewOfKey<V extends string>(e: Pick<KeyboardEvent, 'key' | 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>, views: readonly { id: V }[]): V | null {
  return views.find((v) => chordPressed(VIEW_CHORDS[v.id] ?? '', e))?.id ?? null;
}

/** Whether the keyboard is in a text field (other than search, which has keys of its own). */
function inField(el: Element | null, search: HTMLInputElement | null): boolean {
  if (!el || el === search) return false;
  return el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement || (el as HTMLElement).isContentEditable;
}

/** What the keys read and do, as the window has it. `V` is the window's views. */
export interface MainKeys<V extends string> {
  view: V;
  /** The views in the order ⌃Tab and v go through them; each has a chord (`VIEW_CHORDS`). */
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
  /** Rows picked together in the view's list (useListPicks.ts), and what the keys do to them;
   *  null where the view has no list. */
  listPicks: ListPicksKeys | null;
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
  /** The list's find (ui/ListBar.tsx), while it shows. */
  searchRef: RefObject<HTMLInputElement | null>;
  /** The board, where ⌘F opens the list's find (`data-find="window"`, findKeys.ts). */
  boardRef: RefObject<HTMLElement | null>;
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
  /** Opens the list's find, or gives it the keyboard back, with its words selected. */
  openFind: () => void;
  setQuery: (query: string) => void;
  setCanvasFit: Dispatch<SetStateAction<number>>;
  openRow: (row: ListRow | null) => void;
  setView: (view: V) => void;
  togglePanelMax: () => void;
  /** A PR's panel takes the whole window, or gives the list its place back. */
  togglePrPanelMax: () => void;
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
  const { view, views: VIEWS, settings, previewFor, renaming, panelMax, selected, visible, pickedRows, listPicks, selectedLane, startLaneId, terminalAt, projects, pinnedRows, pinnedLaneRows, pinnedPrRows, shownBays, sessionPins, lanePins, prPins, searchRef, boardRef, listRef, canvasKeys, gPending, todoKeys, travel, actionsRef, openSettings, runQueue, toggleSidebar, pickProject, startForTodos, startForTodo, newSession, openFind, setQuery, setCanvasFit, openRow, setView, togglePanelMax, togglePrPanelMax, movePinnedSession, movePinnedLane, move, focusBox, open, copy, say, markSeen, setSelectedKey, zoom } = keys;
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
  // Several rows picked in the view's list: the keys that act on one act on all of them, by the
  // names one row's actions have.
  const picking = !!listPicks && listPicks.count > 0;
  const picked = (name: string): MenuAction | undefined => {
    const acts = picking ? listPicks?.actions() : null;
    const it = acts ? (acts as unknown as Record<string, MenuAction | { items: unknown } | undefined>)[name] : undefined;
    return it && !('items' in it) ? it : undefined;
  };
  const vertical = e.key === 'ArrowUp' || e.key === 'ArrowDown';
  const toTerminal = chordPressed(menus.CHORDS.terminal ?? '', e);
  const toView = viewOfKey(e, VIEWS);
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
  // ⇧⌘F from a PR panel's own fields (its review box, the files' filter), which have no use for it:
  // in PRs, and in Sessions, where a session's PR shows in its panel's place.
  const prMaxKey = (view === 'prs' || view === 'sessions') && mod && e.shiftKey && !e.ctrlKey && !e.altKey && key === 'f' && !!document.activeElement?.closest('.ca-prpanel');
  // ⌘F, and ⌥⌘F, which goes by the key's code, as ⌥ changes the character.
  const findKey = mod && !e.ctrlKey && !e.shiftKey && (e.altKey ? e.code === 'KeyF' : key === 'f');
  // A text field, a todo's title or description say, keeps its keys but for the window's own ⌘
  // ones, ⌃-, ⌃`, a view's ⌃ and digit, and Sessions' box for the list's chords.
  if (inField(document.activeElement, searchRef.current) && !(inBox && listChord(e)) && !prMaxKey && !findKey && !historyChord(e) && !toTerminal && !toView && !(mod && (key === 'k' || key === 'p' || e.key === ',' || ((e.code === 'BracketRight' || e.code === 'BracketLeft' || key === 'e') && view === 'sessions') || (e.altKey && /^Digit\d$/.test(e.code))))) return;
  if (mod && e.key === ',') {
    e.preventDefault();
    openSettings();
    return;
  }
  // Settings has the keyboard while it's open, as the preview URL and rename sheets do.
  if (settings || previewFor || renaming) return;
  // A button with the keyboard (a row's PR chip, reached by Tab) takes its own Enter, before the
  // list or a view's keys (the canvas's, Todo's) can open or start what's selected.
  if (e.key === 'Enter' && !mod && !e.ctrlKey && !e.altKey && e.target instanceof HTMLButtonElement) return;
  // ⇧⌘ and a view's letter goes straight to it, from any view and any field (`VIEW_CHORDS`).
  if (toView) {
    e.preventDefault();
    setView(toView);
    return;
  }
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
  // ⇧⌘E marks the selected session done (menus.ts `CHORDS`), in any view, and from Sessions' box
  // as ⌘E beside it marks seen there. Done goes on to the next that waits, as ⌘E does.
  if (chordPressed(menus.CHORDS.done ?? '', e)) {
    e.preventDefault();
    if (picking) {
      const act = picked('done');
      if (act) act.run();
      else if (view === 'sessions') say('They’re archived already.');
      return;
    }
    if (selected?.kind !== 'agent') return;
    const act = actionsRef.current.session(selected.lane, selected.session, 'session').done;
    if (act) act.run();
    else say('It’s archived already.');
    return;
  }
  // The queue's keys in Sessions (queue.ts `QUEUE_CHORDS`), from the panel's box too: ⌘] or
  // ⌥⇧↓ skips to the next session that waits on you and ⌘[ or ⌥⇧↑ goes back, with the cursor in
  // its box; ⌘E marks a finished turn seen and goes on, the keyboard staying where it was,
  // and from a session that doesn't wait on you goes on as ⌘] does. Elsewhere ⌘E marks seen
  // as `e` does (below). In the search field the arrows stay the field's own, whatever is
  // held with them: ↑ moves and selects in its text, and ↓ gives the list the keyboard (below).
  const queueStep = view === 'sessions' && !(typing && vertical) ? queueKey(e) : null;
  // With several picked, ⌘E marks them all seen, and stays on them.
  if (queueStep === 'seen' && picking) {
    e.preventDefault();
    const act = picked('seen');
    if (act) act.run();
    else say('None of them has a turn to mark seen.');
    return;
  }
  if (queueStep === 'next' || queueStep === 'back' || queueStep === 'seen') {
    e.preventDefault();
    runQueue(queueStep, queueStep === 'seen' ? inBox : true);
    return;
  }
  // ⌘F opens the list's find from wherever nothing else finds, the list say, and a session's or
  // a PR's panel keeps it for its own find (findKeys.ts); ⌥⌘F opens the list's from anywhere,
  // as in Mail and Notes.
  if (findKey) {
    if (!e.altKey && !findsAt(boardRef.current)) return;
    e.preventDefault();
    openFind();
    return;
  }
  // ⌘K opens the quick panel, to find a session, a todo or a PR, or to start one where ⌘N would.
  if (mod && !e.ctrlKey && !e.altKey && !e.shiftKey && key === 'k') {
    e.preventDefault();
    newSession({ laneId: startLaneId });
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
  // ⌥⌘ digits switch projects. ⌥ changes the character, so they go by the key's code. Not with ⌃
  // as well, as in the quick panel.
  const digit = /^Digit(\d)$/.exec(e.code);
  if (mod && e.altKey && !e.ctrlKey && !e.shiftKey && digit) {
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
  // ⌘A picks every row listed, from the list (in a panel it's the page's), and Esc goes back to
  // the one the keyboard is on.
  const onList = !document.activeElement || document.activeElement === document.body || !!document.activeElement.closest('.ca-board-list');
  if (listPicks && mod && !e.ctrlKey && !e.altKey && !e.shiftKey && key === 'a' && onList) {
    e.preventDefault();
    listPicks.all();
    return;
  }
  if (e.key === 'Escape' && picking) {
    e.preventDefault();
    listPicks?.clear();
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
  // ⇧↑ ⇧↓ pick the row above or below too, as ⇧-click does (the Todo view's own keys, above).
  if (listPicks && e.shiftKey && vertical && !mod && !e.ctrlKey && !e.altKey && selected) {
    e.preventDefault();
    listPicks.step(e.key === 'ArrowUp' ? -1 : 1);
    return;
  }
  // A menu's letter (outside Sessions) or its ⌃⌘ chord acts on all the rows picked: what doesn't
  // apply to any of them does nothing, and what's done to one at a time (rename) waits for one.
  // E marks seen below, and ⌘O opens PRs.
  const acts = picking ? listPicks?.actions() : null;
  if (acts && ((!mod && letters && key !== 'e') || (chord && !vertical))) {
    const found = menus.keyed(acts, key, chord);
    if (found) {
      e.preventDefault();
      found.action.run();
      return;
    }
    if (key === 'r') {
      e.preventDefault();
      say('Rename one at a time: Esc goes back to one.');
      return;
    }
    // Nor does one row's action run on the row the keyboard is on.
    if (chord || Object.values(menus.KEYS).includes(key.toUpperCase())) {
      e.preventDefault();
      return;
    }
  }
  if (mod && e.shiftKey && !e.ctrlKey && !e.altKey && key === 'f' && view === 'sessions' && selected?.kind === 'agent') {
    e.preventDefault();
    togglePanelMax();
    return;
  }
  if (mod && e.shiftKey && !e.ctrlKey && !e.altKey && key === 'f' && view === 'prs' && selected?.kind === 'pr' && !picking) {
    e.preventDefault();
    togglePrPanelMax();
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
    openFind();
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
    const github = picked('github');
    if (github) github.run();
    else if (selected?.kind === 'pr') openRow(selected);
    else if (lane?.pr) open(lane, 'p|github');
  } else if (mod && e.altKey && !e.shiftKey && !e.ctrlKey && e.code === 'KeyC') {
    // ⌥⌘C copies the path, as Finder's Copy as pathname does (⇧⌘C is often a clipboard
    // manager's). By the key's code: ⌥ changes the character.
    e.preventDefault();
    copy(lane?.path, 'worktree path');
  } else if (mod && !e.shiftKey && !e.altKey && key === 'c' && !window.getSelection()?.toString()) {
    e.preventDefault();
    copy(lane?.branch, 'branch name');
  } else if (key === 'e' && ((!mod && letters) || (mod && !e.ctrlKey && !e.shiftKey && !e.altKey)) && picking) {
    // With several picked, all of them.
    const act = picked('seen');
    if (act) act.run();
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
  const { visible, selected, pickedRows, listPicks, selectedLane, startLaneId, terminalAt, move, open, openRow, copy, say, settings, previewFor, renaming, openSettings, newSession, startForTodo, startForTodos, view, setView, runQueue, markSeen, sessionPins, lanePins, prPins, pinnedRows, pinnedLaneRows, pinnedPrRows, shownBays, movePinnedSession, movePinnedLane, projects, pickProject, toggleSidebar, panelMax, togglePanelMax, togglePrPanelMax, actionsRef, todoKeys, travel } = keys;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => onMainKey(e, latest.current);
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [latest, visible, selected, pickedRows, listPicks?.count, selectedLane, startLaneId, terminalAt, move, open, openRow, copy, say, settings, previewFor, renaming, openSettings, newSession, startForTodo, startForTodos, view, setView, runQueue, markSeen, sessionPins, lanePins, prPins, pinnedRows, pinnedLaneRows, pinnedPrRows, shownBays, movePinnedSession, movePinnedLane, projects, pickProject, toggleSidebar, panelMax, togglePanelMax, togglePrPanelMax, actionsRef, todoKeys, travel]);
}
