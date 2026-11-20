import { beforeEach, describe, expect, it } from 'vitest';
import type { ListGroup } from './groups';
import { groupActsOf, type GroupsHas } from './mainGroups';
import type { Pins } from './pins';
import * as fx from './testing/fixtures';
import type { Lane } from './types';

// What a group's heading does in the main window's lists (mainGroups.ts): in Sessions, where a
// worktree's heading is a row, and in PRs, Todo and Schedules, where it shows the worktree.

const pr = fx.pr({ number: 7 });
const app = fx.lane({ id: '/r/northwind/app', callsign: 'app', defaultTarget: 'l|terminal', pr });
const far = fx.lane({ id: '@m1:/r/atlas/web', callsign: 'web', device: { name: 'Kestrel', short: 'm1', color: 'blue', online: true, lastSeen: null } });

let did: string[] = [];
beforeEach(() => {
  did = [];
});

const pins: Pins = {
  keys: [],
  set: new Set(),
  toggle: () => {},
  move: () => {},
  place: (shown, key, index) => void did.push(`place ${key} at ${index} among ${shown.join(', ')}`),
  remove: (key) => void did.push(`unpin ${key}`),
  pinAll: () => {},
};

function has(picked: string[] = []): GroupsHas {
  return {
    focusLane: (lane) => void did.push(`show ${lane.callsign}`),
    picks: {
      sessions: { clear: () => void did.push('no sessions picked') },
      worktrees: {
        select: (row, how) => void did.push(`pick ${row.key} (${how})`),
        clear: () => void did.push('no worktrees picked'),
        keys: new Set(picked),
        menu: () => void did.push('menu of the worktrees picked'),
      },
    },
    pressRow: (key, how) => void did.push(`press ${key} (${how})`),
    setSelectedKey: (key) => void did.push(`select ${key}`),
    open: (lane, target) => void did.push(`open ${target} of ${lane.callsign}`),
    repoMenu: (key, repo) => void did.push(`menu of the repo ${repo.name} (${key})`),
    laneMenu: (lane) => void did.push(`menu of ${lane.callsign}`),
    lanePrMenu: (lane) => void did.push(`menu of ${lane.callsign}’s PR`),
    showPrOf: (p, row) => {
      did.push(`show the PR ${p.url}${row ? ` from ${row.key}` : ''}`);
      row?.select();
    },
    openLanePr: (lane) => void did.push(`open ${lane.callsign}’s PR`),
    sessionGroupPins: pins,
    pinnedGroups: ['/r/northwind/docs'],
  };
}

const group = (lane: Lane | null, over: Partial<ListGroup<unknown>> = {}): ListGroup<unknown> => ({ key: lane?.id ?? '', by: 'worktree', lane, repo: null, rows: [], all: [], folded: 0, fold: lane?.id ?? 'none', opened: false, ...over });
const toggle = (key: string) => void did.push(`toggle ${key}`);

describe('a group’s heading in Sessions', () => {
  it('selects its worktree for its panel on a press, with nothing left picked', () => {
    groupActsOf(has(), true, toggle).onSelect(group(app), 'one');
    expect(did).toEqual(['no worktrees picked', 'no sessions picked', 'press /r/northwind/app (one)', 'select /r/northwind/app']);
  });

  it('picks its worktree with others on ⌘ or ⇧, leaving no session picked', () => {
    groupActsOf(has(), true, toggle).onSelect(group(app), 'toggle');
    groupActsOf(has(), true, toggle).onSelect(group(app), 'range');
    expect(did).toEqual(['no sessions picked', 'pick /r/northwind/app (toggle)', 'no sessions picked', 'pick /r/northwind/app (range)']);
  });

  it('opens the menu of the worktrees picked on one of them, its own on another', () => {
    const acts = groupActsOf(has([app.id]), true, toggle);
    acts.onMenu(group(app));
    acts.onMenu(group(far));
    expect(did).toEqual(['menu of the worktrees picked', 'menu of web']);
  });

  it('shows its PR in its own panel’s place, selecting it alone', () => {
    groupActsOf(has(), true, toggle).onPrShow(group(app));
    expect(did).toEqual([`show the PR ${pr.url} from /r/northwind/app`, 'no worktrees picked', 'select /r/northwind/app']);
  });

  it('is dragged among the pinned groups, and pins a worktree of this device to the desktop', () => {
    const acts = groupActsOf(has([app.id]), true, toggle);
    acts.onPlace?.(app.id, 1);
    acts.onUnpin?.(app.id);
    expect(did).toEqual(['place /r/northwind/app at 1 among /r/northwind/docs', 'unpin /r/northwind/app']);
    expect(acts.desktop?.(group(app))).toEqual({ ref: 'worktree', key: app.id });
    // What's on another device has no card here; a repo has none.
    expect([acts.desktop?.(group(far)), acts.desktop?.(group(null))]).toEqual([null, null]);
    expect(acts.picked).toEqual(new Set([app.id]));
  });
});

describe('a group’s heading in PRs, Todo and Schedules', () => {
  it('shows the worktree on a press, and does nothing on ⌘ or ⇧', () => {
    const acts = groupActsOf(has(), false, toggle);
    acts.onSelect(group(app), 'one');
    acts.onSelect(group(app), 'toggle');
    expect(did).toEqual(['show app']);
  });

  it('opens the worktree’s own menu though it’s picked in Sessions, and shows its PR from what the panel shows', () => {
    const acts = groupActsOf(has([app.id]), false, toggle);
    acts.onMenu(group(app));
    acts.onPrShow(group(app));
    expect(did).toEqual(['menu of app', `show the PR ${pr.url}`]);
  });

  it('isn’t dragged, picked or pinned to the desktop', () => {
    const acts = groupActsOf(has([app.id]), false, toggle);
    expect([acts.onPlace, acts.onUnpin, acts.desktop, acts.picked]).toEqual([undefined, undefined, undefined, undefined]);
  });
});

describe('a group’s heading in every list', () => {
  it('opens the worktree where it lives, its places and its PR, and folds', () => {
    for (const rows of [true, false]) {
      did = [];
      const acts = groupActsOf(has(), rows, toggle);
      acts.onOpen(group(app));
      acts.onOpenTarget(app, 'l|finder');
      acts.onPrMenu(group(app));
      acts.onPrOpen(group(app));
      acts.onToggle(group(app, { fold: 'working:worktree' }));
      expect(did).toEqual(['open l|terminal of app', 'open l|finder of app', 'menu of app’s PR', 'open app’s PR', 'toggle working:worktree']);
    }
  });

  it('opens a repo’s menu on its heading, and does nothing for the rows in no worktree or repo', () => {
    const repo = { name: 'app', slug: 'northwind/app', project: 'Northwind' };
    for (const rows of [true, false]) {
      did = [];
      const acts = groupActsOf(has(), rows, toggle);
      acts.onMenu(group(null, { key: 'northwind/app', by: 'repo', repo }));
      acts.onSelect(group(null), 'one');
      acts.onOpen(group(null));
      acts.onMenu(group(null));
      acts.onPrMenu(group(null));
      acts.onPrShow(group(null));
      acts.onPrShow(group(far));
      acts.onPrOpen(group(null));
      expect(did).toEqual(['menu of the repo app (northwind/app)']);
    }
  });
});
