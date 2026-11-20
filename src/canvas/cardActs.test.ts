// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';
import { engine, settle } from '../testing/dom';
import { prRowOf, sessionIndexOf } from '../boardLists';
import type { MenuSpec } from '../contextMenu';
import { sessionHomes } from '../lanes';
import * as fx from '../testing/fixtures';
import { todoItem } from '../todos';
import { cardActsOf, type CardsHas } from './cardActs';
import { entitiesOf, type RefItem, type RefKind } from './model';

// What a card on the canvas does in the main window (cardActs.ts): opened where its thing lives,
// and the menu its row has in the other views.

const kestrel = fx.session({ key: 'claude:northwind:kestrel', title: 'Kestrel', openTarget: 's|claude:northwind:kestrel|focus' });
const app = fx.lane({ id: '/r/northwind/app', callsign: 'app', defaultTarget: 'l|terminal', sessions: [kestrel] });
const pr = fx.prItem({ number: 7 });
const todo = fx.todo({ id: 't1', title: 'Ship the importer' });
const entities = entitiesOf(fx.board({ prs: [pr] }), [app], [todo], () => true);
const todoItems = [todoItem(todo, new Map(), sessionHomes([app]), new Map())];

let did: string[] = [];
beforeEach(() => {
  did = [];
});

const menu = (label: string): MenuSpec => [{ label, run: () => {} }];
const has = (): CardsHas => ({
  canvasEntities: entities,
  todoItems,
  open: (lane, target) => void did.push(`open ${target} of ${lane?.callsign}`),
  openRow: (row) => void did.push(`open the row ${row?.key ?? 'none'}`),
  setToast: (m) => void did.push(`say “${m}”`),
  laneMenuSpec: (lane) => menu(`worktree ${lane.callsign}`),
  sessionMenuSpec: (_lane, s, pin) => menu(`session ${s.title} (${pin})`),
  todoMenuSpec: (r) => menu(`todo ${r.todo.title}`),
  prMenuSpec: (r) => menu(`PR #${r.pr.number}`),
  prRowOf: (p) => prRowOf(p, new Map(), sessionIndexOf([])),
  showTodo: (t) => void did.push(`show the todo ${t.id}`),
});
const card = (ref: RefKind, key: string): RefItem => ({ kind: 'ref', ref, key, x: 0, y: 0 });
const labels = (spec: MenuSpec) => spec.map((i) => (i === 'separator' ? '-' : i.label));

describe('opening a card on the canvas', () => {
  it('opens a worktree at its default place, and a session where it runs', () => {
    const { openCard } = cardActsOf(has());
    openCard(card('worktree', app.id));
    openCard(card('session', kestrel.key));
    expect(did).toEqual(['open l|terminal of app', 'open s|claude:northwind:kestrel|focus of app']);
  });

  it('opens a todo as a double-click on its row does, and a PR on GitHub', async () => {
    const { openCard } = cardActsOf(has());
    openCard(card('todo', 't1'));
    openCard(card('pr', pr.key));
    await settle();
    expect(did).toEqual(['open the row todo:t1']);
    expect(engine.sent('open_url')).toEqual([{ url: pr.url }]);
  });

  it('does nothing for a card whose thing the board no longer has, but for a todo, which its row’s open is asked of', async () => {
    const { openCard } = cardActsOf(has());
    openCard(card('worktree', '/r/gone'));
    openCard(card('session', 'claude:northwind:gone'));
    openCard(card('pr', 'acme/app#99'));
    openCard(card('todo', 'gone'));
    await settle();
    expect(did).toEqual(['open the row none']);
    expect(engine.sent('open_url')).toEqual([]);
  });

  it('says why when a PR’s page can’t be opened', async () => {
    engine.answer('open_url', () => {
      throw new Error('No browser.');
    });
    cardActsOf(has()).openCard(card('pr', pr.key));
    await settle();
    expect(did).toEqual(['say “Error: No browser.”']);
  });
});

describe('a card’s right-click menu', () => {
  const cardMenu = (ref: RefKind, key: string) => cardActsOf(has()).cardMenu(card(ref, key));

  it('is the one its row has in the other views', () => {
    expect(labels(cardMenu('worktree', app.id))).toEqual(['worktree app']);
    expect(labels(cardMenu('session', kestrel.key))).toEqual(['session Kestrel (session)']);
    expect(labels(cardMenu('pr', pr.key))).toEqual(['PR #7']);
  });

  it('adds Show in Todo to a todo’s', () => {
    const spec = cardMenu('todo', 't1');
    expect(labels(spec)).toEqual(['todo Ship the importer', '-', 'Show in Todo']);
    const show = spec[2];
    if (show !== 'separator' && 'run' in show) show.run();
    expect(did).toEqual(['show the todo t1']);
  });

  it('is empty for a card whose thing the board no longer has', () => {
    expect([cardMenu('worktree', '/r/gone'), cardMenu('session', 'gone'), cardMenu('todo', 'gone'), cardMenu('pr', 'gone')]).toEqual([[], [], [], []]);
  });
});
