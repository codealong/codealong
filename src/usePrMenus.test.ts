// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';
import { engine, mountHook, settle } from './testing/dom';
import { menusAsked, menuWords, pick, under } from './testing/menus';
import { sessionIndexOf, prRowOf } from './boardLists';
import { clearNotice, drawNoticesHere, reportedNotices } from './notices';
import type { Pins } from './pins';
import { START_OPTIONS_NOTICE } from './start';
import * as fx from './testing/fixtures';
import type { MenuGroup, MenuSpec } from './contextMenu';
import type { PrItem, StartOptions } from './types';
import { usePrMenus, type PrMenusInput } from './usePrMenus';

// A PR's menus in the main window (usePrMenus.ts): what its own menu has of this window's, the
// same menu from a chip, Review with… from a button, and the start options they go by.

const app = fx.lane({ id: '/r/northwind/app', callsign: 'app', project: 'Northwind', repoSlug: 'northwind/app' });
const listed = fx.prItem({ repoSlug: 'northwind/app', number: 7, project: 'Northwind', lanes: [app.id], head: 'importer' });
const unlisted = fx.pr({ number: 9, url: 'https://github.com/atlas/site/pull/9' });
const held = fx.lane({ ...app, pr: fx.pr({ number: 7, url: listed.url }) });

/** An account that reviews in the Codex app: Review with… then offers Codex's own review. */
const options = (over: Partial<StartOptions> = {}): StartOptions => ({ accounts: [fx.startAccount({ name: 'atlas', harness: 'codex', places: [{ place: 'codex-app', label: 'Codex app' }] })], codexProjects: [], claudeModels: [], ...over });

// The main window draws the notices it reports (useReportedNotices in App.tsx).
drawNoticesHere();

let did: string[] = [];
beforeEach(() => {
  did = [];
  clearNotice(START_OPTIONS_NOTICE);
});

/** What Review with › offers in a menu's spec. */
const reviews = (spec: MenuSpec): string[] => {
  const group = spec.find((i): i is MenuGroup => i !== 'separator' && 'items' in i && i.label === 'Review with');
  return (group?.items ?? []).map((i) => (i === 'separator' ? '-' : i.label));
};

const pins = (keys: string[] = []): Pins => ({ keys, set: new Set(keys), toggle: (key) => void did.push(`pin ${key}`), move: () => {}, place: () => {}, remove: () => {}, pinAll: () => {} });

/** The window, in PRs, but for what a test says. */
function has(over: Partial<PrMenusInput> = {}): PrMenusInput {
  return {
    board: fx.board({ lanes: [app], prs: [listed] }),
    lanes: [app],
    report: { error: (m) => void did.push(`error: ${m}`), info: (m) => void did.push(`info: ${m}`) },
    setToast: (m) => void did.push(`say “${m}”`),
    track: (_label, work) => work,
    approve: (pr) => void did.push(`approve #${pr.number}`),
    view: 'prs',
    selectPr: (row) => void did.push(`select ${row.key}`),
    prPins: pins(),
    onDesktop: new Set(),
    boardItems: (ref, keys) => [{ label: `Add ${ref} ${keys.join(', ')} to board`, run: () => {} }],
    prRowOf: (pr) => prRowOf(pr, new Map([[app.id, app]]), sessionIndexOf([app])),
    focusLane: (lane, row) => {
      did.push(`show ${lane.callsign}${row ? ` from ${row.key}` : ''}`);
      row?.select();
    },
    ...over,
  };
}
const row = (pr: PrItem = listed) => prRowOf(pr, new Map([[app.id, app]]), sessionIndexOf([app]));

/** The window with its start options read. */
async function mount(over: Partial<PrMenusInput> = {}) {
  const w = mountHook(usePrMenus, has(over));
  await settle();
  return w;
}
/** The last menu the window asked for. */
async function lastMenu() {
  await settle();
  const asked = menusAsked();
  return asked[asked.length - 1];
}

describe('where each account can start a review', () => {
  it('is read as the window opens, and again each time a menu is built, which goes by the last answer', async () => {
    engine.answer('start_options', options({ accounts: [] }));
    const w = await mount();
    expect(engine.sent('start_options')).toHaveLength(1);
    // The answer changes: the menu built now reads again, and still goes by the one before.
    engine.answer('start_options', options());
    expect(reviews(w.current.prMenuSpec(row()))).toEqual([]);
    expect(engine.sent('start_options')).toHaveLength(2);
    await settle();
    expect(reviews(w.current.prMenuSpec(row()))).toEqual(['The Codex app’s own review']);
  });

  it('keeps the last answer when a read fails, and says so until one works', async () => {
    engine.answer('start_options', options());
    const w = await mount();
    engine.answer('start_options', () => {
      throw new Error('The server isn’t answering.');
    });
    w.current.prMenuSpec(row());
    await settle();
    expect(reportedNotices().map((n) => n.key)).toEqual([START_OPTIONS_NOTICE]);
    expect(reviews(w.current.prMenuSpec(row()))).toEqual(['The Codex app’s own review']);
    engine.answer('start_options', options());
    w.current.prMenuSpec(row());
    await settle();
    expect(reportedNotices()).toEqual([]);
  });
});

describe('a PR’s menu in the main window', () => {
  it('has this window’s own: Review with, its pin with its key, its place on a board, and its worktree shown', async () => {
    engine.answer('start_options', options());
    const w = await mount();
    w.current.prMenu(row());
    const menu = await lastMenu();
    expect(menuWords(menu)).toEqual(['Open on GitHub', 'Review with ›', '-', 'Approve #7', 'Nudge its session on updates', '-', 'Pin PR', 'Pin to desktop', '-', 'Add pr northwind/app#7 to board', '-', 'Show worktree app', 'Copy link', 'Copy branch name']);
    expect(menuWords(under(menu, 'Review with'))).toEqual(['The Codex app’s own review']);
    pick(menu, 'Pin PR');
    // In PRs, its worktree shows from its row, which is selected first.
    pick(menu, 'Show worktree app');
    expect(did).toEqual(['pin northwind/app#7', 'show app from pr:northwind/app#7', 'select pr:northwind/app#7']);
  });

  it('says Unpin for a pinned one, and shows its worktree from what the panel shows in another view', async () => {
    const w = await mount({ prPins: pins([listed.key]), view: 'kanban' });
    w.current.prMenu(row());
    const menu = await lastMenu();
    expect(menuWords(menu)).toContain('Unpin PR');
    pick(menu, 'Show worktree app');
    expect(did).toEqual(['show app']);
  });

  it('approves an open PR that isn’t yours in one click, and doesn’t offer to approve your own', async () => {
    const w = await mount();
    w.current.prMenu(row());
    pick(await lastMenu(), 'Approve #7');
    expect(did).toEqual(['approve #7']);
    w.current.prMenu(row(fx.prItem({ ...listed, mine: true })));
    expect(menuWords(await lastMenu())).not.toContain('Approve #7');
  });
});

describe('a PR’s chip on a worktree or a session', () => {
  it('opens the PR’s own menu for one the board lists', async () => {
    const w = await mount();
    w.current.lanePrMenu(held);
    expect(menuWords(await lastMenu())).toContain('Pin PR');
  });

  it('opens what its link allows for one the board doesn’t list', async () => {
    engine.answer('start_options', options());
    const w = await mount();
    w.current.prChipMenu(app, unlisted);
    const menu = await lastMenu();
    expect(menuWords(menu)).toEqual(['Open on GitHub', 'Review with ›', '-', 'Copy link']);
    expect(menuWords(under(menu, 'Review with'))).toEqual(['The Codex app’s own review']);
  });

  it('has no menu on a worktree with no PR', async () => {
    const w = await mount();
    w.current.lanePrMenu(app);
    await settle();
    expect(menusAsked()).toEqual([]);
  });

  it('takes the board’s PR for Review with, or what the worktree knows of one it doesn’t list', async () => {
    const w = await mount();
    expect(w.current.reviewedPr(app, fx.pr({ number: 7, url: listed.url }))).toBe(listed);
    expect(w.current.reviewedPr(app, unlisted)).toEqual({ ...unlisted, repoSlug: 'northwind/app', lanes: [app.id] });
    // The repo it's in now, where its folder has moved to another.
    expect(w.current.reviewedPr({ ...app, repoSlugNow: 'atlas/site' }, unlisted).repoSlug).toBe('atlas/site');
  });
});

describe('Review with, dropped from a button', () => {
  const button = () => document.body.appendChild(document.createElement('button'));

  it('lists where a review can start', async () => {
    engine.answer('start_options', options());
    const w = await mount();
    w.current.reviewMenuFrom(listed, button());
    expect(menuWords(await lastMenu())).toEqual(['The Codex app’s own review']);
  });

  it('says so when no account can start one', async () => {
    engine.answer('start_options', options({ accounts: [] }));
    const w = await mount();
    w.current.reviewMenuFrom(listed, button());
    const menu = await lastMenu();
    expect(menuWords(menu)).toEqual(['No account can start a review here']);
    expect(menu[0].enabled).toBe(false);
  });
});
