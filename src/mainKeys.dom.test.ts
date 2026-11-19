// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { engine, mountHook, press, type, type Mounted } from './testing/dom';
import { prRowOf, sessionItemOf } from './boardLists';
import type { MenuGroup } from './contextMenu';
import type { LaneItem } from './lanes';
import { PLACE_CHORDS, useMainKeys, useViewKeys, VIEW_CHORDS, type MainKeys } from './mainKeys';
import * as menus from './menus';
import type { Pins } from './pins';
import { setPlatform, type Platform } from './platform';
import { QUEUE_CHORDS } from './queue';
import type { Report } from './report';
import { SNOOZE_CHORDS } from './snooze';
import * as fx from './testing/fixtures';
import { todoItem } from './todos';
import type { Lane, Session } from './types';

// The main window's keyboard, as a key pressed on its page comes to it (mainKeys.ts `onMainKey`,
// through `useMainKeys`): the window here is everything the keys read and do, each thing it's
// asked to do written down in words, and a row's actions are the menus' own (menus.ts), each
// saying its name in place of doing it. Every test is pressed twice, as macOS has the keys and as
// Linux does (platform.ts): the chords are written once, for macOS, and `press` sends what the
// platform's keyboard would. mainKeys.test.ts and platform.test.ts hold what each chord is there.

const VIEWS = [{ id: 'sessions' }, { id: 'prs' }, { id: 'todo' }, { id: 'schedules' }, { id: 'kanban' }, { id: 'canvas' }] as const;
type View = (typeof VIEWS)[number]['id'];

const report: Report = { error: () => {}, info: () => {} };

// Kestrel runs in a terminal in Northwind's app worktree, its last turn seen. It opened the
// worktree's PR, which is ready to merge, so every chord has something to do on it.
const KESTREL = 'claude:northwind:kestrel';
const kestrel = (over: Partial<Session> = {}): Session => fx.session({ key: KESTREL, title: 'Kestrel', status: 'idle', live: true, pid: 4242, ...over });
// Wren's turn ended and hasn't been seen; it isn't running, and no PR is its own.
const WREN = 'claude:northwind:wren';
const wren = (over: Partial<Session> = {}): Session => fx.session({ key: WREN, title: 'Wren', status: 'unread', ...over });
const app = (sessions: Session[] = [kestrel()], over: Partial<Lane> = {}): Lane =>
  fx.lane({
    id: '/r/northwind/app',
    project: 'Northwind',
    repoSlug: 'northwind/app',
    branch: 'kestrel-login',
    sessions,
    pr: fx.pr({ number: 12, url: 'https://github.com/northwind/app/pull/12' }),
    preview: { url: 'http://localhost:5173', set: false, port: null, server: null, chromeProfileSet: false },
    defaultTarget: 'l|terminal',
    targets: [fx.target({ id: 'l|terminal' }), fx.target({ id: 'l|editor' }), fx.target({ id: `s|${KESTREL}|focus` }), fx.target({ id: `s|${KESTREL}|claude-app` }), fx.target({ id: 'l|codex' })],
    ...over,
  });
const READY = fx.prItem({ repoSlug: 'northwind/app', number: 12, mine: true, merge: 'clean', primary: KESTREL });

interface Window {
  /** What the window was asked to do, in the order the keys asked. */
  did: string[];
}

// One main window on the page at a time: opening another closes the one before, so a key is
// heard once.
let mounted: Mounted<void, void> | null = null;
afterEach(() => {
  mounted = null;
});

/** What a window's lists hold besides the row selected: the keys of what's pinned in each, and
 *  the sessions picked together. */
interface Lists {
  pinnedSessions?: string[];
  pinnedLanes?: string[];
  picked?: Session[];
}

/** The main window with `over` in place of what it has with nothing selected, in Sessions. */
function mainWindow(over: Partial<MainKeys<View>> = {}, lists: Lists = {}): Window {
  const did: string[] = [];
  /** A row's actions, each saying whose it is and its name instead of running: a snooze by its
   *  place among its menu's times. */
  const named = <A extends object>(whose: string, actions: A): A => {
    const said = (name: string, it: unknown): unknown => {
      if (!it || typeof it !== 'object') return it;
      if ('items' in it) return { ...it, items: (it as MenuGroup).items.map((x, i) => (x === 'separator' ? x : said(`${name} ${i + 1}`, x))) };
      return 'run' in it ? { ...it, run: () => void did.push(`${whose}: ${name}`) } : it;
    };
    return Object.fromEntries(Object.entries(actions).map(([name, it]) => [name, said(name, it)])) as A;
  };
  const pins = (what: string, keys: string[] = []): Pins => ({
    keys,
    set: new Set(keys),
    toggle: (key) => void did.push(`${what}: toggle ${key}`),
    move: (_, key, index) => void did.push(`${what}: move ${key} to ${index}`),
    place: () => {},
    remove: () => {},
    pinAll: () => {},
  });
  // What the main window can do to a session and a worktree, as far as its keys go (App.tsx).
  const sessionCan: menus.SessionCan = { report, open: () => {}, terminal: 'Ghostty', keys: 'chords', rename: () => {}, pin: { label: 'Pin session', run: () => {} }, mergeDone: { prs: [READY], ask: () => true } };
  const laneCan: menus.LaneCan = { report, open: () => {}, keys: 'chords', pin: { label: 'Pin worktree', run: () => {} } };
  // The rows picked together, and what the keys do to them all (useListPicks.ts).
  const picked = (lists.picked ?? []).map((session) => ({ lane: app(lists.picked), session }));
  const listPicks: MainKeys<View>['listPicks'] = lists.picked
    ? {
        count: picked.length,
        step: (dir) => void did.push(`pick the row ${dir > 0 ? 'below' : 'above'} too`),
        all: () => void did.push('pick every row'),
        clear: () => void did.push('back to one row'),
        actions: () => named('the picked', menus.sessionsActions(picked, { report, keys: 'chords', pin: { label: 'Pin sessions', run: () => {} } })),
      }
    : null;
  const keys: MainKeys<View> = {
    view: 'sessions',
    views: VIEWS,
    settings: null,
    previewFor: null,
    renaming: null,
    panelMax: false,
    selected: null,
    visible: [],
    pickedRows: [],
    listPicks,
    selectedLane: null,
    startLaneId: null,
    terminalAt: null,
    projects: [],
    pinnedRows: [],
    pinnedLanes: lists.pinnedLanes ?? [],
    pinnedPrRows: [],
    sessionPins: pins('session pins', lists.pinnedSessions),
    lanePins: pins('worktree pins', lists.pinnedLanes),
    prPins: pins('PR pins'),
    searchRef: { current: null },
    listRef: { current: null },
    canvasKeys: { current: () => false },
    kanbanKeys: { current: () => false },
    gPending: { current: false },
    todoKeys: { current: () => false },
    travel: { current: (dir, box) => void did.push(`history ${dir}${box ? ', into the box' : ''}`) },
    actionsRef: { current: { session: (l, s) => named(s.title, menus.sessionActions(l, s, sessionCan)), lane: (l) => named(l.callsign, menus.laneActions(l, laneCan)) } },
    openSettings: () => void did.push('open Settings'),
    runQueue: (action, box) => {
      did.push(`queue ${action}${box ? ', into the box' : ''}`);
      return true;
    },
    stepView: {
      current: (action) => {
        did.push(`view’s queue ${action}`);
        return true;
      },
    },
    toggleSidebar: () => void did.push('toggle the sidebar'),
    pickProject: (name) => void did.push(`project ${name}`),
    startForTodos: (rows) => void did.push(`start a session for todos ${rows.map((r) => r.todo.id).join(', ')}`),
    startForTodo: (row) => void did.push(`start a session for todo ${row.todo.id}`),
    newSession: ({ laneId }) => void did.push(`quick panel in ${laneId ?? 'no worktree'}`),
    openFind: () => void did.push('filter the list'),
    setQuery: (query) => void did.push(`filter by “${query}”`),
    setCanvasFit: () => void did.push('fit the canvas'),
    openRow: (row) => void did.push(`open row ${row?.key ?? 'none'}`),
    setView: (view) => void did.push(`view ${view}`),
    togglePanelMax: () => void did.push('session panel takes the window'),
    togglePrPanelMax: () => void did.push('PR panel takes the window'),
    movePinnedSession: (key, index) => void did.push(`move pinned session ${key} to ${index}`),
    movePinnedLane: (id, index) => void did.push(`move pinned worktree ${id} to ${index}`),
    move: (delta) => void did.push(`move ${delta}`),
    focusBox: () => void did.push('cursor into the box'),
    open: (lane, target) => void did.push(`open ${target ?? 'nothing'} of ${lane?.callsign ?? 'no worktree'}`),
    copy: (text, what) => void did.push(`copy ${what} ${text}`),
    say: (text) => void did.push(`say “${text}”`),
    markSeen: (seen) => void did.push(`mark seen ${seen.join(', ') || 'nothing'}`),
    setSelectedKey: (key) => void did.push(`select ${key}`),
    zoom: (step) => void did.push(`zoom ${step}`),
    reload: () => void did.push('reload'),
    ...over,
  };
  mounted?.unmount();
  mounted = mountHook(() => useMainKeys(keys));
  return { did };
}

/** The window with a session's row selected in the view, where ⌃` opens its terminal. */
function onSession(s: Session = kestrel(), over: Partial<MainKeys<View>> = {}, lists?: Lists): Window {
  const lane = over.selectedLane ?? app([s]);
  const row = sessionItemOf({ lane, session: s });
  return mainWindow({ selected: row, visible: [row], selectedLane: lane, startLaneId: lane.id, terminalAt: { lane, session: s }, ...over }, lists);
}

/** The window with a worktree's heading selected, in Sessions grouped by worktree. */
function onLane(lane: Lane, over: Partial<MainKeys<View>> = {}, lists?: Lists): Window {
  const row: LaneItem = { kind: 'lane', key: lane.id, lane };
  return mainWindow({ selected: row, visible: [row], selectedLane: lane, startLaneId: lane.id, terminalAt: { lane, session: null }, ...over }, lists);
}

/** Puts the keyboard in a text field of the page: Sessions' box under a session's panel, or any
 *  other (a todo's title). */
function field(where: 'box' | 'other'): HTMLTextAreaElement {
  const el = document.createElement('textarea');
  const holder = document.body.appendChild(document.createElement('div'));
  if (where === 'box') {
    holder.className = 'ca-sessinsp';
    const composer = holder.appendChild(document.createElement('div'));
    composer.className = 'ca-composer';
    composer.append(el);
  } else holder.append(el);
  el.focus();
  return el;
}

/** What each of `chords` asks of a window opened anew for it, by the chord's name. A key the
 *  window acts on isn't also the page's (⌘W doesn't close it, ⌘R is the reload asked for), so
 *  one it leaves alone says so. */
function each(chords: Record<string, string | undefined>, open: () => Window): Record<string, string[]> {
  return Object.fromEntries(
    Object.entries(chords).map(([name, chord]) => {
      const w = open();
      const e = press(chord ?? '');
      return [name, e.defaultPrevented ? w.did : [...w.did, 'left to the page']];
    }),
  );
}

const PLATFORMS: Platform[] = ['mac', 'linux'];

describe.each(PLATFORMS)('a session’s chords in the main window, as %s presses them', (platform) => {
  it('each run their action on the session selected in Sessions (menus.ts CHORDS)', () => {
    setPlatform(platform);
    expect(each(menus.CHORDS, () => onSession())).toEqual({
      // ⌘E is the queue's there: it marks seen and goes on (useWaitingQueue.ts).
      seen: ['queue seen'],
      done: ['Kestrel: done'],
      mergeDone: ['Kestrel: mergeDone'],
      unread: ['Kestrel: unread'],
      pin: ['Kestrel: pin'],
      raise: ['Kestrel: raise'],
      archive: ['Kestrel: archive'],
      rename: ['Kestrel: rename'],
      close: ['Kestrel: close'],
      terminal: ['Kestrel: terminal'],
      ownTerminal: ['Kestrel: terminal'],
    });
  });

  it('work in every view that lists sessions, where ⌘E marks the one selected seen', () => {
    setPlatform(platform);
    const w = onSession(wren(), { view: 'kanban' });
    for (const chord of ['Cmd+E', 'Shift+Cmd+E', 'Ctrl+Cmd+A', 'Ctrl+Cmd+0']) press(chord);
    expect(w.did).toEqual([`mark seen ${WREN}`, 'Wren: done', 'Wren: archive', 'Wren: raise']);
  });

  it('snooze it for one of its menu’s times with ⌃⌘ and a digit', () => {
    setPlatform(platform);
    const w = onSession();
    for (const chord of SNOOZE_CHORDS.slice(0, 3)) press(chord);
    expect(w.did).toEqual(['Kestrel: snooze 1', 'Kestrel: snooze 2', 'Kestrel: snooze 3']);
  });

  it('keep ⌃⌘ and a digit for snoozing: with nothing to snooze, it doesn’t open what ⌘ and the digit opens', () => {
    setPlatform(platform);
    // An archived session has no Snooze menu.
    const w = onSession(kestrel({ archived: true, live: false, pid: null }));
    expect(press('Ctrl+Cmd+1').defaultPrevented).toBe(true);
    expect(press('Ctrl+Cmd+8').defaultPrevented).toBe(true);
    expect(w.did).toEqual([]);
  });

  it('work from Sessions’ box, where a letter alone is typed', () => {
    setPlatform(platform);
    const w = onSession();
    field('box');
    expect(press('a').defaultPrevented).toBe(false);
    expect(press('Shift+E').defaultPrevented).toBe(false);
    for (const chord of ['Ctrl+Cmd+A', 'Shift+Cmd+E', 'Shift+Cmd+M', 'Cmd+E', 'Ctrl+`', 'Ctrl+Cmd+2']) press(chord);
    // ⌘E from the box leaves the cursor in the next one's.
    expect(w.did).toEqual(['Kestrel: archive', 'Kestrel: done', 'Kestrel: mergeDone', 'queue seen, into the box', 'Kestrel: terminal', 'Kestrel: snooze 2']);
  });

  it('are the field’s own in any other text field, but the ones that work from every field', () => {
    setPlatform(platform);
    const w = onSession();
    field('other');
    for (const chord of ['Ctrl+Cmd+A', 'Ctrl+Cmd+R', 'Ctrl+Cmd+0', 'Ctrl+Cmd+2', 'a', 'Down']) expect(press(chord).defaultPrevented, chord).toBe(false);
    expect(w.did).toEqual([]);
    for (const chord of ['Ctrl+`', 'Ctrl+-', 'Ctrl+Shift+-', 'Cmd+K', 'Cmd+,', 'Alt+Cmd+F', 'Cmd+=', 'Cmd+R']) press(chord);
    expect(w.did).toEqual(['Kestrel: terminal', 'history back', 'history forward', 'quick panel in /r/northwind/app', 'open Settings', 'filter the list', 'zoom in', 'reload']);
  });

  // mainKeys.ts says these three are let through for Sessions' box; what lets them through asks
  // only that the view is Sessions, so they act from any of its text fields: a session's PR shows
  // in its panel's place there, with a review box of its own. Held here as it is.
  it('take ⌘E, ⇧⌘E and ⇧⌘M from every text field in Sessions, and from a todo’s title in no view', () => {
    setPlatform(platform);
    const w = onSession();
    field('other');
    for (const chord of ['Cmd+E', 'Shift+Cmd+E', 'Shift+Cmd+M']) press(chord);
    expect(w.did).toEqual(['queue seen', 'Kestrel: done', 'Kestrel: mergeDone']);
    // A todo's title keeps them.
    const todo = onSession(kestrel(), { view: 'todo' });
    for (const chord of ['Cmd+E', 'Shift+Cmd+E', 'Shift+Cmd+M']) expect(press(chord).defaultPrevented, chord).toBe(false);
    expect(todo.did).toEqual([]);
  });

  // The list's find isn't a text field to the keys (`inField` leaves it out, for the find's own
  // keys further down), and ⇧⌘E and ⇧⌘M are taken before those are reached: so with words being
  // typed into a view's find, both are still the window's, in every view. They act where the row
  // selected is a session; on a todo, a PR or no row the keystroke is taken and nothing is done.
  // ⌘E is taken only in Sessions, where it's the queue's; elsewhere the find's own keys end it.
  // Held here as it is.
  it('take ⇧⌘E and ⇧⌘M from the list’s find in every view, to act on a session selected and on nothing else, and ⌘E from it only in Sessions', () => {
    setPlatform(platform);
    const find = document.body.appendChild(document.createElement('input'));
    find.focus();
    const kanban = onSession(kestrel(), { view: 'kanban', searchRef: { current: find } });
    expect(press('Cmd+E').defaultPrevented).toBe(false);
    for (const chord of ['Shift+Cmd+E', 'Shift+Cmd+M']) expect(press(chord).defaultPrevented, chord).toBe(true);
    expect(kanban.did).toEqual(['Kestrel: done', 'Kestrel: mergeDone']);
    // A todo, a PR that's ready to merge, and no row at all: taken, and nothing done or said.
    const todo = todoItem(fx.todo(), new Map(), new Map(), new Map());
    const pr = prRowOf(READY, new Map(), new Map());
    for (const [view, selected] of [['todo', todo], ['prs', pr], ['prs', null]] as const) {
      const w = mainWindow({ view, selected, searchRef: { current: find } });
      for (const chord of ['Shift+Cmd+E', 'Shift+Cmd+M']) expect(press(chord).defaultPrevented, `${chord} on ${selected?.kind ?? 'no row'}`).toBe(true);
      expect(w.did, selected?.kind ?? 'no row').toEqual([]);
    }
    const sessions = onSession(kestrel(), { searchRef: { current: find } });
    for (const chord of ['Cmd+E', 'Shift+Cmd+E', 'Shift+Cmd+M']) press(chord);
    expect(sessions.did).toEqual(['queue seen', 'Kestrel: done', 'Kestrel: mergeDone']);
    // What's typed there is the find's, a menu's chord too; Escape clears it and ↓ leaves it for
    // the list, each giving the keyboard up.
    for (const chord of ['a', 'Ctrl+Cmd+A']) expect(press(chord).defaultPrevented, chord).toBe(false);
    press('Esc');
    expect(document.activeElement).not.toBe(find);
    find.focus();
    press('Down');
    expect(document.activeElement).not.toBe(find);
    expect(sessions.did.slice(3)).toEqual(['filter by “”']);
  });

  it('do nothing where the action doesn’t apply, and say why for the ones with no letter', () => {
    setPlatform(platform);
    // Wren's turn isn't seen yet, so there's nothing to mark unread, and it has no terminal.
    const w = onSession(wren());
    for (const chord of ['Ctrl+Cmd+U', 'Ctrl+Cmd+W']) press(chord);
    expect(w.did).toEqual([]);
    press('Shift+Cmd+M');
    expect(w.did).toEqual(['say “Nothing to merge: Merge and done… is for a PR’s primary session once the PR is ready to merge.”']);
    const archived = onSession(kestrel({ archived: true, live: false, pid: null }));
    press('Shift+Cmd+E');
    expect(archived.did).toEqual(['say “It’s archived already.”']);
  });

  it('say where a terminal can’t open: with nothing selected, or a worktree on another device', () => {
    setPlatform(platform);
    const nothing = mainWindow();
    press('Ctrl+`');
    expect(nothing.did).toEqual(['say “Nothing selected has a folder to open a terminal in.”']);
    const w = onLane(app([], { callsign: 'atlas', device: { name: 'Mac mini', short: 'a1b2c3d4e5', color: 'blue', online: true, lastSeen: null } }));
    press('Ctrl+`');
    expect(w.did).toEqual(['say “atlas is on another device: a terminal opens only on this machine.”']);
  });

  it('act on every row picked, by the names one row’s actions have', () => {
    setPlatform(platform);
    // Kestrel and Wren, picked together: between them there's something for every one to do.
    const w = onSession(kestrel(), {}, { picked: [kestrel(), wren()] });
    for (const chord of ['Shift+Cmd+E', 'Cmd+E', 'Ctrl+Cmd+A', 'Ctrl+Cmd+P', 'Ctrl+Cmd+U', 'Ctrl+Cmd+W', 'Ctrl+Cmd+1']) press(chord);
    expect(w.did).toEqual(['the picked: done', 'the picked: seen', 'the picked: archive', 'the picked: pin', 'the picked: unread', 'the picked: close', 'the picked: snooze 1']);
  });

  it('do to one row at a time what can’t be done to several, and never to the row the keyboard is on alone', () => {
    setPlatform(platform);
    // Two that aren't running: no terminal to close, and nothing merges for several at once.
    const w = onSession(wren(), {}, { picked: [wren(), wren({ key: 'claude:northwind:dune', title: 'Dune' })] });
    for (const chord of ['Ctrl+Cmd+R', 'Ctrl+Cmd+W', 'Shift+Cmd+M']) expect(press(chord).defaultPrevented, chord).toBe(true);
    expect(w.did).toEqual(['say “Rename one at a time: Esc goes back to one.”']);
  });

  it('pick the row below too with ⇧↓, every row with ⌘A, and go back to one with Escape', () => {
    setPlatform(platform);
    const w = onSession(kestrel(), {}, { picked: [kestrel(), wren()] });
    for (const chord of ['Shift+Down', 'Shift+Up', 'Cmd+A', 'Esc']) press(chord);
    expect(w.did).toEqual(['pick the row below too', 'pick the row above too', 'pick every row', 'back to one row']);
  });

  it('act on a worktree’s heading too: ⌘E marks its sessions seen, ⌃⌘P pins it, ⌃` opens a terminal in it', () => {
    setPlatform(platform);
    const w = onLane(app([kestrel(), wren()]));
    for (const chord of ['Cmd+E', 'Ctrl+Cmd+P', 'Ctrl+`']) press(chord);
    expect(w.did).toEqual([`mark seen ${WREN}`, 'app: pin', 'app: terminal']);
  });
});

describe.each(PLATFORMS)('the main window’s keys for getting about, as %s presses them', (platform) => {
  it('go to each view by its chord, from any view', () => {
    setPlatform(platform);
    expect(each(VIEW_CHORDS, () => mainWindow({ view: 'todo' }))).toEqual({ sessions: ['view sessions'], prs: ['view prs'], todo: ['view todo'], schedules: ['view schedules'], kanban: ['view kanban'], canvas: ['view canvas'] });
  });

  it('go to a view from a text field too, but not from under a sheet', () => {
    setPlatform(platform);
    const w = mainWindow();
    field('other');
    press(VIEW_CHORDS.prs);
    expect(w.did).toEqual(['view prs']);
    for (const sheet of [{ settings: {} }, { previewFor: '/r/northwind/app' }, { renaming: KESTREL }]) {
      const under = mainWindow(sheet);
      press(VIEW_CHORDS.todo);
      expect(under.did).toEqual([]);
    }
  });

  it('have no chord for a view the window doesn’t have', () => {
    setPlatform(platform);
    // Boards off: no Kanban, no Canvas.
    const w = mainWindow({ views: VIEWS.filter((v) => v.id !== 'kanban' && v.id !== 'canvas') });
    press(VIEW_CHORDS.kanban);
    press(VIEW_CHORDS.canvas);
    expect(w.did).toEqual([]);
  });

  it('go through the views with ⌃Tab and ⌃⇧Tab as through tabs, round at the ends, and with v outside Sessions', () => {
    setPlatform(platform);
    const first = mainWindow({ view: 'sessions' });
    for (const chord of ['Ctrl+Tab', 'Ctrl+Shift+Tab', 'v']) press(chord);
    expect(first.did).toEqual(['view prs', 'view canvas']);
    const last = mainWindow({ view: 'canvas' });
    for (const chord of ['Ctrl+Tab', 'v']) press(chord);
    expect(last.did).toEqual(['view sessions', 'view sessions']);
  });

  it('go through what waits on you in Sessions: on with ⌘] or ⌥⇧↓ and back with ⌘[ or ⌥⇧↑, the cursor in the next one’s box', () => {
    setPlatform(platform);
    const w = onSession();
    for (const chord of [...QUEUE_CHORDS.next, ...QUEUE_CHORDS.back]) expect(press(chord).defaultPrevented, chord).toBe(true);
    // Next is the strip's Next: on to the view where the next kind waits, past the last session.
    expect(w.did).toEqual(['view’s queue next', 'view’s queue next', 'queue back, into the box', 'queue back, into the box']);
  });

  it('go through what waits in PRs, Todo and Kanban with the same keys, and nowhere in the views that have no queue', () => {
    setPlatform(platform);
    for (const view of ['prs', 'todo', 'kanban'] as const) {
      const w = mainWindow({ view });
      press('Cmd+]');
      press('Alt+Shift+Up');
      expect(w.did, view).toEqual(['view’s queue next', 'view’s queue back']);
    }
    for (const view of ['schedules', 'canvas'] as const) {
      const w = mainWindow({ view });
      press('Cmd+]');
      press('Cmd+[');
      expect(w.did, view).toEqual([]);
    }
  });

  it('go back and forward through the sessions looked at with ⌃- and ⌃⇧-, from any view', () => {
    setPlatform(platform);
    const w = mainWindow({ view: 'prs' });
    press('Ctrl+-');
    press('Ctrl+Shift+-');
    expect(w.did).toEqual(['history back', 'history forward']);
  });

  it('move along the list with the arrows and open the row with ⏎; with j and k too outside Sessions, whose box takes letters', () => {
    setPlatform(platform);
    const w = onSession();
    for (const chord of ['Down', 'Up', 'Alt+Down', 'Alt+Up', 'j', 'k', 'Enter', '/']) press(chord);
    expect(w.did).toEqual(['move 1', 'move -1', 'move 1', 'move -1', 'open row /r/northwind/app::claude:northwind:kestrel', 'filter the list']);
    const kanban = onSession(kestrel(), { view: 'kanban' });
    press('j');
    press('k');
    expect(kanban.did).toEqual(['move 1', 'move -1']);
  });

  it('take a menu’s letter alone outside Sessions: P, A, R and U run the row’s actions, E marks it seen', () => {
    setPlatform(platform);
    const w = onSession(kestrel(), { view: 'kanban' });
    for (const letter of ['p', 'a', 'r', 'u', 'e']) expect(press(letter).defaultPrevented, letter).toBe(letter !== 'e');
    expect(w.did).toEqual(['Kestrel: pin', 'Kestrel: archive', 'Kestrel: rename', 'Kestrel: unread', 'mark seen nothing']);
    const unseen = onSession(wren(), { view: 'kanban' });
    press('e');
    expect(unseen.did).toEqual([`mark seen ${WREN}`]);
    // In Sessions a word typed on the list by mistake would otherwise pin, archive or rename.
    const sessions = onSession();
    for (const letter of ['p', 'a', 'r', 'u', 'e', 'v']) expect(press(letter).defaultPrevented, letter).toBe(false);
    expect(sessions.did).toEqual([]);
  });

  it('open where the row selected lives, each place by its key (PLACE_CHORDS)', () => {
    setPlatform(platform);
    expect(each(PLACE_CHORDS, () => onSession())).toEqual({
      newSession: ['quick panel in /r/northwind/app'],
      terminal: ['open l|terminal of app'],
      editor: ['open l|editor of app'],
      claudeApp: [`open s|${KESTREL}|claude-app of app`],
      codex: ['open l|codex of app'],
      preview: ['open l|preview of app'],
      github: ['open p|github of app'],
    });
  });

  it('start a session for the todo selected with ⌘N, or one for all the todos picked', () => {
    setPlatform(platform);
    const row = (id: string) => todoItem(fx.todo({ id }), new Map(), new Map(), new Map());
    const one = mainWindow({ view: 'todo', selected: row('t1') });
    press('Cmd+N');
    expect(one.did).toEqual(['start a session for todo t1']);
    const several = mainWindow({ view: 'todo', selected: row('t1'), pickedRows: [row('t1'), row('t2')] });
    press('Cmd+N');
    expect(several.did).toEqual(['start a session for todos t1, t2']);
    // ⇧⌘N is File › New window's.
    expect(press('Shift+Cmd+N').defaultPrevented).toBe(false);
  });

  it('copy the worktree’s path with ⌥⌘C and its branch with ⌘C', () => {
    setPlatform(platform);
    const w = onSession();
    press('Alt+Cmd+C');
    press('Cmd+C');
    expect(w.did).toEqual(['copy worktree path /r/northwind/app', 'copy branch name kestrel-login']);
  });

  it('pick a project with ⌥⌘ and its digit, and all of them with ⌥⌘0, from a text field too', () => {
    setPlatform(platform);
    const projects = ['Northwind', 'Atlas', 'Kestrel'].map((name) => ({ name, lanes: 1, repos: [] }));
    const w = mainWindow({ projects });
    field('other');
    for (const chord of ['Alt+Cmd+2', 'Alt+Cmd+0', 'Alt+Cmd+9']) expect(press(chord).defaultPrevented, chord).toBe(true);
    expect(w.did).toEqual(['project Atlas', 'project All projects']);
  });

  it('move a pinned session among the pins with ⌃⌘↑ and ⌃⌘↓, and leave one that isn’t pinned where it is', () => {
    setPlatform(platform);
    const lane = app([wren(), kestrel(), wren({ key: 'claude:northwind:dune', title: 'Dune' })]);
    const rows = lane.sessions.map((session) => sessionItemOf({ lane, session }));
    const pinned = onSession(lane.sessions[1], { selectedLane: lane, pinnedRows: rows }, { pinnedSessions: lane.sessions.map((s) => s.key) });
    press('Ctrl+Cmd+Up');
    press('Ctrl+Cmd+Down');
    // Insertion points: above the row before it, and below the row after it.
    expect(pinned.did).toEqual([`move pinned session ${KESTREL} to 0`, `move pinned session ${KESTREL} to 3`]);
    const loose = onSession(lane.sessions[1], { selectedLane: lane });
    expect(press('Ctrl+Cmd+Up').defaultPrevented).toBe(true);
    expect(loose.did).toEqual([]);
  });

  it('zoom, reload and open Settings from anywhere, under a sheet too; the sidebar and the quick panel from the window', () => {
    setPlatform(platform);
    const under = mainWindow({ settings: {} });
    for (const chord of ['Cmd+=', 'Cmd+-', 'Cmd+0', 'Cmd+R', 'Cmd+,', 'Ctrl+Cmd+S', 'Cmd+K']) press(chord);
    expect(under.did).toEqual(['zoom in', 'zoom out', 'zoom actual', 'reload', 'open Settings']);
    const w = onSession();
    for (const chord of ['Ctrl+Cmd+S', 'Cmd+K']) press(chord);
    expect(w.did).toEqual(['toggle the sidebar', 'quick panel in /r/northwind/app']);
  });

  it('give the list its keys back with Escape from Sessions’ box', () => {
    setPlatform(platform);
    const list = document.body.appendChild(document.createElement('div'));
    list.tabIndex = -1;
    const w = onSession(kestrel(), { listRef: { current: list } });
    field('box');
    expect(press('Esc').defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(list);
    expect(w.did).toEqual([]);
  });

  it('leave a dialog over the window its keys', () => {
    setPlatform(platform);
    const w = onSession();
    document.body.appendChild(document.createElement('div')).setAttribute('aria-modal', 'true');
    for (const chord of ['Ctrl+Cmd+A', 'Shift+Cmd+E', VIEW_CHORDS.prs, 'Down', 'Enter']) press(chord);
    expect(w.did).toEqual([]);
  });
});

describe('the main window’s keys on Linux', () => {
  it('are the ones a Linux keyboard sends, Ctrl for ⌘ and Alt for ⌃ and ⌥, and not macOS’s', () => {
    setPlatform('linux');
    const w = onSession();
    type({ key: 'E', code: 'KeyE', ctrlKey: true, shiftKey: true });
    type({ key: 'a', code: 'KeyA', ctrlKey: true, altKey: true });
    type({ key: '`', code: 'Backquote', ctrlKey: true });
    type({ key: 'P', code: 'KeyP', ctrlKey: true, shiftKey: true });
    type({ key: '1', code: 'Digit1', ctrlKey: true });
    type({ key: '-', code: 'Minus', altKey: true });
    type({ key: 'PageUp', code: 'PageUp', ctrlKey: true, shiftKey: true });
    expect(w.did).toEqual(['Kestrel: done', 'Kestrel: archive', 'Kestrel: terminal', 'view prs', 'open l|terminal of app', 'history back']);
    // A page sees no ⌘ there (platform.ts): what macOS sends for the same chords is nothing's.
    const mac = onSession();
    type({ key: 'E', code: 'KeyE', metaKey: true, shiftKey: true });
    type({ key: 'a', code: 'KeyA', metaKey: true, ctrlKey: true });
    type({ key: 'P', code: 'KeyP', metaKey: true, shiftKey: true });
    expect(mac.did).toEqual([]);
  });
});

describe.each(PLATFORMS)('a view’s chord in a window that floats, as %s presses it', (platform) => {
  it('brings Codealong’s window forward on that view, once the window has put away what has to go', () => {
    setPlatform(platform);
    const order: string[] = [];
    mountHook(() => useViewKeys(() => void order.push(`away with ${engine.sent('show_main').length} shown`)));
    expect(press(VIEW_CHORDS.todo).defaultPrevented).toBe(true);
    expect(order).toEqual(['away with 0 shown']);
    expect(engine.sent('show_main')).toEqual([{ reveal: { kind: 'view', view: 'todo' } }]);
  });

  it('is taken before the window’s own keys and its fields’', () => {
    setPlatform(platform);
    const heard: string[] = [];
    const own = (e: KeyboardEvent) => void heard.push(e.key);
    window.addEventListener('keydown', own);
    try {
      mountHook(() => useViewKeys());
      const box = field('other');
      box.addEventListener('keydown', own);
      press(VIEW_CHORDS.prs);
      press('Cmd+P');
      // ⌘P is the window's: only that one reaches the field and the window.
      expect(heard).toEqual(['p', 'p']);
      expect(engine.sent('show_main')).toEqual([{ reveal: { kind: 'view', view: 'prs' } }]);
    } finally {
      window.removeEventListener('keydown', own);
    }
  });

  it('is left alone while a dialog is up', () => {
    setPlatform(platform);
    mountHook(() => useViewKeys());
    document.body.appendChild(document.createElement('div')).setAttribute('aria-modal', 'true');
    expect(press(VIEW_CHORDS.sessions).defaultPrevented).toBe(false);
    expect(engine.sent('show_main')).toEqual([]);
  });
});
