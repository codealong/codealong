// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';
import './testing/dom';
import * as fx from './testing/fixtures';
import { laneCanOf, sessionCanOf, type MainHas, type MovingSession } from './mainCan';
import type { MoveFollow } from './handoff';
import * as menus from './menus';
import type { Pins } from './pins';
import { sessionItemOf } from './boardLists';

// What the main window can do to a session and a worktree (mainCan.ts), from what a render of
// it has: what its menus and keys then run is menus.ts's, tested there.

const kestrel = fx.session({ key: 'claude:northwind:kestrel', title: 'Kestrel', account: 'northwind' });
const app = fx.lane({ id: '/r/northwind/app', project: 'Northwind', callsign: 'app', sessions: [kestrel] });

let did: string[] = [];
let moving: MovingSession | null = null;
beforeEach(() => {
  did = [];
  moving = null;
});

const pins = (what: string, keys: string[] = []): Pins => ({
  keys,
  set: new Set(keys),
  toggle: (key) => void did.push(`${what}: toggle ${key}`),
  move: () => {},
  place: () => {},
  remove: () => {},
  pinAll: () => {},
});

/** The window, in Sessions with nothing selected, but for what a test says. */
function has(over: Partial<MainHas> = {}): MainHas {
  const lanePins = over.lanePins ?? pins('worktree pins');
  return {
    report: { error: (m) => void did.push(`error: ${m}`), info: (m) => void did.push(`info: ${m}`) },
    view: 'sessions',
    open: (lane, target) => void did.push(`open ${target} of ${lane.callsign}`),
    board: fx.board({ lanes: [app] }),
    editors: undefined,
    newSession: ({ laneId }) => void did.push(`new session in ${laneId}`),
    todoItems: { current: (project, link, context, title) => [{ label: `Add to todo: ${project}, ${JSON.stringify(link)}, ${context.label}, ${title ?? ''}`, run: () => {} }] },
    sessionPins: pins('session pins'),
    lanePins,
    toggleLanePin: (lane) => lanePins.toggle(lane.id),
    onDesktop: new Set(['session:x']),
    keptSessions: new Set(['kept']),
    setRenaming: (key) => void did.push(`rename ${key}`),
    leftAndOn: (s, done) => void did.push(`left ${s.title}: ${done}`),
    setArchived: (keys, archived, title) => void did.push(`${archived ? 'archive' : 'unarchive'} ${keys.join(', ')} (${title})`),
    setArchivedUndo: (undo) => void did.push(`undo for ${undo?.keys.join(', ')}: ${undo?.message}`),
    queue: { again: (key) => void did.push(`again ${key}`) },
    mergingDone: { ask: () => true },
    doneHelpers: undefined,
    selected: null,
    setMoving: (to) => {
      moving = typeof to === 'function' ? to(moving) : to;
    },
    boardItems: (ref, keys) => [{ label: `Add ${ref} ${keys.join(', ')} to board`, run: () => {} }],
    todoStore: { labels: [] },
    setPreviewFor: (id) => void did.push(`preview of ${id}`),
    openSettings: (section, focus) => void did.push(`settings ${section} ${focus}`),
    setCleaning: (lanes) => void did.push(`clean up ${lanes.map((l) => l.callsign).join(', ')}`),
    ...over,
  };
}

const labelOf = (spec: unknown) => (spec as { label: string }[])[0].label;

describe('what the main window can do to a session', () => {
  it('passes every capability on the main window’s list', () => {
    const can = sessionCanOf(has(), app, kestrel, 'session');
    expect(Object.keys(can).sort()).toEqual([...menus.SESSION_CAN.main].sort());
  });

  it('shows chords beside its actions in Sessions, where letters alone do nothing, and letters elsewhere', () => {
    expect(sessionCanOf(has(), app, kestrel, 'session').keys).toBe('chords');
    expect(sessionCanOf(has({ view: 'kanban' }), app, kestrel, 'session').keys).toBe(true);
    expect(laneCanOf(has(), app).keys).toBe('chords');
    expect(laneCanOf(has({ view: 'prs' }), app).keys).toBe(true);
  });

  it('pins what the list shows: the session in Sessions, its worktree in a list of worktrees', () => {
    const one = sessionCanOf(has(), app, kestrel, 'session').pin;
    expect(one?.label).toBe('Pin session');
    one?.run();
    expect(sessionCanOf(has({ sessionPins: pins('session pins', [kestrel.key]) }), app, kestrel, 'session').pin?.label).toBe('Unpin session');
    const lane = sessionCanOf(has(), app, kestrel, 'worktree').pin;
    expect(lane?.label).toBe('Pin worktree');
    lane?.run();
    expect(sessionCanOf(has({ lanePins: pins('worktree pins', [app.id]) }), app, kestrel, 'worktree').pin?.label).toBe('Unpin worktree');
    expect(did).toEqual(['session pins: toggle claude:northwind:kestrel', 'worktree pins: toggle /r/northwind/app']);
  });

  it('names the terminal app, the board’s worktrees and PRs, and nothing of the board before it’s read', () => {
    const can = sessionCanOf(has(), app, kestrel, 'session');
    expect([can.terminal, can.worktrees, can.mergeDone?.prs]).toEqual(['Ghostty', [app], []]);
    const before = sessionCanOf(has({ board: null }), app, kestrel, 'session');
    expect([before.terminal, before.worktrees, before.mergeDone]).toEqual(['Terminal', undefined, undefined]);
  });

  it('offers a terminal of Codealong’s own under a session on this Mac, unless they’re kept in your terminal app', () => {
    expect(sessionCanOf(has(), app, kestrel, 'session').ownTerminal?.has).toBe(false);
    const open = fx.board({ lanes: [app], terminals: { [kestrel.key]: [{ id: 't1', title: 'zsh', shell: 'zsh', session: kestrel.key, folder: app.path, startedAt: fx.NOW, running: true }] } });
    expect(sessionCanOf(has({ board: open }), app, kestrel, 'session').ownTerminal?.has).toBe(true);
    const outside = fx.board({ lanes: [app], apps: { ...fx.board().apps, runIn: 'terminal' } });
    expect(sessionCanOf(has({ board: outside }), app, kestrel, 'session').ownTerminal).toBeUndefined();
  });

  it('adds it to a todo as itself in its worktree, and to a board as a session', () => {
    const can = sessionCanOf(has(), app, kestrel, 'session');
    expect(labelOf(can.todos)).toBe('Add to todo: Northwind, {"session":"claude:northwind:kestrel"}, northwind in app, Kestrel');
    expect(labelOf(can.boards)).toBe('Add session claude:northwind:kestrel to board');
    const lane = laneCanOf(has(), app);
    expect(labelOf(lane.todos)).toBe('Add to todo: Northwind, {"worktree":"/r/northwind/app"}, app, ');
    expect(labelOf(lane.boards)).toBe('Add worktree /r/northwind/app to board');
  });

  it('archives with the way back, and goes on from one archived but not from one put back', () => {
    const can = sessionCanOf(has(), app, kestrel, 'session');
    can.archive?.(true);
    can.archive?.(false);
    can.undoDone?.('Closed its terminal.');
    can.left?.('snoozed');
    expect(did).toEqual([
      'left Kestrel: archived',
      'archive claude:northwind:kestrel (Kestrel)',
      'unarchive claude:northwind:kestrel (Kestrel)',
      'undo for claude:northwind:kestrel: Closed its terminal.',
      'left Kestrel: snoozed',
    ]);
  });

  it('brings a session whose merge was refused round again, saying why where the dialog didn’t', () => {
    const can = sessionCanOf(has(), app, kestrel, 'session');
    can.failed?.(null);
    can.failed?.('GitHub refused the merge.');
    expect(did).toEqual(['again claude:northwind:kestrel', 'again claude:northwind:kestrel', 'error: GitHub refused the merge.']);
  });

  it('holds a session moved from its panel in Sessions until its copy is known, and no other', async () => {
    const row = sessionItemOf({ lane: app, session: kestrel });
    // Not the one shown, or not in Sessions: nothing is held.
    sessionCanOf(has(), app, kestrel, 'session').moving?.(Promise.resolve({ kind: 'key', key: 'copy' }));
    sessionCanOf(has({ view: 'kanban', selected: row }), app, kestrel, 'session').moving?.(Promise.resolve({ kind: 'key', key: 'copy' }));
    const other = fx.session({ key: 'claude:northwind:atlas' });
    sessionCanOf(has({ selected: sessionItemOf({ lane: app, session: other }) }), app, kestrel, 'session').moving?.(Promise.resolve({ kind: 'key', key: 'copy' }));
    expect(moving).toBeNull();
    // The one shown: held as it was, then with its copy's key.
    sessionCanOf(has({ selected: row }), app, kestrel, 'session').moving?.(Promise.resolve({ kind: 'key', key: 'claude:atlas:copy' }));
    expect(moving).toEqual({ from: row, to: null, known: new Set() });
    await Promise.resolve();
    expect(moving).toEqual({ from: row, to: 'claude:atlas:copy', known: new Set() });
    // An async move without an upfront key stays held to match by origin.
    sessionCanOf(has({ selected: row }), app, kestrel, 'session').moving?.(Promise.resolve({ kind: 'origin' }));
    await Promise.resolve();
    expect(moving).toEqual({ from: row, to: 'origin', known: new Set() });
    // A move that left no copy lets it go.
    sessionCanOf(has({ selected: row }), app, kestrel, 'session').moving?.(Promise.resolve(null));
    await Promise.resolve();
    expect(moving).toBeNull();
    // Another session held since isn't touched by the first one's answer.
    let settle: (key: MoveFollow) => void = () => {};
    sessionCanOf(has({ selected: row }), app, kestrel, 'session').moving?.(new Promise((resolve) => (settle = resolve)));
    const later: MovingSession = { from: sessionItemOf({ lane: app, session: other }), to: null, known: new Set() };
    moving = later;
    settle({ kind: 'key', key: 'claude:atlas:copy' });
    await Promise.resolve();
    expect(moving).toBe(later);
  });

  it('renames in its sheet, starts a session in its worktree, and opens where it lives', () => {
    const can = sessionCanOf(has(), app, kestrel, 'session');
    can.rename?.();
    can.newSession?.();
    can.open(app, 'l|terminal');
    expect(did).toEqual(['rename claude:northwind:kestrel', 'new session in /r/northwind/app', 'open l|terminal of app']);
    expect([can.desktop, can.windows]).toEqual([new Set(['session:x']), new Set(['kept'])]);
  });
});

describe('what the main window can do to a worktree', () => {
  it('sets its preview URL, opens Settings at its repo’s automation, and asks to clean it up', () => {
    const can = laneCanOf(has(), app);
    can.setPreview?.();
    can.automation?.('northwind/app');
    can.cleanUp?.([app]);
    can.newSession?.();
    can.pin?.run();
    expect(did).toEqual(['preview of /r/northwind/app', 'settings automation northwind/app', 'clean up app', 'new session in /r/northwind/app', 'worktree pins: toggle /r/northwind/app']);
    expect(can.pin?.label).toBe('Pin worktree');
    expect(laneCanOf(has({ lanePins: pins('worktree pins', [app.id]) }), app).pin?.label).toBe('Unpin worktree');
  });
});
