// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';
import './testing/dom';
import { sessionItemOf, type Facets } from './boardLists';
import { sessionHomes } from './lanes';
import { helperActsOf, type HelpersHas } from './mainHelpers';
import * as fx from './testing/fixtures';
import type { Session, SideGlance } from './types';

// What a session's helpers chip and its side conversations do in the main window (mainHelpers.ts).

const parent = fx.session({ key: 'claude:northwind:kestrel', title: 'Kestrel', status: 'working' });
const helping = (over: Partial<Session> = {}) => fx.session({ key: 'codex:atlas:review', title: 'Review the importer', account: 'atlas', harness: 'codex', helper: { parent: parent.key } as Session['helper'], ...over });
const done = helping({ status: 'idle' });
const old = fx.session({ key: 'claude:northwind:old', title: 'Old', archived: true });
const app = (helper: Session) => fx.lane({ id: '/r/northwind/app', sessions: [parent, helper, old] });

let did: string[] = [];
let facets: Facets = {};
let shown: string[] = [];
beforeEach(() => {
  did = [];
  facets = {};
  shown = [];
});

/** The window, with the helper's session listed and the helper not (it's finished). */
function has(helper: Session = done, over: Partial<HelpersHas> = {}): HelpersHas {
  const lane = app(helper);
  return {
    homes: sessionHomes([lane]),
    archivedItems: [sessionItemOf({ lane, session: old })],
    picks: { sessions: { select: (row, how) => void did.push(`select ${row.session.title} (${how})`) } },
    focusBox: () => void did.push('cursor in the box'),
    facets,
    setFacets: (next) => {
      facets = typeof next === 'function' ? next(facets) : next;
    },
    setHelpersShownList: (next) => {
      shown = next(shown);
    },
    sessionRows: [sessionItemOf({ lane, session: parent })],
    lanes: [lane],
    openRow: (row) => void did.push(`open ${row && 'session' in row ? row.session.title : 'nothing'}`),
    sessionMenu: (l, s, pin) => void did.push(`menu of ${s.title} in ${l.callsign} (${pin})`),
    ...over,
  };
}

describe('a helper shown from its session’s chip', () => {
  it('is listed under its session, selected with its conversation in the panel and the cursor in its box', () => {
    helperActsOf(has()).helperActions.show(done.key);
    expect(shown).toEqual([parent.key]);
    expect(did).toEqual(['select Review the importer (one)', 'cursor in the box']);
  });

  it('is opened where it runs, listed so too', () => {
    helperActsOf(has()).helperActions.open?.(done.key);
    expect(shown).toEqual([parent.key]);
    expect(did).toEqual(['open Review the importer']);
  });

  it('shows every agent and label when the sidebar’s picks would hide it, and leaves them when they wouldn’t', () => {
    facets = { agents: ['northwind'], sessionLabels: ['bug'], repos: ['northwind/app'] };
    helperActsOf(has()).helperActions.show(done.key);
    expect(facets).toEqual({ agents: [], sessionLabels: [], repos: ['northwind/app'] });
    facets = { agents: ['atlas'] };
    helperActsOf(has()).helperActions.show(done.key);
    expect(facets).toEqual({ agents: ['atlas'] });
  });

  it('opens its own menu, as a session’s in Sessions', () => {
    helperActsOf(has()).helperActions.menu?.(done.key);
    expect(did).toEqual(['menu of Review the importer in app (session)']);
  });

  it('does nothing for one the views don’t list', () => {
    const acts = helperActsOf(has()).helperActions;
    acts.show('codex:atlas:gone');
    acts.open?.('codex:atlas:gone');
    acts.menu?.('codex:atlas:gone');
    expect([did, shown]).toEqual([[], []]);
  });
});

describe('a side conversation shown from its session', () => {
  const side: SideGlance = { id: 'side-1', question: 'Why a queue?', asked: 1, running: false, open: true, startedAt: fx.NOW - 1000, lastAt: fx.NOW - 500 };

  it('selects the session and puts the cursor in its box', () => {
    helperActsOf(has()).showSideOf(parent.key, side);
    expect(did).toEqual(['select Kestrel (one)', 'cursor in the box']);
  });

  it('finds an archived session in the Archived fold', () => {
    helperActsOf(has()).showSideOf(old.key, { ...side, open: false });
    expect(did).toEqual(['select Old (one)', 'cursor in the box']);
  });

  it('still takes the keyboard for a session nothing lists', () => {
    helperActsOf(has()).showSideOf('claude:northwind:gone', side);
    expect(did).toEqual(['cursor in the box']);
  });
});
