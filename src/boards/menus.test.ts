import { beforeEach, describe, expect, it } from 'vitest';
import type { MenuAction, MenuGroup, MenuSpec } from '../contextMenu';
import { addToBoard, boardMenu, columnMenu, type AddToBoardCan, type BoardCan, type ColumnCan } from './menus';
import { STAGE_COLUMNS, type BoardColumn, type BoardDoc } from './model';

// The menus of a board, of a column of its Kanban, and Add to board › (boards/menus.ts), as the
// main window builds them: what each lists, and what a pick asks of the window.

const board = (id: string, name: string, over: Partial<BoardDoc> = {}): BoardDoc => ({ id, name, project: 'Northwind', rule: null, added: [], removed: [], ...over });

/** A menu as its labels: a submenu as its label and ›, a line as -, a checked item with ✓. */
function labels(spec: MenuSpec): string[] {
  return spec.map((i) => (i === 'separator' ? '-' : 'items' in i ? `${i.label} ›` : `${i.checked ? '✓ ' : ''}${i.label}`));
}
const item = (spec: MenuSpec, label: string): MenuAction => {
  const found = spec.find((i): i is MenuAction => i !== 'separator' && !('items' in i) && i.label === label);
  if (!found) throw new Error(`No ${label} in ${labels(spec).join(', ')}`);
  return found;
};
const group = (spec: MenuSpec, label: string): MenuGroup => {
  const found = spec.find((i): i is MenuGroup => i !== 'separator' && 'items' in i && i.label === label);
  if (!found) throw new Error(`No ${label} › in ${labels(spec).join(', ')}`);
  return found;
};

let did: string[] = [];
beforeEach(() => {
  did = [];
});

describe('a board’s menu', () => {
  const launch = board('b1', 'Launch');
  const atlas = board('b2', 'Site', { project: 'Atlas', columns: [{ id: 'c1', name: 'Doing', sort: 'hand' }] });
  const can = (over: Partial<BoardCan> = {}): BoardCan => ({
    boards: [launch, atlas],
    showing: 'b1',
    current: 'Northwind',
    projects: ['Northwind', 'Atlas', 'All projects'],
    show: (id) => void did.push(`show ${id}`),
    create: () => void did.push('new board'),
    settings: (b) => void did.push(`settings of ${b.name}`),
    move: (b, project) => void did.push(`move ${b.name} to ${project}`),
    stages: (b) => void did.push(`stages for ${b.name}`),
    remove: (b) => void did.push(`delete ${b.name}`),
    ...over,
  });

  it('lists the project’s boards with the one showing checked, a new one, and the board’s own items', () => {
    expect(labels(boardMenu(launch, can()))).toEqual(['✓ Launch', 'Site · Atlas', '-', 'New board…', 'Settings of Launch…', 'Move to project ›', '-', 'Delete Launch']);
    // None is checked while another view shows.
    expect(labels(boardMenu(launch, can({ showing: null }))).slice(0, 2)).toEqual(['Launch', 'Site · Atlas']);
  });

  it('is the Board view’s with no board: the boards and a new one, or only a new one', () => {
    expect(labels(boardMenu(null, can()))).toEqual(['✓ Launch', 'Site · Atlas', '-', 'New board…']);
    expect(labels(boardMenu(null, can({ boards: [], showing: null })))).toEqual(['New board…']);
  });

  it('offers a board with columns of its own the stages back', () => {
    expect(labels(boardMenu(atlas, can()))).toContain('Kanban columns back to the stages');
    expect(labels(boardMenu(launch, can()))).not.toContain('Kanban columns back to the stages');
  });

  it('asks the window for what’s picked', () => {
    const menu = boardMenu(atlas, can());
    item(menu, 'Site · Atlas').run();
    item(menu, 'New board…').run();
    item(menu, 'Settings of Site…').run();
    item(menu, 'Kanban columns back to the stages').run();
    item(menu, 'Delete Site').run();
    const projects = group(menu, 'Move to project').items;
    expect(labels(projects)).toEqual(['Northwind', '✓ Atlas', 'All projects']);
    item(projects, 'Northwind').run();
    expect(did).toEqual(['show b2', 'new board', 'settings of Site', 'stages for Site', 'delete Site', 'move Site to Northwind']);
  });
});

describe('a Kanban column’s menu', () => {
  const cols: BoardColumn[] = [
    { id: 'a', name: 'Next', sort: 'hand' },
    { id: 'b', name: 'Doing', sort: 'move', folded: true },
    { id: 'c', name: 'Shipped', sort: 'latest' },
  ];
  let set: BoardColumn[][] = [];
  beforeEach(() => {
    set = [];
  });
  const can: ColumnCan = { settings: (c) => void did.push(`settings of ${c.name}`), add: () => void did.push('add column'), set: (next) => void set.push(next) };
  const of = (id: string, list = cols) => columnMenu(list, { id, column: list.find((c) => c.id === id) ?? null }, can) ?? [];

  it('lists its settings, its fold, its moves and its end', () => {
    expect(labels(of('b'))).toEqual(['Column settings…', 'Open at first', '-', 'Move left', 'Move right', '-', 'Add column…', 'Delete Doing']);
    expect(labels(of('a'))[1]).toBe('Fold at first');
  });

  it('has none for a column the board doesn’t list, or one that isn’t its own', () => {
    expect(columnMenu(cols, { id: 'gone', column: cols[0] }, can)).toBeNull();
    expect(columnMenu(cols, { id: 'a', column: null }, can)).toBeNull();
  });

  it('can’t move the first left or the last right, nor delete the only one', () => {
    const enabled = (spec: MenuSpec, label: string) => item(spec, label).enabled;
    expect([enabled(of('a'), 'Move left'), enabled(of('a'), 'Move right')]).toEqual([false, true]);
    expect([enabled(of('c'), 'Move left'), enabled(of('c'), 'Move right')]).toEqual([true, false]);
    expect(enabled(of('a'), 'Delete Next')).toBe(true);
    expect(enabled(of('a', [cols[0]]), 'Delete Next')).toBe(false);
  });

  it('leaves the board’s columns as a pick asks', () => {
    const menu = of('b');
    item(menu, 'Open at first').run();
    item(menu, 'Move left').run();
    item(menu, 'Move right').run();
    item(menu, 'Delete Doing').run();
    expect(set.map((next) => next.map((c) => `${c.id}${c.folded ? ' folded' : ''}`))).toEqual([
      ['a', 'b', 'c'],
      ['b folded', 'a', 'c'],
      ['a', 'c', 'b folded'],
      ['a', 'c'],
    ]);
    item(menu, 'Column settings…').run();
    item(menu, 'Add column…').run();
    expect(did).toEqual(['settings of Doing', 'add column']);
  });

  it('works on the stages as it does on a board’s own columns', () => {
    expect(labels(of('done', STAGE_COLUMNS))).toContain('Open at first');
  });
});

describe('Add to board', () => {
  const launch = board('b1', 'Launch', { added: ['todo:t1'] });
  const ideas = board('b2', 'Ideas', { project: 'All projects' });
  const site = board('b3', 'Site', { project: 'Atlas' });
  const can = (over: Partial<AddToBoardCan> = {}): AddToBoardCan => ({
    boards: [launch, ideas, site],
    current: 'Northwind',
    holds: (b) => (card) => b.added.includes(card),
    kanban: null,
    put: (b, on, cards) => void did.push(`${on ? 'add' : 'remove'} ${cards.join(', ')} ${on ? 'to' : 'from'} ${b.name}`),
    create: (cards) => void did.push(`new board with ${cards.join(', ')}`),
    ...over,
  });
  const t1 = { card: 'todo:t1', project: 'Northwind' };
  const t2 = { card: 'todo:t2', project: 'Northwind' };
  const elsewhere = { card: 'todo:t3', project: 'Atlas' };

  it('lists the boards that would show one thing, the ones that hold it checked, and a new board with it', () => {
    const menu = addToBoard('todo', [t1], can());
    expect(labels(menu)).toEqual(['Add to board ›']);
    const choices = group(menu, 'Add to board').items;
    expect(labels(choices)).toEqual(['✓ Launch', 'Ideas · All projects', '-', 'New board with it…']);
    // A pick takes it off a board that holds it, and puts it on one that doesn't.
    item(choices, 'Launch').run();
    item(choices, 'Ideas · All projects').run();
    item(choices, 'New board with it…').run();
    expect(did).toEqual(['remove todo:t1 from Launch', 'add todo:t1 to Ideas', 'new board with todo:t1']);
  });

  it('says how many of several it takes where that’s fewer than all, and puts on only the ones a board doesn’t hold', () => {
    const menu = addToBoard('todo', [t1, t2, elsewhere], can());
    expect(labels(menu)).toEqual(['Add 3 todos to board ›']);
    const choices = group(menu, 'Add 3 todos to board').items;
    expect(labels(choices)).toEqual(['Launch (2 of 3)', 'Ideas · All projects', 'Site · Atlas (1 of 3)', '-', 'New board with them…']);
    item(choices, 'Launch (2 of 3)').run();
    item(choices, 'New board with them…').run();
    expect(did).toEqual(['add todo:t2 to Launch', 'new board with todo:t1, todo:t2, todo:t3']);
  });

  it('offers Remove from board in Kanban, for what the board shown holds', () => {
    const kanban = { board: launch, member: (card: string) => card === 'todo:t1' };
    const one = addToBoard('todo', [t1], can({ kanban }));
    expect(labels(one)).toEqual(['Add to board ›', 'Remove from board']);
    const several = addToBoard('session', [t1, t2], can({ kanban }));
    expect(labels(several)).toEqual(['Add 2 sessions to board ›', 'Remove 1 session from board']);
    item(several, 'Remove 1 session from board').run();
    expect(did).toEqual(['remove todo:t1 from Launch']);
    // None of them on it: nothing to remove.
    expect(labels(addToBoard('todo', [t2], can({ kanban })))).toEqual(['Add to board ›']);
  });
});
