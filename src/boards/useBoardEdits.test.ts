// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';
import { act, mountHook, settle, type Mounted } from '../testing/dom';
import { menusAsked, menuWords, pick, under } from '../testing/menus';
import type { BoardApplied } from '../api';
import type { MenuAction, MenuGroup, MenuSpec } from '../contextMenu';
import * as fx from '../testing/fixtures';
import type { BoardDoc, BoardOp } from './model';
import { useBoardEdits, type BoardEditsInput } from './useBoardEdits';
import type { BoardsHandle } from './useBoards';

// Making and changing boards from the main window (useBoardEdits.ts): what its dialogs are on,
// and what a menu's pick and a dialog's Save ask of the window and of its boards. What each menu
// lists is menus.ts's (menus.boards.test.ts).

const board = (id: string, name: string, over: Partial<BoardDoc> = {}): BoardDoc => ({ id, name, project: 'Northwind', rule: null, added: [], removed: [], ...over });
const launch = board('b1', 'Launch', { added: ['todo:t1'] });
const site = board('b2', 'Site', {
  columns: [
    { id: 'c1', name: 'Next', sort: 'hand' },
    { id: 'c2', name: 'Doing', sort: 'move' },
  ],
});

const kestrel = fx.session({ key: 'claude:northwind:kestrel', account: 'northwind' });
const atlas = fx.session({ key: 'codex:atlas:atlas', account: 'atlas' });
const app = fx.lane({ id: '/r/northwind/app', project: 'Northwind', repo: 'app', repoSlug: 'northwind/app', sessions: [kestrel, atlas] });
const todo = fx.todo({ id: 't1', project: 'Northwind' });

/** What was asked of the window, in order; the changes asked of its boards; and the engine's
 *  word on a board's move to another project, which waits on the test. */
let did: string[] = [];
let ops: BoardOp[] = [];
let moved: { resolve: (applied: BoardApplied | null) => void; reject: (e: unknown) => void } | null = null;
beforeEach(() => {
  did = [];
  ops = [];
  moved = null;
});

/** The window's boards (useBoards.ts), showing `shown`. */
const handle = (shown: BoardDoc | null, list: BoardDoc[] = [launch, site]): BoardsHandle => ({
  boards: list,
  board: shown,
  read: true,
  error: null,
  pick: (id, inProject) => void did.push(`pick ${id}${inProject ? ` in ${inProject}` : ''}`),
  apply: (op) => {
    ops.push(op);
    return new Promise((resolve, reject) => (moved = { resolve, reject }));
  },
});

/** The window, showing Launch's Kanban in Northwind, but for what a test says. */
function has(over: Partial<BoardEditsInput> = {}): BoardEditsInput {
  return {
    boards: handle(launch),
    applyBoard: (op) => {
      ops.push(op);
      return Promise.resolve(null);
    },
    boardsOn: true,
    view: 'kanban',
    setView: (view) => void did.push(`view ${view}`),
    lastLayout: 'canvas',
    current: 'Northwind',
    projects: [{ name: 'Northwind' }, { name: 'Atlas' }, { name: 'Elsewhere' }],
    pickProject: (name) => void did.push(`project ${name}`),
    setSelectedKey: (key) => void did.push(`select ${key}`),
    setToast: (message) => void did.push(`say “${message}”`),
    board: fx.board({ lanes: [app], todos: [todo] }),
    rawLanes: [app],
    projectLanes: [app],
    todoStore: { todos: [todo], labels: [{ id: 'l1', name: 'bug', color: 'red' }] },
    member: (card) => launch.added.includes(card),
    allSubjects: new Map(),
    now: fx.NOW,
    ...over,
  };
}
const mount = (over: Partial<BoardEditsInput> = {}) => mountHook(useBoardEdits, has(over));
type Edits = Mounted<ReturnType<typeof useBoardEdits>, BoardEditsInput>;

const labels = (spec: MenuSpec): string[] => spec.map((i) => (i === 'separator' ? '-' : 'items' in i ? `${i.label} ›` : `${i.checked ? '✓ ' : ''}${i.label}`));
const group = (spec: MenuSpec, label: string): MenuSpec => spec.find((i): i is MenuGroup => i !== 'separator' && 'items' in i && i.label === label)?.items ?? [];
const item = (spec: MenuSpec, label: string): MenuAction => {
  const found = spec.find((i): i is MenuAction => i !== 'separator' && !('items' in i) && i.label === label);
  if (!found) throw new Error(`No ${label} in ${labels(spec).join(', ')}`);
  return found;
};

/** The board's menu, as the window shows it now. */
async function boardMenu(w: Edits, b?: BoardDoc | null) {
  const before = menusAsked().length;
  if (b === undefined) w.current.boardMenu();
  else w.current.boardMenu(b);
  await settle();
  return menusAsked()[before];
}

describe('Add to board, from another thing’s menu', () => {
  it('offers nothing while boards are off, or for nothing', () => {
    expect(mount({ boardsOn: false }).current.boardItems('todo', ['t1'])).toEqual([]);
    expect(mount().current.boardItems('todo', [])).toEqual([]);
  });

  it('lists the boards with the one that holds it checked, puts it on another, and takes it off the Kanban shown', () => {
    const menu = mount().current.boardItems('todo', ['t1']);
    expect(labels(menu)).toEqual(['Add to board ›', 'Remove from board']);
    expect(labels(group(menu, 'Add to board'))).toEqual(['✓ Launch', 'Site', '-', 'New board with it…']);
    item(group(menu, 'Add to board'), 'Site').run();
    item(menu, 'Remove from board').run();
    expect(ops).toEqual([
      { op: 'add', id: 'b2', cards: ['todo:t1'] },
      { op: 'remove', id: 'b1', cards: ['todo:t1'] },
    ]);
  });

  it('has no Remove from board where another view lists it', () => {
    expect(labels(mount({ view: 'todo' }).current.boardItems('todo', ['t1']))).toEqual(['Add to board ›']);
  });

  it('opens the dialog on a new board that starts with it, and makes that board in the project shown', () => {
    const w = mount({ view: 'sessions' });
    act(() => item(group(w.current.boardItems('session', [kestrel.key]), 'Add to board'), 'New board with it…').run());
    expect(w.current.boardEdit).toEqual({ board: null, with: [`session:${kestrel.key}`] });
    act(() => w.current.saveBoard({ name: 'Importer', rule: null, columns: null }));
    expect(ops).toHaveLength(1);
    const made = ops[0];
    expect(made).toMatchObject({ op: 'create', board: { name: 'Importer', project: 'Northwind', rule: null, added: [`session:${kestrel.key}`], removed: [], columns: null } });
    // The window goes to the board it made, in the layout it last showed a board in.
    expect(did).toEqual([`pick ${made.op === 'create' ? made.board.id : ''}`, 'view canvas']);
  });
});

describe('a board’s dialog', () => {
  it('renames a board and changes its rule, each only when it changed', () => {
    const w = mount();
    act(() => w.current.setBoardEdit({ board: launch }));
    w.current.saveBoard({ name: 'Launch', rule: null, columns: null });
    expect(ops).toEqual([]);
    w.current.saveBoard({ name: 'Launch week', rule: { kinds: ['todo'] }, columns: null });
    expect(ops).toEqual([
      { op: 'rename', id: 'b1', name: 'Launch week' },
      { op: 'rule', id: 'b1', rule: { kinds: ['todo'] } },
    ]);
    // It's shown already: the window stays where it is.
    expect(did).toEqual([]);
  });

  it('makes a board with nothing on it from New board…, staying in the Board view’s layout', () => {
    const w = mount({ view: 'canvas' });
    act(() => w.current.setBoardEdit({ board: null }));
    w.current.saveBoard({ name: 'Ideas', rule: {}, columns: null });
    expect(ops[0]).toMatchObject({ op: 'create', board: { name: 'Ideas', added: [], rule: {} } });
    expect(did).toHaveLength(1);
    expect(did[0]).toMatch(/^pick /);
  });

  it('offers a rule the project’s labels, repos and accounts, the same list while they are the same', () => {
    const input = has();
    const w = mountHook(useBoardEdits, input);
    const choices = w.current.ruleChoices;
    expect(choices).toEqual({ labels: [{ id: 'l1', name: 'bug', color: 'red' }], repos: [{ id: 'northwind/app', label: 'app' }], accounts: ['atlas', 'northwind'] });
    w.rerender({ ...input, now: fx.NOW + 1000, view: 'canvas' });
    expect(w.current.ruleChoices).toBe(choices);
    w.rerender({ ...input, projectLanes: [] });
    expect(w.current.ruleChoices).toEqual({ labels: choices.labels, repos: [], accounts: [] });
  });
});

describe('showing a board', () => {
  it('picks it with nothing selected, and goes to the Board view from another', () => {
    mount().current.showBoard('b2');
    expect(did).toEqual(['pick b2', 'select null']);
    did = [];
    mount({ view: 'prs' }).current.showBoard('b2');
    expect(did).toEqual(['pick b2', 'select null', 'view canvas']);
  });
});

describe('a board’s menu', () => {
  it('is the board shown’s, or the Board view’s with none', async () => {
    const w = mount();
    expect(menuWords(await boardMenu(w))).toEqual(['✓ Launch', 'Site', '-', 'New board…', 'Settings of Launch…', 'Move to project ›', '-', 'Delete Launch']);
    expect(menuWords(await boardMenu(w, null))).toEqual(['✓ Launch', 'Site', '-', 'New board…']);
    expect(menuWords(await boardMenu(w, site))).toContain('Kanban columns back to the stages');
    // Another view showing: no board is the one shown.
    expect(menuWords(await boardMenu(mount({ view: 'sessions' }))).slice(0, 2)).toEqual(['Launch', 'Site']);
  });

  it('moves a board to every project but Elsewhere, and to All projects', async () => {
    expect(menuWords(under(await boardMenu(mount()), 'Move to project'))).toEqual(['✓ Northwind', 'Atlas', 'All projects']);
  });

  it('shows another board, and opens the dialog on a new one or on the board’s settings', async () => {
    const w = mount();
    const menu = await boardMenu(w, site);
    pick(menu, 'Launch');
    expect(did).toEqual(['pick b1', 'select null']);
    pick(menu, 'New board…');
    expect(w.current.boardEdit).toEqual({ board: null });
    pick(menu, 'Settings of Site…');
    expect(w.current.boardEdit).toEqual({ board: site });
  });

  it('puts a board’s columns back to the stages, and deletes it saying what stays', async () => {
    const menu = await boardMenu(mount(), site);
    pick(menu, 'Kanban columns back to the stages');
    pick(menu, 'Delete Site');
    expect(ops).toEqual([
      { op: 'columns', id: 'b2', columns: null },
      { op: 'delete', id: 'b2' },
    ]);
    expect(did).toEqual(['say “Deleted the board Site. What was on it stays as it was.”']);
  });
});

describe('moving a board to another project', () => {
  const move = async (w: Edits) => pick(under(await boardMenu(w), 'Move to project'), 'Atlas');

  it('asks the engine through the window’s boards, and says why when it’s turned down', async () => {
    await move(mount());
    expect(ops).toEqual([{ op: 'project', id: 'b1', project: 'Atlas' }]);
    moved?.reject(new Error('That project is gone.'));
    await settle();
    expect(did).toEqual(['say “That project is gone.”']);
  });

  it('follows the board it shows to its new project, once it’s moved', async () => {
    await move(mount());
    expect(did).toEqual([]);
    moved?.resolve(null);
    await settle();
    expect(did).toEqual(['pick b1 in Atlas', 'project Atlas']);
  });

  it('stays where it is under All projects, which lists the board still', async () => {
    await move(mount({ current: 'All projects' }));
    moved?.resolve(null);
    await settle();
    expect(did).toEqual([]);
  });

  it('stays where it has gone since it asked', async () => {
    const w = mount();
    await move(w);
    w.rerender(has({ view: 'sessions' }));
    moved?.resolve(null);
    await settle();
    expect(did).toEqual([]);
  });
});

describe('a Kanban column', () => {
  it('has no menu and saves nothing with no board shown', async () => {
    const w = mount({ boards: handle(null, []) });
    w.current.columnMenu({ id: 'todo', column: null });
    w.current.saveColumn({ name: 'Later', sort: 'hand' });
    await settle();
    expect([menusAsked(), ops]).toEqual([[], []]);
  });

  it('opens its settings in the dialog from its menu, and leaves the board’s columns as a pick asks', async () => {
    const w = mount({ boards: handle(site) });
    w.current.columnMenu({ id: 'c2', column: site.columns?.[1] ?? null });
    await settle();
    const [menu] = menusAsked();
    expect(menuWords(menu)).toEqual(['Column settings…', 'Fold at first', '-', 'Move left', 'Move right', '-', 'Add column…', 'Delete Doing']);
    pick(menu, 'Column settings…');
    expect(w.current.columnEdit).toEqual({ column: site.columns?.[1] });
    pick(menu, 'Add column…');
    expect(w.current.columnEdit).toEqual({ column: null });
    pick(menu, 'Move left');
    expect(ops).toEqual([{ op: 'columns', id: 'b2', columns: [site.columns?.[1], site.columns?.[0]] }]);
  });

  it('changes the column its dialog is on, keeping its id, and adds a new one after the rest', () => {
    const w = mount({ boards: handle(site) });
    act(() => w.current.setColumnEdit({ column: site.columns?.[0] ?? null }));
    w.current.saveColumn({ name: 'Up next', sort: 'latest' });
    act(() => w.current.setColumnEdit({ column: null }));
    w.current.saveColumn({ name: 'Shipped', sort: 'latest' });
    expect(ops).toHaveLength(2);
    expect(ops[0]).toEqual({
      op: 'columns',
      id: 'b2',
      columns: [
        { id: 'c1', name: 'Up next', sort: 'latest' },
        { id: 'c2', name: 'Doing', sort: 'move' },
      ],
    });
    const added = ops[1].op === 'columns' ? (ops[1].columns ?? []) : [];
    expect(added.map((c) => c.name)).toEqual(['Next', 'Doing', 'Shipped']);
    expect(new Set(added.map((c) => c.id)).size).toBe(3);
  });
});
