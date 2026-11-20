import { describe, expect, it } from 'vitest';
import { prRowOf, sessionIndexOf, sessionItemOf, type ListRow } from './boardLists';
import { sessionHomes, type LaneItem } from './lanes';
import { panelKindOf, selectedRow, shownSessionOf, type PanelOn, type SelectionHas, type ShownSessionOn } from './mainSelection';
import { VIEWS, type View } from './mainViews';
import * as fx from './testing/fixtures';
import { todoItem } from './todos';

// What the main window has selected and what its panel shows for it (mainSelection.ts).

const kestrel = fx.session({ key: 'claude:northwind:kestrel' });
const atlas = fx.session({ key: 'claude:northwind:atlas' });
const app = fx.lane({ id: '/r/northwind/app', sessions: [kestrel] });
const docs = fx.lane({ id: '/r/northwind/docs', sessions: [atlas] });
const row = { kestrel: sessionItemOf({ lane: app, session: kestrel }), atlas: sessionItemOf({ lane: docs, session: atlas }) };
const head = (lane: typeof app): LaneItem => ({ kind: 'lane', key: lane.id, lane });
const pr = prRowOf(fx.prItem({ number: 7 }), new Map(), sessionIndexOf([]));
const merged = prRowOf(fx.prItem({ number: 8, state: 'merged' }), new Map(), sessionIndexOf([]));
const todo = (id: string) => todoItem(fx.todo({ id }), new Map(), sessionHomes([app, docs]), new Map());
const open = todo('t1');
const done = todo('t2');

const NONE: SelectionHas = { todoItems: [], prRows: [], held: undefined, chosenLane: undefined };
const selected = (view: View, key: string | null, visible: ListRow[], has: Partial<SelectionHas> = {}) => selectedRow(view, key, visible, { ...NONE, ...has })?.key ?? null;

describe('the row selected', () => {
  it('is the one the list has by the key selected', () => {
    expect(selected('sessions', row.atlas.key, [row.kestrel, row.atlas])).toBe(row.atlas.key);
    expect(selected('prs', pr.key, [merged, pr])).toBe(pr.key);
    expect(selected('todo', open.key, [done, open])).toBe(open.key);
  });

  it('is the top session with nothing selected, else the top row, else none', () => {
    expect(selected('sessions', null, [head(app), row.kestrel, row.atlas])).toBe(row.kestrel.key);
    expect(selected('prs', null, [pr, merged])).toBe(pr.key);
    expect(selected('sessions', null, [])).toBeNull();
  });

  it('stays on a session listed under another worktree now', () => {
    // Its commands moved to docs: the same session, under another worktree's key.
    const moved = sessionItemOf({ lane: docs, session: kestrel });
    expect(selected('sessions', row.kestrel.key, [row.atlas, moved])).toBe(moved.key);
  });

  it('stays on its worktree when the selected session’s line goes, not on the top', () => {
    const other = fx.session({ key: 'claude:northwind:other' });
    const beside = sessionItemOf({ lane: app, session: other });
    expect(selected('sessions', row.kestrel.key, [row.atlas, beside])).toBe(beside.key);
    // Nothing of its worktree is left: the top session.
    expect(selected('sessions', row.kestrel.key, [head(docs), row.atlas])).toBe(row.atlas.key);
  });

  it('holds a session you’re moving in Sessions until its copy takes its place', () => {
    expect(selected('sessions', row.kestrel.key, [row.atlas], { held: row.kestrel })).toBe(row.kestrel.key);
  });

  it('is a worktree selected by its id in Sessions, whether the list has its heading or not', () => {
    expect(selected('sessions', app.id, [row.atlas], { chosenLane: head(app) })).toBe(app.id);
    expect(selected('sessions', app.id, [row.atlas, head(app)])).toBe(app.id);
  });

  it('keeps a todo that left the list in the panel, in Todo and on a Kanban', () => {
    const has = { todoItems: [open, done] };
    expect(selected('todo', done.key, [open], has)).toBe(done.key);
    expect(selected('kanban', done.key, [open, row.kestrel], has)).toBe(done.key);
    // One that's gone altogether: the top session of a Kanban, else its top card.
    expect(selected('kanban', 'todo:gone', [open, row.kestrel], has)).toBe(row.kestrel.key);
    expect(selected('todo', 'todo:gone', [open], has)).toBe(open.key);
  });

  it('keeps a PR that left the list in the panel, as one you merge does', () => {
    expect(selected('prs', merged.key, [pr], { prRows: [pr, merged] })).toBe(merged.key);
    expect(selected('prs', 'pr:gone', [pr], { prRows: [pr, merged] })).toBe(pr.key);
  });

  it('doesn’t take a todo or a PR by its key in another view', () => {
    expect(selected('sessions', done.key, [row.kestrel], { todoItems: [done], prRows: [pr] })).toBe(row.kestrel.key);
    expect(selected('sessions', pr.key, [row.kestrel], { todoItems: [done], prRows: [pr] })).toBe(row.kestrel.key);
  });
});

describe('the session whose conversation the panel shows', () => {
  const on = (over: Partial<ShownSessionOn> = {}): ShownSessionOn => ({ scheduleSession: null, drilled: false, selected: null, canvasSel: null, ...over });

  it('is the session selected in Sessions, and none for a worktree’s heading', () => {
    expect(shownSessionOf('sessions', on({ selected: row.kestrel }))).toBe(kestrel.key);
    expect(shownSessionOf('sessions', on({ selected: head(app) }))).toBeNull();
    // A PR in its panel's place doesn't take it out of Sessions' history.
    expect(shownSessionOf('sessions', on({ selected: row.kestrel, drilled: true }))).toBe(kestrel.key);
  });

  it('is a Kanban’s session card, unless a PR or a worktree shows in its panel’s place', () => {
    expect(shownSessionOf('kanban', on({ selected: row.kestrel }))).toBe(kestrel.key);
    expect(shownSessionOf('kanban', on({ selected: row.kestrel, drilled: true }))).toBeNull();
    expect(shownSessionOf('kanban', on({ selected: open }))).toBeNull();
  });

  it('is the one selected under a schedule, whatever is selected elsewhere', () => {
    expect(shownSessionOf('schedules', on({ scheduleSession: atlas.key, selected: row.kestrel }))).toBe(atlas.key);
    expect(shownSessionOf('schedules', on({ selected: row.kestrel }))).toBeNull();
  });

  it('is the canvas’s selected card when that’s a session', () => {
    expect(shownSessionOf('canvas', on({ canvasSel: { ref: 'session', key: kestrel.key } }))).toBe(kestrel.key);
    expect(shownSessionOf('canvas', on({ canvasSel: { ref: 'todo', key: 't1' }, selected: row.kestrel }))).toBeNull();
  });

  it('is none in PRs and Todo: a session their panels list shows in the peek', () => {
    expect([shownSessionOf('prs', on({ selected: pr })), shownSessionOf('todo', on({ selected: open }))]).toEqual([null, null]);
  });
});

describe('the panel on the right', () => {
  const on = (over: Partial<PanelOn> = {}): PanelOn => ({ boardless: false, scheduleSession: false, canvasKind: null, selectedKind: undefined, ...over });

  it('is each view’s own with nothing selected', () => {
    expect(VIEWS.map((v) => [v.id, panelKindOf(v.id, on())])).toEqual([
      ['sessions', 'lane'],
      ['prs', 'pr'],
      ['todo', 'todo'],
      ['schedules', 'schedule'],
      ['kanban', 'todo'],
      ['canvas', null],
    ]);
  });

  it('is a session’s in Sessions and on a Kanban when a session is selected, a worktree’s for a heading', () => {
    expect(panelKindOf('sessions', on({ selectedKind: 'agent' }))).toBe('session');
    expect(panelKindOf('sessions', on({ selectedKind: 'lane' }))).toBe('lane');
    expect(panelKindOf('kanban', on({ selectedKind: 'agent' }))).toBe('session');
    expect(panelKindOf('kanban', on({ selectedKind: 'todo' }))).toBe('todo');
  });

  it('is the session’s under a schedule once one is selected there', () => {
    expect(panelKindOf('schedules', on({ scheduleSession: true }))).toBe('session');
  });

  it('is the selected card’s on the canvas', () => {
    expect((['lane', 'session', 'todo', 'pr'] as const).map((canvasKind) => panelKindOf('canvas', on({ canvasKind })))).toEqual(['lane', 'session', 'todo', 'pr']);
  });

  it('is none in the Board view with no board to show', () => {
    expect([panelKindOf('kanban', on({ boardless: true })), panelKindOf('canvas', on({ boardless: true, canvasKind: 'todo' }))]).toEqual([null, null]);
  });
});
