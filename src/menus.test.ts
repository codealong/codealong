import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { asideAhead, doneAhead, labelAhead, quietAhead, raiseAhead, seenAhead } from './ahead';
import { getAccounts, moveSession, moveSessionTo, openInChrome, openUrl, resumeSession, sendToSession, setPrPrimary, type Done, type MergeDone, NONE_CHOSEN } from './api';
import type { MenuAction, MenuGroup, MenuSpec } from './contextMenu';
import { carryOnIn, chordLetter, chordSigns, chorded, CHORDS, DONE_HOLD_MS, SESSION_CAN, SUGGESTION_CHORDS, suggestionChord, doneTitle, fileMenu, keyed, laneActions, laneMenu, lanesActions, lanesMenu, mergesWith, prMenu, prsActions, prsMenu, repoMenu, sessionActions, sessionMenu, sessionsActions, sessionsMenu, todoMenu, todoPanelMenu, todosMenu, toggleWaiting, TRY_AGAIN, linkMenu, unfinishedHelpers, helpersChosen, type CanPlace, type DoneHelpersAsk, type MergeDoneAsk, type SessionCan, type SessionsCan, type TodoEdit } from './menus';
import type { MergeForm } from './merge';
import { openLabel } from './open';
import { setPlatform } from './platform';
import type { Report } from './report';
import { startAccountsNow } from './startAccounts';
import * as fx from './testing/fixtures';
import type { TodoOp } from './todos';
import type { Label, Lane, PrItem, Session, Todo } from './types';

// What the engine says to Mark as done: done, unless a test says otherwise.
vi.mock('./ahead', async (original) => ({ ...(await original<typeof import('./ahead')>()), doneAhead: vi.fn(), labelAhead: vi.fn(() => Promise.resolve()), quietAhead: vi.fn(() => Promise.resolve()), raiseAhead: vi.fn(() => Promise.resolve()), asideAhead: vi.fn(() => Promise.resolve()), seenAhead: vi.fn(() => Promise.resolve()) }));
// No accounts to move to, unless a test says otherwise; what a move gives back, as a test says.
vi.mock('./api', async (original) => ({ ...(await original<typeof import('./api')>()), getAccounts: vi.fn(() => Promise.resolve([])), moveSession: vi.fn(), moveSessionTo: vi.fn(), openInChrome: vi.fn(() => Promise.resolve()), openUrl: vi.fn(() => Promise.resolve()), resumeSession: vi.fn(), sendToSession: vi.fn(), setPrPrimary: vi.fn(() => Promise.resolve()) }));
// No accounts to resume in, unless a test says otherwise.
vi.mock('./startAccounts', async (original) => ({ ...(await original<typeof import('./startAccounts')>()), startAccountsNow: vi.fn(() => []) }));

const report: Report = { error: () => {}, info: () => {} };
const noop = () => {};

/** A menu as its labels: a submenu as its label and ›, a line as -. */
function labels(spec: MenuSpec): string[] {
  return spec.map((i) => (i === 'separator' ? '-' : 'items' in i ? `${i.label} ›` : i.label));
}

const session = (over: Partial<Session> = {}): Session =>
  fx.session({
    key: 'claude:c:1',
    id: '4f1c2a9e',
    title: 'Fix login',
    status: 'unread',
    live: true,
    pid: 42,
    hosted: false,
    resumeCommand: 'claude --resume 4f1c2a9e',
    openTarget: 's|claude:c:1|focus',
    ...over,
  });

const lane = (over: Partial<Lane> = {}): Lane =>
  fx.lane({
    id: '/r/api',
    callsign: 'api',
    path: '/r/api',
    branch: 'fix-login',
    pr: fx.pr({ number: 7, url: 'https://github.com/acme/api/pull/7' }),
    defaultTarget: 'l|terminal',
    targets: [
      fx.target({ id: 'l|terminal', label: 'Open in Ghostty' }),
      fx.target({ id: 'l|finder', label: 'Reveal in Finder' }),
      fx.target({ id: 's|claude:c:1|focus', label: 'Respond in Ghostty' }),
    ],
    ...over,
  });

const base: SessionCan = { report, open: noop, terminal: 'Ghostty' };

describe('carryOnIn', () => {
  it('goes on where the account’s last session did, else in Codealong, else a terminal, as the box would', () => {
    const codealong = { place: 'codealong' as const, label: 'Codealong' };
    const terminal = { place: 'terminal' as const, label: 'Terminal' };
    const background = { place: 'background' as const, label: 'Background' };
    const app = { place: 'codex-app' as const, label: 'Codex app' };
    expect(carryOnIn([background, terminal, codealong], 'background')).toBe(background);
    expect(carryOnIn([background, terminal, codealong], undefined)).toBe(codealong);
    expect(carryOnIn([background, terminal], undefined)).toBe(terminal);
    // Where it last went on, if it can't go there now, isn't the place.
    expect(carryOnIn([background, terminal], 'codealong')).toBe(terminal);
    expect(carryOnIn([app], undefined)).toBe(app);
  });
});

describe('Try again', () => {
  afterEach(() => vi.mocked(startAccountsNow).mockReturnValue([]));

  it('sends a session whose turn an API error ended the message to try again, where Carry on would take one that isn’t running', async () => {
    const send = vi.mocked(sendToSession).mockResolvedValue({ message: 'Sent.', opened: false });
    const resume = vi.mocked(resumeSession).mockResolvedValue({ message: 'Resumed.', opened: false });
    const errored = session({ apiError: { at: 1, said: '529 Overloaded.' } });
    const acts = sessionActions(lane(), errored, base);
    expect(acts.tryAgain?.label).toBe('Try again');
    acts.tryAgain?.run();
    await vi.waitFor(() => expect(send).toHaveBeenCalledWith(errored.key, TRY_AGAIN, false));
    // Codealong carries a lost connection on by itself, so its button tries now.
    expect(sessionActions(lane(), { ...errored, connectionLost: { at: 1, continues: true } }, base).tryAgain?.label).toBe('Try now');
    // Nothing to try while it works, once it's archived, or without an error.
    expect(sessionActions(lane(), { ...errored, status: 'working' }, base).tryAgain).toBeUndefined();
    expect(sessionActions(lane(), { ...errored, archived: true }, base).tryAgain).toBeUndefined();
    expect(sessionActions(lane(), session(), base).tryAgain).toBeUndefined();
    // One that isn't running goes on where Carry on would take it, on the message.
    vi.mocked(startAccountsNow).mockReturnValue([
      fx.startAccount({
        places: [
          { place: 'background', label: 'Background' },
          { place: 'terminal', label: 'Ghostty' },
        ],
        resumes: ['background', 'terminal'],
      }),
    ]);
    const stopped = { ...errored, key: 'claude:c:2', id: '2', live: false, pid: null };
    sessionActions(lane(), stopped, base).tryAgain?.run();
    await vi.waitFor(() => expect(resume).toHaveBeenCalledWith(stopped.key, { place: 'terminal', model: null, effort: null }, TRY_AGAIN));
    expect(send).toHaveBeenCalledTimes(1);
  });
});

describe('sessionMenu', () => {
  it('reads the same in every window, less what a window can’t do', () => {
    const everywhere = labels(sessionMenu(lane(), session(), base));
    expect(everywhere).toEqual([
      'Respond in Ghostty',
      'Open PR #7 on GitHub',
      'New session here…',
      'Fork in terminal',
      '-',
      'Mark as seen',
      'Move to top',
      'Snooze ›',
      'Quiet',
      'Close terminal',
      'Mark as done',
      'Archive',
      '-',
      'Copy session ID',
      'Copy resume command',
    ]);
    // The bar's pill: its name first, then showing and answering it in Codealong.
    const bar = labels(sessionMenu(lane(), session(), { ...base, heading: true, reveal: noop, reply: noop, desktop: new Set() }));
    // The main window's: its pins, todos and boards.
    const main = labels(
      sessionMenu(lane(), session(), {
        ...base,
        pin: { label: 'Pin session', run: noop },
        desktop: new Set(['session:claude:c:1']),
        todos: [{ label: 'Add to todo', items: [] }],
        boards: [{ label: 'Add to board', items: [{ label: 'Release', run: noop }] }],
      }),
    );
    // Every window's has everything the plainest one does, in the same order.
    const common = everywhere.filter((l) => l !== '-');
    for (const menu of [bar, main]) expect(menu.filter((l) => common.includes(l))).toEqual(common);
    expect(bar.slice(0, 2)).toEqual(['Fix login · api', '-']);
    expect(bar).toEqual(expect.arrayContaining(['Open in main window', 'Reply here', 'Pin to desktop']));
    expect(main).toEqual(expect.arrayContaining(['Add to todo ›', 'Add to board ›', 'Pin session', 'Remove from desktop']));
  });

  it('turns an orchestrator off in Mark as done’s place, and offers no pin, snooze or archive for its session', () => {
    const o = session({ orchestrator: { schedule: 'orch-web', project: 'Web' } });
    const acts = sessionActions(lane(), o, { ...base, pin: { label: 'Pin', run: noop } });
    expect(acts.done?.label).toBe('Turn off orchestrator');
    expect(acts.look?.label).toBe('Look now');
    expect(acts.fresh?.label).toBe('Start afresh');
    expect(acts.pin).toBeUndefined();
    expect(acts.snooze).toBeUndefined();
    expect(acts.archive).toBeUndefined();
    expect(doneTitle(o)).toMatch(/^Turn off the orchestrator/);
    // Picked with others, it's left to its own menu: neither marked done nor archived with them.
    const other = { lane: lane(), session: session({ key: 'claude:c:2', id: 'c2' }) };
    const picked = sessionsActions([{ lane: lane(), session: o }, other], { report });
    expect(picked.takes.done).toBe(1);
    expect(picked.takes.archive).toBe(1);
    // Turning it off leaves the window's list, as Mark as done does.
    const left = vi.fn();
    sessionActions(lane(), o, { ...base, left }).done?.run();
    expect(left).toHaveBeenCalledWith('done');
    // Another machine's is turned off there: nothing here acts on it.
    const theirs = sessionActions(lane({ device: { short: 'mini', name: 'Mini' } as Lane['device'] }), o, base);
    expect(theirs.done ?? theirs.look ?? theirs.fresh ?? theirs.archive).toBeUndefined();
    // Any other session keeps all of them.
    const plain = sessionActions(lane(), session(), { ...base, pin: { label: 'Pin', run: noop } });
    expect(plain.done?.label).toBe('Mark as done');
    expect(plain.pin && plain.snooze && plain.archive).toBeTruthy();
  });

  it('offers what the session allows', () => {
    const has = (s: Session, label: string, can: Partial<SessionCan> = {}) => labels(sessionMenu(lane(), s, { ...base, ...can })).includes(label);
    expect(has(session({ status: 'idle' }), 'Mark as seen')).toBe(false);
    expect(has(session({ status: 'needs', asks: true }), 'Mark as seen')).toBe(true);
    // A turn you've seen can be your turn again; one that runs, waits on you or is archived can't.
    expect(has(session({ status: 'idle' }), 'Mark as unread')).toBe(true);
    expect(has(session({ status: 'stopped', live: false }), 'Mark as unread')).toBe(true);
    expect(has(session({ status: 'unread' }), 'Mark as unread')).toBe(false);
    expect(has(session({ status: 'working' }), 'Mark as unread')).toBe(false);
    expect(has(session({ status: 'needs', asks: true }), 'Mark as unread')).toBe(false);
    expect(has(session({ status: 'idle', archived: true }), 'Mark as unread')).toBe(false);
    expect(has(session({ attachCommand: 'claude attach 6e07c149', pid: null }), 'End background job')).toBe(true);
    expect(has(session({ live: false }), 'Close terminal')).toBe(false);
    expect(has(session({ surface: 'app' }), 'Close terminal')).toBe(false);
    expect(has(session({ harness: 'codex' }), 'Archive in Codex')).toBe(true);
    expect(has(session({ resumeCommand: null }), 'Fork in terminal')).toBe(false);
    expect(has(session({ status: 'working', stoppable: true }), 'Stop turn')).toBe(true);
    expect(has(session(), 'Stop turn')).toBe(false);
    // A helper can be made your own, after Quiet; any other session is yours already.
    const helper = labels(sessionMenu(lane(), session({ helper: { parent: 'claude:c:boss' } }), base));
    expect(helper.slice(helper.indexOf('Quiet'), helper.indexOf('Quiet') + 2)).toEqual(['Quiet', 'Make it my own']);
    expect(has(session({ helper: { pr: 'acme/api#7' } }), 'Make it my own')).toBe(true);
    expect(has(session(), 'Make it my own')).toBe(false);
    expect(has(session({ helper: { parent: 'claude:c:boss' }, archived: true }), 'Make it my own')).toBe(false);
    // Renaming, where the window asks for a name and the harness keeps one.
    expect(has(session(), 'Rename…')).toBe(false);
    expect(has(session(), 'Rename…', { rename: noop })).toBe(true);
    expect(has(session({ key: 'antigravity:a:1', harness: 'antigravity' }), 'Rename…', { rename: noop })).toBe(true);
    expect(has(session({ key: 'cursor:a:1', harness: 'cursor' }), 'Rename…', { rename: noop })).toBe(false);
    // Archived: only unarchiving, no pins or snoozes.
    const archived = session({ archived: true });
    expect(has(archived, 'Unarchive')).toBe(true);
    for (const label of ['Pin session', 'Pin to desktop', 'Snooze ›', 'Quiet', 'Mark as done']) expect(has(archived, label, { pin: { label: 'Pin session', run: noop }, desktop: new Set() })).toBe(false);
    // Done with whatever it is on this Mac: an app's session, or one that isn't running, is
    // marked seen and archived, with nothing to close.
    for (const s of [session({ surface: 'app' }), session({ live: false }), session({ hosted: true })]) expect(has(s, 'Mark as done')).toBe(true);
    // On another device: no closing, and new sessions only where a window starts them there.
    // Done, Snooze and Quiet go there, which closes it if it can and keeps the snooze or quiet.
    const remote = session({ key: '@ab12:claude:c:1' });
    const remoteLane = lane({ id: '@ab12:/r/api' });
    const there = labels(sessionMenu(remoteLane, remote, base));
    expect(there).toContain('Snooze ›');
    expect(there).toContain('Quiet');
    expect(there).not.toContain('Close terminal');
    expect(there).toContain('Mark as done');
    expect(there).not.toContain('New session here…');
    expect(labels(sessionMenu(remoteLane, remote, { ...base, newSession: noop }))).toContain('New session here…');
    // A card on the desktop shows it from that device's worktrees, and its menu takes it off.
    expect(has(remote, 'Pin to desktop', { desktop: new Set() })).toBe(true);
    // It has nothing to open here: its feed carries no open target.
    expect(labels(sessionMenu(remoteLane, { ...remote, openTarget: '' }, base))).not.toContain('Open');
    expect(has(remote, 'Remove from desktop', { desktop: new Set(['session:@ab12:claude:c:1']) })).toBe(true);
  });

  it('moves to another worktree on this Mac, by project, or to a chat of its own', () => {
    const at = lane();
    const w = (id: string, over: Partial<Lane> = {}) => fx.lane({ id, project: 'Acme', repo: 'api', ...over });
    const worktrees = [
      at,
      w('/r/api-old', { lastActivity: 1 }),
      w('/r/api-main', { origin: 'main', callsign: 'api', lastActivity: 0 }),
      w('/r/api-new', { lastActivity: 9 }),
      w('/r/web', { repo: 'web' }),
      w('/r/site', { project: 'Blog', repo: 'site' }),
      w('/r/far', { project: 'Blog', repo: 'site', device: { short: 'ab12', name: 'Studio', color: 'blue' } as Lane['device'] }),
      w('/r/stray', { project: 'Elsewhere', repo: 'stray' }),
      w('/chats', { project: 'Elsewhere', repo: 'Chats', callsign: 'Chats', startsChats: true, hidden: true }),
    ];
    const menu = sessionMenu(at, session(), { ...base, worktrees });
    const move = menu.find((i): i is Extract<typeof i, { items: MenuSpec }> => i !== 'separator' && 'items' in i && i.label === 'Move to worktree');
    expect(move).toBeDefined();
    // The session's own project first; never where it is, another Mac's or a stray folder.
    expect(labels(move!.items)).toEqual(['Acme ›', 'Blog ›', '-', 'A chat of its own']);
    const acme = move!.items[0] as Extract<MenuSpec[number], { items: MenuSpec }>;
    // Each repo under its name, its main checkout first, then the latest.
    expect(labels(acme.items)).toEqual(['api', 'api (main checkout)', 'api-new', 'api-old', '-', 'web', 'web']);
    expect(labels((move!.items[1] as Extract<MenuSpec[number], { items: MenuSpec }>).items)).toEqual(['site']);
    // Only what Codealong can close and open again in a terminal moves, and only here.
    const has = (s: Session, l: Lane = at) => labels(sessionMenu(l, s, { ...base, worktrees })).includes('Move to worktree ›');
    expect(has(session())).toBe(true);
    expect(has(session({ surface: 'app' }))).toBe(false);
    expect(has(session({ hosted: true }))).toBe(false);
    expect(has(session({ harness: 'codex', key: 'codex:x:1' }))).toBe(false);
    expect(has(session({ resumeCommand: null }))).toBe(false);
    expect(has(session({ key: '@ab12:claude:c:1' }))).toBe(false);
    // A chat moves to a project, not to another chat.
    const chat = labels((sessionMenu(worktrees[8], session(), { ...base, worktrees }).find((i) => i !== 'separator' && 'items' in i && i.label === 'Move to worktree') as Extract<MenuSpec[number], { items: MenuSpec }>).items);
    expect(chat).toEqual(['Acme ›', 'Blog ›']);
  });

  it('hands a window that follows a moved session the copy that takes its place, and tells the rest it left', async () => {
    const to = vi.mocked(moveSessionTo);
    const settled = () => new Promise((r) => setTimeout(r, 0));
    const elsewhere = fx.lane({ id: '/r/api-new', project: 'Acme', repo: 'api', callsign: 'api-new' });
    const menuOf = (key: string, can: Partial<SessionCan> = {}) => sessionMenu(lane(), session({ key }), { ...base, worktrees: [lane(), elsewhere], ...can });
    const folders = (key: string) => menuOf(key).find((i): i is MenuGroup => i !== 'separator' && 'items' in i && i.label === 'Move to worktree');
    const run = async (key: string, can: Partial<SessionCan>, clicks = 1) => {
      const said: string[] = [];
      const left: string[] = [];
      const copies: (string | null)[] = [];
      const menu = menuOf(key, { report: { error: (m) => said.push(`error: ${m}`), info: (m) => said.push(m) }, left: (d) => left.push(d), ...(can.moving && { moving: (c) => void c.then((k) => copies.push(k)) }) });
      const item = ((menu.find((i) => i !== 'separator' && 'items' in i && i.label === 'Move to worktree') as MenuGroup).items[0] as MenuGroup).items[0] as MenuAction;
      for (let i = 0; i < clicks; i++) item.run();
      await settled();
      return { said, left, copies };
    };
    // A background job Claude Code copied into a new one: the copy takes its place, and a second
    // click while it moves, or the menu after, moves nothing again.
    to.mockClear();
    to.mockResolvedValue({ key: 'claude:c:2', message: 'Moved “Fix login” to api-new.' });
    expect(await run('claude:c:1', { moving: noop }, 2)).toEqual({ said: ['Moved “Fix login” to api-new.'], left: [], copies: ['claude:c:2'] });
    expect(to.mock.calls).toEqual([['claude:c:1', '/r/api-new']]);
    expect(folders('claude:c:1')).toBeUndefined();
    expect(await run('claude:c:3', {})).toEqual({ said: ['Moved “Fix login” to api-new.'], left: ['archived'], copies: [] });
    // Moved as itself, it stays where it's listed, can move again, and has no copy to follow.
    to.mockResolvedValue({ key: 'claude:c:4', message: 'Moved “Fix login” to api-new.' });
    expect(await run('claude:c:4', { moving: noop })).toEqual({ said: ['Moved “Fix login” to api-new.'], left: [], copies: [null] });
    expect(folders('claude:c:4')).toBeDefined();
    expect(await run('claude:c:4', {})).toEqual({ said: ['Moved “Fix login” to api-new.'], left: [], copies: [] });
    // A move that fails leaves it where it was, says why, and can be tried again.
    to.mockRejectedValue(new Error('“Fix login” hasn’t closed yet. Move it once it has.'));
    expect(await run('claude:c:5', { moving: noop })).toEqual({ said: ['error: “Fix login” hasn’t closed yet. Move it once it has.'], left: [], copies: [null] });
    expect(folders('claude:c:5')).toBeDefined();
  });

  it('follows a session moved to another account to its copy, and lets it go where the move names none', async () => {
    const to = vi.mocked(moveSession);
    const accounts = vi.mocked(getAccounts);
    const settled = () => new Promise((r) => setTimeout(r, 0));
    const later = Date.now() + 61_000;
    vi.useFakeTimers({ toFake: ['Date'] });
    // The accounts are read again a minute after they last were, as a session's actions are
    // built (for its menu, its panel's buttons and its box's account).
    const readAccounts = async (list: Awaited<ReturnType<typeof getAccounts>>, at: number) => {
      accounts.mockResolvedValueOnce(list);
      vi.setSystemTime(at);
      sessionActions(lane(), session(), base);
      await settled();
    };
    const c2 = { name: 'c2', harness: 'claude' as const, home: '~/.claude-c2', command: 'claude-c2', makesPages: true, copiesSettings: true, folderExists: true, signedIn: true, email: null, signInNote: null };
    try {
      // One turned off isn't offered.
      await readAccounts([{ ...c2, disabled: true }], later);
      expect(sessionActions(lane(), session({ key: 'claude:c:10' }), base).move).toBeUndefined();
      expect(sessionActions(lane(), session({ key: 'claude:c:10' }), base).moveAccounts).toBeUndefined();
      await readAccounts([c2], later + 61_000);
      // A window that asks which account first gets the same moves, and the one a usage limit has
      // room on, which the copy is told to carry on under.
      const asks = (s: Partial<Session>) => {
        const m = sessionActions(lane(), session({ key: 'claude:c:10', ...s }), base).moveAccounts;
        return m && { to: m.to, suggested: m.suggested, goesOn: m.goesOn };
      };
      expect(asks({})).toEqual({ to: ['c2'], suggested: null, goesOn: false });
      const limit = { meter: 'week' as const, resetsAt: null, spendCapped: false, at: 1, moveTo: 'c2' };
      expect(asks({ status: 'stopped', live: false, limit })).toEqual({ to: ['c2'], suggested: 'c2', goesOn: true });
      expect(asks({ status: 'stopped', live: false, limit: { ...limit, moveTo: null } })).toEqual({ to: ['c2'], suggested: null, goesOn: true });
      const run = async (key: string, can: Partial<SessionCan>) => {
        const left: string[] = [];
        const copies: (string | null)[] = [];
        const move = sessionActions(lane(), session({ key }), { ...base, left: (d) => left.push(d), ...(can.moving && { moving: (c) => void c.then((k) => copies.push(k)) }) }).move as MenuAction;
        expect(move.label).toBe('Move to c2');
        move.run();
        await settled();
        return { left, copies };
      };
      to.mockResolvedValue({ key: 'claude:c2:9', message: 'Moved “Fix login” to c2: it goes on in a new terminal in api.' });
      expect(await run('claude:c:11', { moving: noop })).toEqual({ left: [], copies: ['claude:c2:9'] });
      expect(to).toHaveBeenLastCalledWith('claude:c:11', 'c2');
      // Moved out, it offers no other move here.
      expect(sessionActions(lane(), session({ key: 'claude:c:11' }), base).move).toBeUndefined();
      expect(await run('claude:c:12', {})).toEqual({ left: ['archived'], copies: [] });
      // A job whose ID the service didn't say: it's archived all the same, with no copy to show.
      to.mockResolvedValue({ key: null, message: 'Moved “Fix login” to c2: it goes on as a background job.' });
      expect(await run('claude:c:13', { moving: noop })).toEqual({ left: ['archived'], copies: [null] });
    } finally {
      await readAccounts([], later + 122_000);
      vi.useRealTimers();
    }
  });

  it('never starts, ends or doubles a line', () => {
    const spec = sessionMenu(lane({ pr: null }), session({ status: 'idle', resumeCommand: null, live: false }), base);
    const l = labels(spec);
    expect(l[0]).not.toBe('-');
    expect(l[l.length - 1]).not.toBe('-');
    expect(l.join('|')).not.toContain('-|-');
  });

  it('moves a session to the top on ⌃⌘0, and puts one there back in line', () => {
    const raise = vi.mocked(raiseAhead);
    raise.mockClear();
    const acts = sessionActions(lane(), session(), base);
    expect(acts.raise?.label).toBe('Move to top');
    expect(keyed(acts, '0', true)?.action).toBe(acts.raise);
    acts.raise?.run();
    expect(raise).toHaveBeenLastCalledWith(['claude:c:1'], true);
    const top = sessionActions(lane(), session({ raised: { at: 1, agent: true, byTitle: 'Orchestrator' } }), base).raise;
    expect(top?.label).toBe('Back in line');
    top?.run();
    expect(raise).toHaveBeenLastCalledWith(['claude:c:1'], false);
    expect(sessionActions(lane(), session({ archived: true }), base).raise).toBeUndefined();
  });

  it('sets aside on ⌘E, in Mark as seen’s place, one that needs you while its side conversation answers', () => {
    const aside = vi.mocked(asideAhead);
    aside.mockClear();
    const left: string[] = [];
    const asks = session({ status: 'needs', need: 'question', excerpt: 'Asks you · Which color?' });
    const acts = sessionActions(lane(), asks, { ...base, answering: true, left: (d) => left.push(d) });
    expect(acts.seen?.label).toBe('Set aside until its side conversation answers');
    expect(chorded(acts, { key: 'e', code: 'KeyE', metaKey: true, ctrlKey: false, altKey: false, shiftKey: false })).toBe(acts.seen);
    acts.seen?.run();
    expect(aside).toHaveBeenLastCalledWith(['claude:c:1']);
    expect(left).toEqual(['set aside']);
    // With nothing answering on its side, ⌘E has nothing to do; the side box's ⌘⏎ still can.
    const idle = sessionActions(lane(), asks, base);
    expect(idle.seen).toBeUndefined();
    expect(idle.aside?.label).toBe('Set aside until its side conversation answers');
    // A finished turn is marked seen, answering or not; one set aside already isn't again.
    expect(sessionActions(lane(), session({ status: 'unread' }), { ...base, answering: true }).seen?.label).toBe('Mark as seen');
    expect(sessionActions(lane(), { ...asks, aside: 1 }, { ...base, answering: true }).seen).toBeUndefined();
  });

  it('asks on the side with ⌘⏎ and takes the session out of what waits on you until it answers', async () => {
    const [aside, seen] = [vi.mocked(asideAhead), vi.mocked(seenAhead)];
    const flush = () => new Promise((r) => setTimeout(r, 0));
    const run = async (s: Session, took: boolean) => {
      for (const m of [aside, seen]) m.mockClear();
      const order: string[] = [];
      // Shown seen at once, by what's handed it to mark them with.
      seen.mockImplementation((keys: string[], mark?: (keys: string[]) => Promise<number | null>) => (order.push('seen'), mark ? mark(keys).then(() => {}) : Promise.resolve()));
      const left: string[] = [];
      const ask = vi.fn((asSeen: boolean) => (order.push(asSeen ? 'ask seen' : 'ask'), Promise.resolve(took)));
      sessionActions(lane(), s, { ...base, left: (d) => left.push(d), failed: (why) => left.push(`failed ${why}`) }).askAside(ask);
      await flush();
      return { order, left };
    };
    // A finished turn goes on at once, shown seen, and is asked seen: the engine marks it as of
    // the question, once the copy has it, and not at all when it didn't.
    expect(await run(session({ status: 'unread' }), true)).toEqual({ order: ['seen', 'ask seen'], left: ['marked seen'] });
    // Not asked, it comes round again where the window goes through what waits.
    expect(await run(session({ status: 'unread' }), false)).toEqual({ order: ['seen', 'ask seen'], left: ['marked seen', 'failed null'] });
    // One that asks you a question is set aside once the copy has it, and not if it didn't.
    const asks = session({ status: 'needs', need: 'question', excerpt: 'Asks you · Which color?' });
    expect(await run(asks, true)).toEqual({ order: ['ask'], left: ['set aside'] });
    expect(aside).toHaveBeenLastCalledWith(['claude:c:1']);
    expect(await run(asks, false)).toEqual({ order: ['ask'], left: [] });
    expect(aside).not.toHaveBeenCalled();
    // One that doesn't wait on you is only asked, as ⌘⏎ in its own box only sends.
    expect(await run(session({ status: 'working' }), true)).toEqual({ order: ['ask'], left: [] });
    seen.mockImplementation(() => Promise.resolve());
  });

  it('checks Quiet while it’s quiet, and goes on from a turn it takes out of what waits on you', () => {
    const quiet = vi.mocked(quietAhead);
    quiet.mockClear();
    const left: string[] = [];
    const item = (s: Session) => sessionActions(lane(), s, { ...base, left: (done) => left.push(done) }).quiet;
    // A turn that ended leaves what waits on you as it's quieted.
    const ended = item(session());
    expect([ended?.label, !!ended?.checked]).toEqual(['Quiet', false]);
    ended?.run();
    expect(quiet).toHaveBeenLastCalledWith(['claude:c:1'], true);
    expect(left).toEqual(['quieted']);
    // One that needs you stays where it is, and so does one quiet already, unchecked.
    item(session({ status: 'needs', asks: true }))?.run();
    const already = item(session({ status: 'idle', quiet: true }));
    expect(already?.checked).toBe(true);
    already?.run();
    expect(quiet).toHaveBeenLastCalledWith(['claude:c:1'], false);
    expect(left).toEqual(['quieted']);
  });

  it('names where it opens when the worktree doesn’t list it', () => {
    expect(openLabel(lane(), session(), 'Ghostty')).toBe('Respond in Ghostty');
    // In the engine's words for how it opens.
    expect(openLabel(lane({ targets: [] }), session(), 'Ghostty')).toBe('Show in Ghostty');
    expect(openLabel(lane({ targets: [] }), session({ openTarget: 's|claude:c:1|resume' }), 'Ghostty')).toBe('Resume in Ghostty');
    expect(openLabel(lane({ targets: [] }), session({ openTarget: 's|claude:c:1|claude-app' }), 'Ghostty')).toBe('Open in Claude app');
  });

  it('places Open folder in after the session’s own open, and after Resume in / Carry on in when stopped', () => {
    const here = lane({ targets: [...lane().targets, fx.target({ id: 'l|editor', label: 'Open in Cursor' })] });
    const editors = { apps: { editor: 'Cursor', editorId: 'cursor', editors: [{ id: 'cursor', name: 'Cursor' }, { id: 'zed', name: 'Zed' }] }, icons: {} };
    const can: SessionCan = { ...base, editors };
    // Live: directly after its own open.
    expect(labels(sessionMenu(here, session(), can)).slice(0, 2)).toEqual(['Respond in Ghostty', 'Open folder in ›']);
    // Stopped with multiple resume places: after its own open and Resume in (or Carry on in when cut off).
    vi.mocked(startAccountsNow).mockReturnValue([
      fx.startAccount({
        places: [
          { place: 'terminal', label: 'Ghostty' },
          { place: 'background', label: 'Background' },
        ],
        resumes: ['terminal', 'background'],
      }),
    ]);
    try {
      const stopped = session({ live: false, pid: null, status: 'idle', openTarget: 's|claude:c:1|resume' });
      expect(labels(sessionMenu(here, stopped, can)).slice(0, 3)).toEqual(['Resume in Ghostty', 'Resume in ›', 'Open folder in ›']);
      const cutOff = session({ ...stopped, status: 'error', cutOff: 1 });
      expect(labels(sessionMenu(here, cutOff, can)).slice(0, 3)).toEqual(['Resume in Ghostty', 'Carry on in ›', 'Open folder in ›']);
    } finally {
      vi.mocked(startAccountsNow).mockReturnValue([]);
    }
  });
});

describe('what each window can do to a session', () => {
  const PLACES = Object.keys(SESSION_CAN) as CanPlace[];
  /** A menu a session has: the parts in `SESSION_CAN` it's made of, and what those list that the
   *  window passes as undefined for the session the menu is for. */
  interface Menu {
    of: CanPlace[];
    without?: (keyof SessionCan)[];
  }
  // Every menu a session has, as its window makes it once it has the board. What a window passes
  // or not by the session is written here as the window has it, with where: nothing mounts the
  // windows, so this is held to them by reading.
  const MENUS: Record<string, Menu> = {
    'the main window': { of: ['main'] },
    'the quick panel': { of: ['quick'] },
    'a pill on the bar': { of: ['bar'] },
    'a card on the desktop': { of: ['desktop'] },
    // cardHost.ts: the peek's links and the notch's. The waiting window's have its own menu, below.
    'a session a conversation links, in the peek and the notch': { of: ['link'] },
    'a session a PR’s card names in the notch': { of: ['notchReview'] },
    'a session a todo’s card names in the notch': { of: ['notchTodo'] },
    // Waiting.tsx `can`: the session shown is answered where it is, so Reply here is for another;
    // going on from it (`left`, `failed`) is for the one shown.
    'the waiting window’s card': { of: ['waiting', 'card'], without: ['reply'] },
    // SessionCard.tsx `whole`: the terminal under the box is the session shown's.
    'a session the waiting window’s card names': { of: ['waiting', 'card'], without: ['left', 'failed', 'ownTerminal'] },
    // Waiting.tsx `sessionMenu`: a PR's card's sessions, a todo's, and what a conversation links.
    'a session a PR’s or a todo’s card names in the waiting window': { of: ['waiting', 'waitingNamed'], without: ['left', 'failed'] },
    'a session a conversation links, in the waiting window': { of: ['waiting', 'waitingNamed'], without: ['left', 'failed'] },
    'the notch’s card': { of: ['notch', 'card'] },
    'a session the notch’s card names': { of: ['notchNamed', 'card'], without: ['ownTerminal'] },
    // Peek.tsx `putBack`: only from a window the session is kept in.
    'the peek’s card': { of: ['peek', 'card'], without: ['putBack'] },
    'a kept session’s window': { of: ['peek', 'card'] },
    'a session the peek’s card names': { of: ['peekNamed', 'card'], without: ['ownTerminal'] },
  };
  // Every capability, and the places that pass it. It's written by capability so that a new one
  // doesn't compile until it's decided for every place, and so that a window that goes without
  // one stands out on its line.
  const PASSED: Record<keyof SessionCan, readonly CanPlace[]> = {
    report: ['main', 'quick', 'bar', 'desktop', 'link', 'notchReview', 'notchTodo', 'waiting', 'notch', 'peek', 'notchNamed', 'peekNamed'],
    open: ['main', 'quick', 'bar', 'desktop', 'link', 'notchReview', 'notchTodo', 'waiting', 'notch', 'peek', 'notchNamed', 'peekNamed'],
    terminal: ['main', 'quick', 'bar', 'desktop', 'link', 'notchReview', 'notchTodo', 'card', 'waitingNamed'],
    editors: ['main', 'quick', 'bar', 'desktop', 'link', 'notchReview', 'notchTodo', 'card', 'waitingNamed'],
    desktop: ['main', 'quick', 'bar', 'desktop', 'link', 'notchReview', 'notchTodo', 'card', 'waitingNamed'],
    reveal: ['quick', 'bar', 'desktop', 'link', 'notchReview', 'notchTodo', 'waiting', 'notch', 'peek', 'notchNamed', 'peekNamed'],
    rename: ['main', 'quick', 'bar', 'desktop', 'waiting', 'notch', 'peek'],
    left: ['main', 'bar', 'link', 'waiting', 'notch', 'peek'],
    labels: ['main', 'quick', 'link', 'card', 'waitingNamed'],
    reply: ['quick', 'bar', 'desktop', 'waiting'],
    doneHelpers: ['main', 'quick', 'card', 'waitingNamed'],
    worktrees: ['main', 'quick', 'waiting'],
    failed: ['main', 'waiting', 'notch'],
    pin: ['main', 'card'],
    mergeDone: ['main', 'card'],
    ownTerminal: ['main', 'card'],
    keys: ['main', 'card'],
    answering: ['card'],
    heading: ['bar'],
    putBack: ['peek'],
    windows: ['main'],
    newSession: ['main'],
    todos: ['main'],
    archive: ['main'],
    undoDone: ['main'],
    moving: ['main'],
    boards: ['main'],
    related: ['main'],
  };

  // A session of these tests' own (one another test moved offers no other move meanwhile), with
  // a helper still at work.
  const mine = session({ key: 'claude:c:everywhere' });
  const helper = session({ key: 'claude:c:helping', id: 'helping', title: 'Check the migration', status: 'working', helper: { parent: mine.key } });
  const lanes = [lane({ sessions: [mine, helper] }), lane({ id: '/r/web', callsign: 'web', path: '/r/web' })];
  const apps = { terminal: 'Ghostty', terminalId: 'ghostty', editor: 'Cursor', editorId: 'cursor', editors: [{ id: 'cursor', name: 'Cursor' }] };
  // Everything a window could pass, for a session whose PR is ready to merge.
  const every: Required<SessionCan> = {
    report,
    open: noop,
    terminal: 'Ghostty',
    answering: false,
    heading: true,
    reveal: noop,
    putBack: noop,
    windows: new Set(),
    reply: noop,
    editors: { apps, icons: {} },
    newSession: noop,
    ownTerminal: { run: noop, has: false },
    todos: [{ label: 'Add to todo', items: [] }],
    rename: noop,
    worktrees: lanes,
    pin: { label: 'Pin session', run: noop },
    desktop: new Set(),
    archive: noop,
    undoDone: noop,
    left: noop,
    failed: noop,
    moving: noop,
    boards: [{ label: 'Add to board', items: [] }],
    related: [{ label: 'Show finished helper', run: noop }],
    labels: [{ id: 'l1', name: 'urgent', color: 'red' }],
    mergeDone: { prs: [fx.prItem({ repoSlug: 'acme/api', number: 7, mine: true, merge: 'clean', primary: mine.key })], ask: () => true },
    doneHelpers: { lanes, ask: noop },
    keys: 'chords',
  };
  /** The `SessionCan` a menu is made with: what its parts list, less what the window passes as
   *  undefined there, `over` in place of the fixture's. */
  const canOf = ({ of, without = [] }: Menu, over: Partial<SessionCan> = {}): SessionCan => {
    const passed = of.flatMap((p) => [...SESSION_CAN[p]]).filter((k) => !without.includes(k));
    return { ...base, ...Object.fromEntries(passed.map((k) => [k, k in over ? over[k] : every[k]])) };
  };

  it('passes in each place what it always has: one dropped there is a menu item gone from that window', () => {
    const listed = Object.fromEntries(PLACES.map((p) => [p, [...SESSION_CAN[p]].sort()]));
    const passed = Object.fromEntries(PLACES.map((p) => [p, (Object.keys(PASSED) as (keyof SessionCan)[]).filter((k) => PASSED[k].includes(p)).sort()]));
    expect(listed).toEqual(passed);
  });

  it('makes every menu of parts that pass each capability once, the three no menu goes without among them', () => {
    for (const [menu, { of, without = [] }] of Object.entries(MENUS)) {
      const all = of.flatMap((p) => [...SESSION_CAN[p]]);
      expect(all.filter((k, i) => all.indexOf(k) !== i), menu).toEqual([]);
      expect(all, menu).toEqual(expect.arrayContaining(['report', 'open', 'terminal']));
      // What a window leaves undefined for a session is something its parts list.
      expect(without.filter((k) => !all.includes(k)), menu).toEqual([]);
    }
    // And every place is part of some menu.
    expect(PLACES.filter((p) => !Object.values(MENUS).some((m) => m.of.includes(p)))).toEqual([]);
  });

  it('offers in each window what that window can do, on top of what every window can', () => {
    const plain = labels(sessionMenu(lane(), mine, base));
    const more = (menu: Menu) => labels(sessionMenu(lane(), mine, canOf(menu))).filter((l) => l !== '-' && !plain.includes(l));
    expect(Object.fromEntries(Object.entries(MENUS).map(([name, menu]) => [name, more(menu)]))).toEqual({
      'the main window': ['New terminal', 'Open in its own window', 'Move to worktree ›', 'Add to todo ›', 'Add to board ›', 'Labels ›', 'Show finished helper', 'Rename…', 'Pin session', 'Pin to desktop', 'Merge #7 and done…'],
      'the quick panel': ['Open in main window', 'Reply here', 'Move to worktree ›', 'Labels ›', 'Rename…', 'Pin to desktop'],
      'a pill on the bar': ['Fix login · api', 'Open in main window', 'Reply here', 'Rename…', 'Pin to desktop'],
      'a card on the desktop': ['Open in main window', 'Reply here', 'Rename…', 'Pin to desktop'],
      'a session a conversation links, in the peek and the notch': ['Open in main window', 'Labels ›', 'Pin to desktop'],
      'a session a PR’s card names in the notch': ['Open in main window', 'Pin to desktop'],
      'a session a todo’s card names in the notch': ['Open in main window', 'Pin to desktop'],
      'the waiting window’s card': ['New terminal', 'Open in main window', 'Move to worktree ›', 'Labels ›', 'Rename…', 'Pin session', 'Pin to desktop', 'Merge #7 and done…'],
      'a session the waiting window’s card names': ['Open in main window', 'Reply here', 'Move to worktree ›', 'Labels ›', 'Rename…', 'Pin session', 'Pin to desktop', 'Merge #7 and done…'],
      'a session a PR’s or a todo’s card names in the waiting window': ['Open in main window', 'Reply here', 'Move to worktree ›', 'Labels ›', 'Rename…', 'Pin to desktop'],
      'a session a conversation links, in the waiting window': ['Open in main window', 'Reply here', 'Move to worktree ›', 'Labels ›', 'Rename…', 'Pin to desktop'],
      'the notch’s card': ['New terminal', 'Open in main window', 'Labels ›', 'Rename…', 'Pin session', 'Pin to desktop', 'Merge #7 and done…'],
      'a session the notch’s card names': ['Open in main window', 'Labels ›', 'Pin session', 'Pin to desktop', 'Merge #7 and done…'],
      'the peek’s card': ['New terminal', 'Open in main window', 'Labels ›', 'Rename…', 'Pin session', 'Pin to desktop', 'Merge #7 and done…'],
      // Kept on the desktop, it goes back to the main window in place of showing there.
      'a kept session’s window': ['New terminal', 'Put back in main window', 'Labels ›', 'Rename…', 'Pin session', 'Pin to desktop', 'Merge #7 and done…'],
      'a session the peek’s card names': ['Open in main window', 'Labels ›', 'Pin session', 'Pin to desktop', 'Merge #7 and done…'],
    });
  });

  it('asks what becomes of a helper still at work, at Mark as done, only where the window has the dialog for it', async () => {
    const done = vi.mocked(doneAhead);
    done.mockReset();
    done.mockResolvedValue({ done: true, message: 'Done.', at: 5 });
    const asks = 'asks about Check the migration';
    const goes = 'marks it done without asking';
    const how: Record<string, string> = {};
    for (const [name, menu] of Object.entries(MENUS)) {
      done.mockClear();
      const asked: DoneHelpersAsk[] = [];
      sessionActions(lane(), mine, canOf(menu, { doneHelpers: { lanes, ask: (a) => void asked.push(a) } })).done?.run();
      await new Promise((r) => setTimeout(r, 0));
      // Asked, nothing goes to the engine until it's answered; not asked, it's marked done at
      // once with no helper chosen to end or to keep, and the engine makes the one at work your
      // own. The choice is read for what it says: any that names none asks the engine the same.
      const went = done.mock.calls.map((call) => call[3]);
      const none = went.length === 1 && !!went[0] && went[0].end.length === 0 && went[0].keep.length === 0;
      if (asked.length === 1 && went.length === 0) how[name] = `asks about ${asked[0].helpers.map((h) => h.title).join(', ')}`;
      else how[name] = asked.length === 0 && none ? goes : `asked ${asked.length}, marked done ${JSON.stringify(went)}`;
    }
    // The mock as the other tests find it.
    done.mockReset();
    expect(how).toEqual({
      'the main window': asks,
      'the quick panel': asks,
      // No dialog in these: the helper is made a session of your own, unasked (actions.rs
      // `finish_helpers`).
      'a pill on the bar': goes,
      'a card on the desktop': goes,
      'a session a conversation links, in the peek and the notch': goes,
      'a session a PR’s card names in the notch': goes,
      'a session a todo’s card names in the notch': goes,
      'the waiting window’s card': asks,
      'a session the waiting window’s card names': asks,
      'a session a PR’s or a todo’s card names in the waiting window': asks,
      // The same link asks in the waiting window, whose menu has the dialog.
      'a session a conversation links, in the waiting window': asks,
      'the notch’s card': asks,
      'a session the notch’s card names': asks,
      'the peek’s card': asks,
      'a kept session’s window': asks,
      'a session the peek’s card names': asks,
    });
  });
});

describe('several sessions picked together', () => {
  // Unread with a terminal of its own; idle with none; archived.
  const a = { lane: lane(), session: session() };
  const b = { lane: lane(), session: session({ key: 'claude:c:2', id: 'b2', title: 'Add tests', status: 'idle', live: false, pid: null }) };
  const c = { lane: lane(), session: session({ key: 'claude:c:3', id: 'c3', title: 'Old work', status: 'idle', live: false, pid: null, archived: true }) };
  const settled = () => new Promise((r) => setTimeout(r, 0));

  it('says how many each action takes, and leaves out what takes none', () => {
    expect(labels(sessionsMenu([a, b], { report, pin: { label: 'Pin 2 sessions', run: noop } }))).toEqual([
      'Mark 1 session as seen',
      // The one whose turn you've seen; an archived one stays as it is.
      'Mark 1 session as unread',
      'Pin 2 sessions',
      'Move 2 sessions to top',
      'Snooze 2 sessions ›',
      'Quiet 2 sessions',
      'Close 1 session',
      'Mark 2 sessions as done',
      'Archive 2 sessions',
      '-',
      'Copy session IDs',
    ]);
    expect(sessionsActions([a, b, c], { report }).takes).toEqual({ carryOn: 0, stop: 0, seen: 1, unread: 1, snooze: 2, close: 1, done: 2, archive: 2 });
    // All archived: they come out of it together, and aren't pinned, snoozed or done.
    const d = { ...c, session: { ...c.session, key: 'claude:c:4', id: 'd4' } };
    expect(labels(sessionsMenu([c, d], { report, pin: { label: 'Pin 2 sessions', run: noop } }))).toEqual(['Unarchive 2 sessions', '-', 'Copy session IDs']);
  });

  it('puts them on a board where the window has boards, where one session’s menu has it', () => {
    const boards = [{ label: 'Add 2 sessions to board', items: [{ label: 'Release', run: noop }] }];
    expect(labels(sessionsMenu([c, { ...c, session: { ...c.session, key: 'claude:c:4' } }], { report, boards }))).toEqual(['Add 2 sessions to board ›', '-', 'Unarchive 2 sessions', '-', 'Copy session IDs']);
  });

  it('runs on the chords one session’s actions do, shown beside them', () => {
    const acts = sessionsActions([a, b], { report, keys: 'chords' });
    expect(keyed(acts, 'a', true)?.action).toBe(acts.archive);
    expect(keyed(acts, 'w', true)?.action).toBe(acts.close);
    expect(keyed(acts, '2', true)?.id).toBe('snooze');
    // Rename is one at a time.
    expect(keyed(acts, 'r', true)).toBeNull();
    const chords = sessionsMenu([a, b], { report, keys: 'chords' }).flatMap((i) => (i !== 'separator' && !('items' in i) && i.accelerator ? [i.accelerator] : []));
    expect(chords).toEqual(['Cmd+E', 'Ctrl+Cmd+U', 'Ctrl+Cmd+0', 'Ctrl+Cmd+W', 'Shift+Cmd+E', 'Ctrl+Cmd+A']);
    expect(keyed(acts, 'u', true)?.action).toBe(acts.unread);
  });

  it('puts a label on the ones without it, or takes it off them all once every one wears it', () => {
    const label = vi.mocked(labelAhead);
    label.mockClear();
    const bug: Label = { id: 'l1', name: 'Bug', color: 'red' };
    const ui: Label = { id: 'l2', name: 'UI', color: 'teal' };
    // One on another Mac wears its own Mac's label of that name, and one this Mac has none of.
    const there: Label = { id: 'l-there', name: 'bug', color: 'blue' };
    const docs: Label = { id: 'l-docs', name: 'Docs', color: 'green' };
    const one = { ...a, session: { ...a.session, labels: [bug] } };
    const other = { ...b, session: { ...b.session, labels: [there, docs] } };
    const menu = sessionsActions([one, other], { report, labels: [bug, ui] }).labels;
    expect(menu?.items.map((i) => (i === 'separator' ? '-' : [i.label, !!('checked' in i && i.checked)]))).toEqual([
      ['Bug', true],
      ['UI', false],
      ['Docs', false],
    ]);
    const run = (name: string) => menu?.items.forEach((i) => i !== 'separator' && i.label === name && 'run' in i && i.run());
    run('Bug');
    expect(label).toHaveBeenLastCalledWith(['claude:c:1', 'claude:c:2'], [], ['Bug']);
    run('UI');
    expect(label).toHaveBeenLastCalledWith(['claude:c:1', 'claude:c:2'], [ui], []);
    // No labels and none worn: nothing to offer, as one's header makes the first.
    expect(sessionsActions([a], { report }).labels).toBeUndefined();
    expect(sessionActions(lane(), one.session, { ...base, labels: [] }).labels?.items).toHaveLength(1);
    expect(sessionActions(lane(), one.session, base).relabel).toBeUndefined();
  });

  it('quiets them all, or makes them all heard again once every one is quiet', () => {
    const quiet = vi.mocked(quietAhead);
    quiet.mockClear();
    const left: [string, string[]][] = [];
    const can: SessionsCan = { report, left: (done, keys) => left.push([done, keys]) };
    const some = sessionsActions([a, b, c], can).quiet;
    expect([some?.label, !!some?.checked]).toEqual(['Quiet 2 sessions', false]);
    some?.run();
    // The archived one is left as it is, and only the turn that ended leaves what waits on you.
    expect(quiet).toHaveBeenLastCalledWith(['claude:c:1', 'claude:c:2'], true);
    expect(left).toEqual([['quieted', ['claude:c:1']]]);
    const both = [a, b].map((x) => ({ ...x, session: { ...x.session, quiet: true } }));
    const all = sessionsActions(both, can).quiet;
    expect(all?.checked).toBe(true);
    all?.run();
    expect(quiet).toHaveBeenLastCalledWith(['claude:c:1', 'claude:c:2'], false);
    expect(left).toHaveLength(1);
  });

  it('archives the ones not archived yet, and says they left', () => {
    const archived: [string[], boolean][] = [];
    const left: [string, string[]][] = [];
    const can: SessionsCan = { report, archive: (keys, on) => archived.push([keys, on]), left: (done, keys) => left.push([done, keys]) };
    sessionsActions([a, b, c], can).archive?.run();
    expect(archived).toEqual([[['claude:c:1', 'claude:c:2'], true]]);
    expect(left).toEqual([['archived', ['claude:c:1', 'claude:c:2']]]);
  });

  it('closes the ones it can, one after another, and says how many', async () => {
    const said: string[] = [];
    const left: [string, string[]][] = [];
    sessionsActions([a, b], { report: { error: (m) => said.push(`error: ${m}`), info: (m) => said.push(m) }, left: (done, keys) => left.push([done, keys]) }).close?.run();
    await settled();
    expect(left).toEqual([['closed', ['claude:c:1']]]);
    expect(said).toEqual(['Closed 1 session.']);
  });

  it('carries on the ones cut off, one after another, each where Carry on would take it alone', async () => {
    const resume = vi.mocked(resumeSession);
    resume.mockReset();
    let going = 0;
    resume.mockImplementation(async (key) => {
      // One at a time: the next waits until this one's terminal has opened.
      expect(going++).toBe(0);
      await new Promise((r) => setTimeout(r, 1));
      going--;
      if (key === 'claude:c:8') throw new Error('Its folder is gone.');
      // The app only opens one of its own, with the prompt on the clipboard.
      if (key === 'claude:c:11') return { message: '“Cut 11” is open in the Claude app and the prompt is on the clipboard. Paste it there to send it.', opened: true };
      return { message: 'Resumed.', opened: false };
    });
    vi.mocked(startAccountsNow).mockReturnValue([
      fx.startAccount({
        places: [
          { place: 'background', label: 'Background' },
          { place: 'terminal', label: 'Ghostty' },
          { place: 'claude-app', label: 'Claude app' },
        ],
        resumes: ['background', 'terminal', 'claude-app'],
      }),
    ]);
    const said: string[] = [];
    const cut = (n: number, over: Partial<Session> = {}) => ({ lane: lane(), session: session({ key: `claude:c:${n}`, id: `${n}`, title: `Cut ${n}`, status: 'needs', live: false, pid: null, cutOff: 1, ...over }) });
    try {
      // Not one running, nor one on another Mac, nor one that wasn't cut off.
      const picked = [cut(7), a, cut(8), cut(9, { live: true, pid: 9 }), cut(10, { key: '@mini:claude:c:10' }), b, cut(11, { surface: 'app' })];
      const acts = sessionsActions(picked, { report: { error: (m) => said.push(`error: ${m}`), info: (m) => said.push(m) } });
      expect(acts.takes.carryOn).toBe(3);
      expect(labels(sessionsMenu(picked, { report })).slice(0, 2)).toEqual(['Carry on 3 sessions', '-']);
      acts.carryOn?.run();
      await vi.waitFor(() => expect(said).toHaveLength(1));
      expect(resume.mock.calls).toEqual([
        ['claude:c:7', { place: 'terminal', model: null, effort: null }],
        ['claude:c:8', { place: 'terminal', model: null, effort: null }],
        ['claude:c:11', { place: 'claude-app', model: null, effort: null }],
      ]);
      expect(said).toEqual([
        'error: Resumed 1 session to carry on. “Cut 11” is open in the Claude app and the prompt is on the clipboard. Paste it there to send it. Couldn’t carry on “Cut 8”: Its folder is gone.',
      ]);
      expect(sessionsActions([a, b], { report }).carryOn).toBeUndefined();
    } finally {
      vi.mocked(startAccountsNow).mockReturnValue([]);
    }
  });

  it('marks them done together, holding the working ones until asked again', async () => {
    const ask = vi.mocked(doneAhead);
    ask.mockReset();
    ask.mockResolvedValue({ done: true, message: 'Done.', at: 5 });
    const said: string[] = [];
    const undone: [string[], string][] = [];
    const left: [string, string[]][] = [];
    const can: SessionsCan = { report: { error: (m) => said.push(`error: ${m}`), info: (m) => said.push(m) }, undoDone: (keys, m) => undone.push([keys, m]), left: (done, keys) => left.push([done, keys]) };
    const busy = { lane: lane(), session: session({ key: 'claude:c:5', id: 'e5', title: 'Busy', status: 'working' }) };
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(2_000_000);
    try {
      sessionsActions([a, b, busy], can).done?.run();
      await settled();
      // Each waits on the one before, so their terminals close one after another.
      expect(ask.mock.calls.map(([key, stop]) => [key, stop])).toEqual([
        ['claude:c:1', false],
        ['claude:c:2', false],
      ]);
      expect(ask.mock.calls[0][2]).toBeUndefined();
      expect(ask.mock.calls[1][2]).toBe(ask.mock.results[0].value);
      expect(left).toEqual([['done', ['claude:c:1', 'claude:c:2']]]);
      expect(undone).toEqual([[['claude:c:1', 'claude:c:2'], 'Done with 2 sessions. “Busy” is still working. Mark it done again to end that too.']]);
      // Again soon after, the working one ends too.
      ask.mockClear();
      sessionsActions([busy], can).done?.run();
      await settled();
      expect(ask).toHaveBeenLastCalledWith('claude:c:5', true, undefined, NONE_CHOSEN);
      // All of them working: nothing's asked, and it says so.
      ask.mockClear();
      said.length = 0;
      const busy2 = { lane: lane(), session: session({ key: 'claude:c:6', id: 'f6', title: 'Busy too', status: 'working' }) };
      vi.setSystemTime(2_000_000 + DONE_HOLD_MS + 1);
      sessionsActions([busy, busy2], can).done?.run();
      expect(ask).not.toHaveBeenCalled();
      expect(said).toEqual(['“Busy” and “Busy too” are still working. Mark them done again to end that too.']);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('worktrees picked together', () => {
  // One with a turn that ended and one idle, and one with nothing to mark seen.
  const api = lane({ sessions: [session(), session({ key: 'claude:c:2', id: 'b2', status: 'idle' })] });
  const web = lane({ id: '/r/web', callsign: 'web', path: '/r/web', branch: 'fix-nav', pr: null, sessions: [session({ key: 'claude:c:3', id: 'c3', status: 'idle' })] });
  const pin = { label: 'Pin 2 worktrees', run: noop };

  it('marks their sessions seen, as E does on one, and pins, reuses and copies them', () => {
    expect(labels(lanesMenu([api, web], { report, pin }))).toEqual(['Mark 1 session as seen', '-', 'Reuse for new work', '-', 'Pin 2 worktrees', '-', 'Copy paths', 'Copy branch names']);
    const acts = lanesActions([api, web], { report, pin, keys: true });
    expect(acts.takes).toEqual({ seen: 1 });
    // P pins them and E marks seen, as on one worktree; nothing else takes a key.
    expect(keyed(acts, 'p')?.action).toBe(pin);
    expect(keyed(acts, 'e')?.action).toBe(acts.seen);
    expect(keyed(acts, 'a')).toBeNull();
  });

  it('leaves out Mark as seen where none of their sessions has a turn that ended', () => {
    expect(labels(lanesMenu([web], { report, pin }))).toEqual(['Reuse for new work', '-', 'Pin 2 worktrees', '-', 'Copy path', 'Copy branch name']);
  });

  it('puts them on a board where the window has boards, where one worktree’s menu has it', () => {
    const boards = [{ label: 'Add 2 worktrees to board', items: [{ label: 'Release', run: noop }] }];
    expect(labels(lanesMenu([api, web], { report, boards })).slice(0, 4)).toEqual(['Mark 1 session as seen', '-', 'Add 2 worktrees to board ›', '-']);
  });
});

describe('PRs picked together', () => {
  const pr = (over: Partial<PrItem> = {}): PrItem => fx.prItem({ key: 'acme/api#7', number: 7, url: 'https://github.com/acme/api/pull/7', head: 'fix-login', reviewRequested: true, ...over });
  const asked = pr();
  const mine = pr({ key: 'acme/api#8', number: 8, url: 'https://github.com/acme/api/pull/8', head: 'add-tests', reviewRequested: false, mine: true });
  const merged = pr({ key: 'acme/api#9', number: 9, url: 'https://github.com/acme/api/pull/9', head: 'old', state: 'merged' });
  const other = pr({ key: 'acme/api#10', number: 10, url: 'https://github.com/acme/api/pull/10', head: 'docs', reviewRequested: false });

  it('approves any open PR but your own, snoozes the ones that ask for your review, and says how many', () => {
    const approved: string[][] = [];
    const can = { report, approve: (prs: PrItem[]) => approved.push(prs.map((p) => p.key)), pin: { label: 'Pin 4 PRs', run: noop } };
    expect(labels(prsMenu([asked, mine, merged, other], can))).toEqual(['Open 4 PRs on GitHub', '-', 'Approve 2 PRs', 'Snooze 1 review ›', '-', 'Pin 4 PRs', '-', 'Copy links', 'Copy branch names']);
    const acts = prsActions([asked, mine, merged, other], can);
    expect(acts.takes).toEqual({ approve: 2, snooze: 1 });
    acts.approve?.run();
    // A merged PR asked of you is past reviewing, and GitHub doesn't let you approve your own.
    expect(approved).toEqual([['acme/api#7', 'acme/api#10']]);
    expect(keyed(acts, 'p')?.action).toBe(can.pin);
  });

  it('snoozes a merge of yours that waits on you with the reviews', () => {
    const waits = pr({ key: 'acme/api#11', number: 11, url: 'https://github.com/acme/api/pull/11', head: 'ready', reviewRequested: false, mine: true, waitsToMerge: true });
    expect(labels(prsMenu([asked, waits, mine], { report }))).toContain('Snooze 2 PRs ›');
    expect(prsActions([asked, waits, mine], { report }).takes).toMatchObject({ snooze: 2 });
  });

  it('offers no approval where the window can’t give one, nor where none can be given', () => {
    expect(prsActions([asked, other], { report }).approve).toBeUndefined();
    expect(labels(prsMenu([mine, merged], { report, approve: noop }))).toEqual(['Open 2 PRs on GitHub', '-', 'Copy links', 'Copy branch names']);
  });

  it('puts them on a board where the window has boards, where one PR’s menu has it', () => {
    const boards = [{ label: 'Add 2 PRs to board', items: [{ label: 'Release', run: noop }] }];
    expect(labels(prsMenu([mine, merged], { report, boards }))).toEqual(['Open 2 PRs on GitHub', '-', 'Add 2 PRs to board ›', '-', 'Copy links', 'Copy branch names']);
  });
});

describe('Merge and done', () => {
  const ready = (over: Partial<PrItem> = {}): PrItem => fx.prItem({ key: 'acme/api#7', repoSlug: 'acme/api', number: 7, state: 'approved', merge: 'clean', primary: 'claude:c:1', ...over });

  it('is for the primary session of a PR that’s ready to merge, and no other', () => {
    const s = session();
    expect(mergesWith([ready()], s)?.key).toBe('acme/api#7');
    // Another session is its primary; the PR waits on a review, its checks, its conflicts; it's merged.
    expect(mergesWith([ready({ primary: 'claude:c:2' })], s)).toBeUndefined();
    expect(mergesWith([ready({ primary: undefined })], s)).toBeUndefined();
    expect(mergesWith([ready({ merge: 'blocked' })], s)).toBeUndefined();
    expect(mergesWith([ready({ checks: fx.checks({ pending: 1 }) })], s)).toBeUndefined();
    expect(mergesWith([ready({ merge: 'conflicts' })], s)).toBeUndefined();
    expect(mergesWith([ready({ state: 'draft' })], s)).toBeUndefined();
    expect(mergesWith([ready({ state: 'merged' })], s)).toBeUndefined();
    // An archived session is done with already.
    expect(mergesWith([ready()], { ...s, archived: true })).toBeUndefined();
    // One that took the PR over from a helper it started has work of its own, picked since or
    // not: the PR waits apart.
    expect(mergesWith([ready({ primaryBy: 'helper', takenOver: true })], s)).toBeUndefined();
    expect(mergesWith([ready({ primaryBy: 'picked', takenOver: true })], s)).toBeUndefined();
    expect(mergesWith([ready({ primaryBy: 'picked' })], s)?.key).toBe('acme/api#7');
    // The one that's ready, of several it's the primary of.
    expect(mergesWith([ready({ key: 'acme/api#6', number: 6, merge: 'behind' }), ready()], s)?.number).toBe(7);
  });

  it('asks in the window’s dialog, from the menu, after Mark as done', () => {
    const asked: MergeDoneAsk[] = [];
    const can: SessionCan = { ...base, mergeDone: { prs: [ready()], ask: (a) => asked.push(a) > 0 } };
    const menu = labels(sessionMenu(lane(), session(), can));
    expect(menu.slice(menu.indexOf('Mark as done'), menu.indexOf('Archive') + 1)).toEqual(['Mark as done', 'Merge #7 and done…', 'Archive']);
    sessionActions(lane(), session(), can).mergeDone?.run();
    expect(asked.map((a) => [a.session.key, a.pr.key])).toEqual([['claude:c:1', 'acme/api#7']]);
    // Not where the window has no dialog for it, nor for a PR that isn't ready.
    expect(sessionActions(lane(), session(), base).mergeDone).toBeUndefined();
    expect(sessionActions(lane(), session(), { ...can, mergeDone: { prs: [ready({ merge: 'blocked' })], ask: () => true } }).mergeDone).toBeUndefined();
  });

  const commit: MergeForm = { method: 'squash', title: 'Add login (#7)', body: '* Add login', email: null, deleteBranch: true };
  const merged = (done: Partial<MergeDone>): MergeDone => ({ note: null, shownAt: 4, ...done });
  const flush = () => new Promise((r) => setTimeout(r, 0));

  it('goes on as it’s confirmed, and says what the merge and the session’s ending came to as Mark as done says it', async () => {
    const said: string[] = [];
    const left: string[] = [];
    const undone: string[] = [];
    let asked: MergeDoneAsk | undefined;
    const can: SessionCan = { ...base, report: { error: (m) => said.push(`error: ${m}`), info: (m) => said.push(m) }, left: (d) => left.push(d), mergeDone: { prs: [ready()], ask: (a) => !!(asked = a) } };
    /** The dialog the session's action opened, confirmed, GitHub answering `d`. */
    const confirm = async (c: SessionCan, d: MergeDone) => {
      sessionActions(lane(), session(), c).mergeDone?.run();
      asked?.then(Promise.resolve(d), 'Merged #7.', commit);
      await flush();
    };

    // The main window: gone on before GitHub answers; then Undo, with what the merge said first.
    let answer = (_: MergeDone) => {};
    sessionActions(lane(), session({ status: 'working' }), { ...can, undoDone: (m) => undone.push(m) }).mergeDone?.run();
    asked?.then(new Promise((yes) => (answer = yes)), 'Squashed and merged #7.', commit);
    expect(left).toEqual(['done']);
    expect(undone).toEqual([]);
    answer(merged({ done: { done: true, message: 'Done with “Fix login”: its turn is stopped, its terminal is closed and it’s archived.', at: 5 } }));
    await flush();
    expect(undone).toEqual(['Squashed and merged #7. Done with “Fix login”: its turn is stopped, its terminal is closed and it’s archived.']);
    expect(said).toEqual([]);

    // A floating window says only what didn't go as asked: a branch that stayed, as the engine
    // says it, and a terminal left open.
    await confirm(can, merged({ note: 'Merged #7, but couldn’t delete its branch: Reference does not exist', done: { done: true, message: 'Done with “Fix login”: it’s archived.', at: 6 } }));
    await confirm(can, merged({ done: { done: true, message: 'Done with “Fix login”: it’s archived. It’s still open in the Claude app.', noted: true, at: 7 } }));
    await confirm(can, merged({ done: { done: true, message: 'Done with “Fix login”: it’s archived.', at: 8 } }));
    expect(said).toEqual(['Merged #7, but couldn’t delete its branch: Reference does not exist Done with “Fix login”: it’s archived.', 'Merged #7. Done with “Fix login”: it’s archived. It’s still open in the Claude app.']);

    // One the engine didn't mark done is back as it was (ahead.ts), and says why after the merge
    // that went.
    said.length = 0;
    await confirm(can, merged({ doneError: 'That session isn’t on the board any more.' }));
    expect(said).toEqual(['error: Merged #7. That session isn’t on the board any more.']);
  });

  it('comes back with the commit as written and why where GitHub refuses the merge', async () => {
    const said: string[] = [];
    const asks: MergeDoneAsk[] = [];
    let inFront = true;
    const can: SessionCan = { ...base, report: { error: (m) => said.push(`error: ${m}`), info: (m) => said.push(m) }, left: noop, mergeDone: { prs: [ready()], ask: (a) => asks.push(a) > 0 && inFront } };
    sessionActions(lane(), session(), can).mergeDone?.run();
    asks[0].then(Promise.reject(new Error('Couldn’t merge #7: Head branch was modified. Review and try the merge again.')), 'Merged #7.', commit);
    await flush();
    expect(asks[1]).toMatchObject({ pr: { key: 'acme/api#7' }, session: { key: 'claude:c:1' }, refused: { commit, why: 'Couldn’t merge #7: Head branch was modified. Review and try the merge again.' } });
    // The main window's dialog is in front of you: it says why itself.
    expect(said).toEqual([]);

    // Refused again, where the window may not show the dialog then (a card's): it says why too.
    inFront = false;
    asks[1].then(Promise.reject(new Error('Couldn’t merge #7: Base branch was modified.')), 'Merged #7.', commit);
    await flush();
    expect(asks[2].refused?.why).toBe('Couldn’t merge #7: Base branch was modified.');
    expect(said).toEqual(['error: Couldn’t merge #7: Base branch was modified. “Fix login” isn’t done.']);

    // A window that went on from it has it come round again, and says why itself where the
    // dialog isn't in front of you.
    said.length = 0;
    const failed: (string | null)[] = [];
    const going: SessionCan = { ...can, failed: (why) => failed.push(why) };
    for (const front of [true, false]) {
      inFront = front;
      sessionActions(lane(), session(), going).mergeDone?.run();
      asks[asks.length - 1].then(Promise.reject(new Error('Couldn’t merge #7: Head branch was modified.')), 'Merged #7.', commit);
      await flush();
    }
    expect(failed).toEqual([null, 'Couldn’t merge #7: Head branch was modified. “Fix login” isn’t done.']);
    expect(said).toEqual([]);
  });
});

describe('doneTitle', () => {
  const own = fx.session({ live: true, pid: 42, ownWorktree: '/r/app/.claude/worktrees/brave-otter' });

  it('says a session holding its own worktree exits its harness’s way, which cleans that up', () => {
    expect(doneTitle(own)).toBe(
      'Mark as done: Claude Code exits it, removing the worktree it made for it (brave-otter), or asking in its terminal whether to keep it if anything there would be lost. It’s marked seen and archived',
    );
  });

  it('says the worktree stays while its terminal can only close', () => {
    expect(doneTitle({ ...own, status: 'working' })).toBe(
      'Mark as done: it closes where Codealong can close it, and is marked seen and archived. While it’s busy or waiting on you, its terminal just closes and the worktree Claude Code made for it (brave-otter) stays',
    );
  });

  it('says what it always did for any other', () => {
    expect(doneTitle(fx.session({ live: true, pid: 42 }))).toBe('Mark as done: it closes where Codealong can close it, and is marked seen and archived');
  });

  it('says a background job’s worktree goes after it unless something there would be lost', () => {
    expect(doneTitle({ ...own, attachCommand: 'claude attach 1df9ebbd' })).toBe(
      'Mark as done: its background job stops, and the worktree Claude Code made for it (brave-otter) goes too unless something there would be lost: uncommitted files, or commits on no remote. It’s marked seen and archived',
    );
  });
});

describe('Clean up worktree', () => {
  const can = { report, open: noop, cleanUp: vi.fn() };
  const done = lane({ cleanup: { ok: true } });

  it('is offered where the window can confirm it, beside Reuse for new work', () => {
    expect(labels(laneMenu(done, can))).toContain('Clean up worktree…');
    const menu = labels(laneMenu(done, can));
    expect(menu.slice(menu.indexOf('Reuse for new work') - 1, menu.indexOf('Reuse for new work') + 3)).toEqual(['-', 'Reuse for new work', 'Clean up worktree…', '-']);
    laneActions(done, can).cleanUp?.run();
    expect(can.cleanUp).toHaveBeenCalledWith([done]);
    // Not where there's no dialog for it, nor before its git is read.
    expect(laneActions(done, { report, open: noop }).cleanUp).toBeUndefined();
    expect(laneActions(lane(), can).cleanUp).toBeUndefined();
  });

  it('says why one stays, dimmed', () => {
    const kept = laneActions(lane({ cleanup: { ok: false, why: '3 uncommitted files' } }), can).cleanUp;
    expect(kept).toMatchObject({ label: 'Clean up worktree (3 uncommitted files)', enabled: false });
  });

  it('takes only the ones that can go when several are picked', () => {
    const pick = vi.fn();
    const other = lane({ id: '/r/api-two', path: '/r/api-two', cleanup: { ok: true } });
    const main = lane({ id: '/r/api-main', path: '/r/api-main', cleanup: { ok: false, why: 'the main checkout' } });
    const item = lanesActions([done, main, other], { report, cleanUp: pick }).cleanUp;
    expect(item?.label).toBe('Clean up 2 worktrees…');
    item?.run();
    expect(pick).toHaveBeenCalledWith([done, other]);
    expect(lanesActions([main], { report, cleanUp: pick }).cleanUp).toMatchObject({ label: 'Clean up worktree (the main checkout)', enabled: false });
    expect(lanesActions([main, main], { report, cleanUp: pick }).cleanUp).toMatchObject({ label: 'Clean up worktrees (none can go)', enabled: false });
    expect(labels(lanesMenu([done, other], { report, cleanUp: pick }))).toContain('Clean up 2 worktrees…');
  });
});

describe('repoMenu', () => {
  it('opens the repo on GitHub, pins it and copies its name, with no keys: its heading isn’t a row the keyboard selects', () => {
    const pin = { label: 'Pin repo', run: noop };
    const menu = repoMenu({ name: 'web', slug: 'acme/web' }, { report, pin });
    expect(labels(menu)).toEqual(['Open on GitHub', '-', 'Pin repo', '-', 'Copy repo name']);
    expect(menu.some((i) => i !== 'separator' && 'accelerator' in i && i.accelerator)).toBe(false);
    // A repo with no remote on GitHub has no page there.
    expect(labels(repoMenu({ name: 'scratch', slug: null }, { report, pin }))).toEqual(['Pin repo', '-', 'Copy repo name']);
  });
});

describe('fileMenu', () => {
  const can = { report, editor: { id: 'cursor', name: 'Cursor' }, open: noop, edit: noop, reveal: noop };

  it('opens a file in its app or your editor, shows it and copies its path', () => {
    expect(labels(fileMenu('/Users/me/projects/atlas/plan.md', 'file', can))).toEqual(['Open', 'Open in Cursor', 'Reveal in Finder', '-', 'Copy path']);
  });

  it('has no Open for a folder, which opening only shows in the Finder', () => {
    expect(labels(fileMenu('/Users/me/projects/atlas', 'folder', { ...can, editor: { id: 'zed', name: 'Zed' } }))).toEqual(['Open in Zed', 'Reveal in Finder', '-', 'Copy path']);
  });

  it('runs what it was given for each', () => {
    const run = { open: vi.fn(), edit: vi.fn(), reveal: vi.fn() };
    const menu = fileMenu('/Users/me/a.md', 'file', { ...can, ...run }).filter((i): i is MenuAction => i !== 'separator' && !('items' in i));
    menu.slice(0, 3).forEach((i) => i.run());
    expect([run.open, run.edit, run.reveal].map((f) => f.mock.calls.length)).toEqual([1, 1, 1]);
  });
});

describe('linkMenu', () => {
  const URL = 'https://claude.ai/artifact/Pq7';
  const personal = { id: 'Personal', name: 'Personal', dir: 'Default', email: 'me@example.com' };
  const work = { id: 'Work', name: 'Work', dir: 'Profile 3', email: 'me@work.example' };

  it('opens it as a click does, in each of Chrome’s profiles, and copies it', () => {
    expect(labels(linkMenu(URL, [personal, work], { report }))).toEqual(['Open link', '-', 'Open in Chrome (Personal)', 'Open in Chrome (Work)', '-', 'Copy link']);
  });

  it('names one profile alone by nothing, and leaves Chrome out where it has none', () => {
    expect(labels(linkMenu(URL, [work], { report }))).toEqual(['Open link', '-', 'Open in Chrome', '-', 'Copy link']);
    expect(labels(linkMenu(URL, [], { report }))).toEqual(['Open link', '-', 'Copy link']);
  });

  it('tells profiles of one name apart by who’s signed in, or their folders', () => {
    const me = [{ ...personal, name: 'Me' }, { ...work, name: 'me', email: null }];
    expect(labels(linkMenu(URL, me, { report }))).toEqual(['Open link', '-', 'Open in Chrome (Me · me@example.com)', 'Open in Chrome (me · Profile 3)', '-', 'Copy link']);
  });

  it('opens the link where each says, and says what went wrong', async () => {
    const errors: string[] = [];
    vi.mocked(openInChrome).mockClear();
    vi.mocked(openInChrome).mockImplementationOnce(() => Promise.reject(new Error('Chrome didn’t answer.')));
    const menu = linkMenu(URL, [personal, work], { report: { error: (m) => errors.push(m), info: noop } }).filter((i): i is MenuAction => i !== 'separator' && !('items' in i));
    menu[0].run();
    menu[2].run();
    await Promise.resolve();
    await Promise.resolve();
    expect(vi.mocked(openUrl).mock.calls.at(-1)).toEqual([URL]);
    expect(vi.mocked(openInChrome).mock.calls).toEqual([[URL, work]]);
    expect(errors).toEqual(['Chrome didn’t answer.']);
  });
});

describe('laneMenu', () => {
  it('opens, shows, starts, pins and copies', () => {
    expect(labels(laneMenu(lane(), { report, open: noop, reveal: noop, desktop: new Set(['worktree:/r/api']) }))).toEqual([
      'Open in Ghostty',
      'Reveal in Finder',
      'Open PR #7 on GitHub',
      'Open in main window',
      'New session here…',
      '-',
      'Reuse for new work',
      '-',
      'Remove from desktop',
      '-',
      'Copy path',
      'Copy branch name',
      'Copy PR link',
    ]);
  });

  it('checks Reuse for new work while the worktree is reusable, and has it only where it can be', () => {
    const reuse = (l: Lane) => laneActions(l, { report, open: noop }).reuse;
    expect(reuse(lane())?.checked).toBe(false);
    expect(reuse(lane({ reuse: 'free' }))?.checked).toBe(true);
    // Another device's worktree is marked there; a plain folder or one outside your projects never is.
    expect(reuse(lane({ device: { name: 'Mini', short: 'mini', color: 'blue', online: true, lastSeen: null } }))).toBeUndefined();
    expect(reuse(lane({ project: 'Elsewhere' }))).toBeUndefined();
    expect(reuse(lane({ origin: 'folder' }))).toBeUndefined();
  });
});

describe('prMenu and todoMenu', () => {
  it('lead with opening, and copy what there is', () => {
    const s = { lane: lane(), session: session(), open: noop };
    expect(labels(prMenu({ url: 'https://github.com/acme/api/pull/7', number: 7, head: 'fix-login' }, { report, session: s, reveal: noop }))).toEqual([
      'Open on GitHub',
      'Open its session',
      'Open in main window',
      '-',
      'Copy link',
      'Copy branch name',
    ]);
    expect(labels(todoMenu(fx.todo(), { report, start: noop, session: s, desktop: new Set() }))).toEqual(['Start session…', 'Open claude: Fix login', '-', 'Pin to desktop', '-', 'Copy as prompt', 'Copy todo ID']);
  });

  it('offers the nudge for an open PR the board lists, checked while it is on', () => {
    const pr = { url: 'https://github.com/acme/api/pull/7', number: 7, key: 'acme/api#7' };
    const item = (over: object) => prMenu({ ...pr, ...over }, { report }).find((i) => i !== 'separator' && i.label === 'Nudge its session on updates');
    expect(labels(prMenu({ ...pr, state: 'open' }, { report }))).toEqual(['Open on GitHub', '-', 'Nudge its session on updates', '-', 'Copy link']);
    expect(item({ state: 'open' })).toMatchObject({ checked: false });
    expect(item({ state: 'approved', nudge: true })).toMatchObject({ checked: true });
    // Not once it's merged, nor for a PR the board doesn't list (no key).
    expect(item({ state: 'merged', nudge: true })).toBeUndefined();
    expect(item({ state: 'open', key: undefined })).toBeUndefined();
  });

  it('offers Primary session where an open PR has more than one session here, its primary checked', () => {
    const routing = fx.session({ key: 'claude:claude:a', title: 'Routing', account: 'claude' });
    const review = fx.session({ key: 'claude:claude2:b', title: 'Review', account: 'claude2' });
    const lanes = [fx.lane({ id: '/r/a', sessions: [routing] }), fx.lane({ id: '/r/b', sessions: [review] })];
    const sessions = [
      { key: routing.key, lane: '/r/a' },
      { key: review.key, lane: '/r/b' },
    ];
    const pr = { url: 'https://github.com/acme/api/pull/7', number: 7, key: 'acme/api#7', state: 'open' as const, primary: review.key, sessions };
    const group = (over: object, here: Lane[] | null = lanes) => prMenu({ ...pr, ...over }, { report, lanes: here ?? undefined }).find((i): i is MenuGroup => i !== 'separator' && 'items' in i && i.label === 'Primary session');
    const items = group({})?.items as MenuAction[];
    expect(items.map((i) => [i.label, i.checked])).toEqual([
      ['Routing · claude', false],
      ['Review · claude2', true],
    ]);
    items[0].run();
    expect(setPrPrimary).toHaveBeenCalledWith('acme/api#7', routing.key);
    // Not with one session here, once it's merged, nor where the window has no lanes to name them.
    expect(group({ sessions: sessions.slice(0, 1) })).toBeUndefined();
    // An archived one is listed on the PR but can't be its primary, so it isn't offered.
    expect(group({}, [lanes[0], fx.lane({ id: '/r/b', sessions: [{ ...review, archived: true }] })])).toBeUndefined();
    expect(group({ state: 'merged' })).toBeUndefined();
    expect(group({}, null)).toBeUndefined();
  });

  it('puts Snooze review beside Approve, where the review waits on you', () => {
    const snooze = { label: 'Snooze review', items: [] };
    expect(labels(prMenu({ url: 'https://github.com/acme/api/pull/7', number: 7 }, { report, approve: noop, snooze }))).toEqual([
      'Open on GitHub',
      '-',
      'Approve #7',
      'Snooze review ›',
      '-',
      'Copy link',
    ]);
  });
});

describe('the todo menus', () => {
  // A Friday, so the days offered are Today, Tomorrow, Monday and Next Friday.
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 9, 2, 12));
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  /** An item by its path of labels, through submenus. */
  const find = (spec: MenuSpec, ...path: string[]): MenuSpec[number] | undefined => {
    const [first, ...rest] = path;
    const hit = spec.find((i) => i !== 'separator' && i.label === first);
    if (!hit || rest.length === 0) return hit;
    return hit !== 'separator' && 'items' in hit ? find(hit.items, ...rest) : undefined;
  };
  const pick = (spec: MenuSpec, ...path: string[]) => {
    const hit = find(spec, ...path);
    if (!hit || hit === 'separator' || 'items' in hit) throw new Error(`no item ${path.join(' › ')}`);
    hit.run();
  };
  const checked = (spec: MenuSpec) => spec.flatMap((i) => (i !== 'separator' && !('items' in i) && i.checked ? [i.label] : []));
  const submenu = (spec: MenuSpec, label: string) => {
    const hit = find(spec, label);
    return hit && hit !== 'separator' && 'items' in hit ? hit.items : [];
  };

  const bug: Label = { id: 'l1', name: 'Bug', color: 'red' };
  const ui: Label = { id: 'l2', name: 'UI', color: 'blue' };
  const editing = (todos: Todo[], over: Partial<TodoEdit> = {}) => {
    const ops: TodoOp[] = [];
    const edit: TodoEdit = { run: (op) => ops.push(op), todos, labels: [bug, ui], projects: ['Api', 'Web'], ...over };
    return { ops, edit };
  };

  it("has a todo's status, waiting, review, due date, labels and project where the window has them all", () => {
    const t = fx.todo({ status: 'in-progress', labels: ['l1'], project: 'Api' });
    const { ops, edit } = editing([t], { pickDate: noop });
    const menu = todoMenu(t, { report, start: noop, edit, desktop: new Set(), remove: noop });
    expect(labels(menu)).toEqual([
      'Start session…',
      '-',
      'Todo',
      'In progress',
      'Done',
      'Mark as waiting',
      'Ready for review',
      '-',
      'Due date ›',
      'Labels ›',
      'Move to project ›',
      '-',
      'Pin to desktop',
      '-',
      'Copy as prompt',
      'Copy todo ID',
      '-',
      'Archive todo',
      'Delete todo',
    ]);
    expect(checked(menu)).toEqual(['In progress']);
    expect(labels(submenu(menu, 'Due date'))).toEqual(['Today', 'Tomorrow', 'Monday', 'Next Friday', 'Pick a date…']);
    expect(checked(submenu(menu, 'Labels'))).toEqual(['Bug']);
    expect(checked(submenu(menu, 'Move to project'))).toEqual(['Api']);
    pick(menu, 'Done');
    pick(menu, 'Due date', 'Tomorrow');
    pick(menu, 'Labels', 'Bug');
    pick(menu, 'Labels', 'UI');
    pick(menu, 'Move to project', 'Web');
    pick(menu, 'Mark as waiting');
    pick(menu, 'Ready for review');
    expect(ops).toEqual([
      { op: 'update', id: 't1', patch: { status: 'done' } },
      { op: 'update', id: 't1', patch: { due: '2026-10-03' } },
      { op: 'update', id: 't1', patch: { labels: [] } },
      { op: 'update', id: 't1', patch: { labels: ['l1', 'l2'] } },
      { op: 'update', id: 't1', patch: { project: 'Web' } },
      { op: 'update', id: 't1', patch: { waiting: { note: '', since: 0 } } },
      { op: 'update', id: 't1', patch: { review: { note: '', since: 0 } } },
    ]);
    // Marked, it goes back.
    const back = todoMenu({ ...t, review: { note: '', since: 1 } }, { report, start: noop, edit });
    pick(back, 'Back to in progress');
    expect(ops[ops.length - 1]).toEqual({ op: 'update', id: 't1', patch: { review: null } });
  });

  it('leaves out what a window can’t change: a desktop card has its status and due date', () => {
    const t = fx.todo({ due: '2026-10-02' });
    const ops: TodoOp[] = [];
    const menu = todoMenu(t, { report, start: noop, reveal: noop, edit: { run: (op) => ops.push(op) }, desktop: new Set() });
    expect(labels(menu)).toEqual(['Start session…', 'Open in main window', '-', 'Todo', 'In progress', 'Done', 'Ready for review', '-', 'Due date ›', '-', 'Pin to desktop', '-', 'Copy as prompt', 'Copy todo ID', '-', 'Archive todo']);
    // No picker here, and the day it's due is checked.
    expect(labels(submenu(menu, 'Due date'))).toEqual(['Today', 'Tomorrow', 'Monday', 'Next Friday', '-', 'Remove due date']);
    expect(checked(submenu(menu, 'Due date'))).toEqual(['Today']);
    pick(menu, 'Due date', 'Remove due date');
    pick(menu, 'Archive todo');
    expect(ops).toEqual([
      { op: 'update', id: 't1', patch: { due: null } },
      { op: 'update', id: 't1', patch: { archived: true } },
    ]);
  });

  it('archives todos, and unarchives them once they all are', () => {
    const a = fx.todo({ id: 'a' });
    const b = fx.todo({ id: 'b', archivedAt: fx.NOW });
    const { ops, edit } = editing([a, b]);
    // Several: the ones not archived yet.
    pick(todosMenu([a, b], { report, start: noop, edit }), 'Archive 1 todo');
    expect(ops).toEqual([{ op: 'update', id: 'a', patch: { archived: true } }]);
    const both = { ...a, archivedAt: fx.NOW };
    pick(todosMenu([both, b], { report, start: noop, edit }), 'Unarchive 2 todos');
    expect(ops.slice(1)).toEqual([
      { op: 'update', id: 'a', patch: { archived: false } },
      { op: 'update', id: 'b', patch: { archived: false } },
    ]);
    // The main window's does it with an undo of its own.
    const asked: boolean[] = [];
    pick(todoMenu(b, { report, start: noop, edit, archive: (on) => asked.push(on) }), 'Unarchive todo');
    pick(todoPanelMenu(a, { report, edit, archive: (on) => asked.push(on) }), 'Archive todo');
    expect(asked).toEqual([false, true]);
    expect(ops).toHaveLength(3);
  });

  it('acts on all of several todos, and checks what they all are', () => {
    const a = fx.todo({ id: 'a', status: 'done', due: '2026-10-05' });
    const b = fx.todo({ id: 'b', status: 'done', due: '2026-10-05' });
    const { ops, edit } = editing([a, b], { pickDate: noop });
    const boards = [{ label: 'Add 2 todos to board', items: [{ label: 'Release', run: noop }] }];
    const menu = todosMenu([a, b], { report, start: noop, edit, boards, remove: noop });
    // Done ones can't wait.
    expect(labels(menu)).toEqual(['Start one session for 2 todos…', '-', 'Todo', 'In progress', 'Done', 'Due date ›', '-', 'Add 2 todos to board ›', '-', 'Copy as prompt', 'Copy todo IDs', '-', 'Archive 2 todos', 'Delete 2 todos']);
    expect(checked(menu)).toEqual(['Done']);
    expect(labels(submenu(menu, 'Due date'))).toEqual(['Today', 'Tomorrow', 'Monday', 'Next Friday', '-', 'Remove due dates']);
    expect(checked(submenu(menu, 'Due date'))).toEqual(['Monday']);
    pick(menu, 'Todo');
    expect(ops).toEqual([
      { op: 'update', id: 'a', patch: { status: 'todo' } },
      { op: 'update', id: 'b', patch: { status: 'todo' } },
    ]);
  });

  it("has what the todo panel has no control for in its ⋯ menu", () => {
    const t = fx.todo();
    expect(labels(todoPanelMenu(t, { report, edit: editing([t]).edit, remove: noop }))).toEqual(['Move to project ›', 'Copy as prompt', 'Copy todo ID', '-', 'Archive todo', 'Delete todo']);
    expect(labels(todoPanelMenu(t, { report }))).toEqual(['Copy as prompt', 'Copy todo ID']);
  });

  it('says what stopping a wait changed, with the undo where the window has one', () => {
    const t = fx.todo({ title: 'Docs', waiting: { note: '', since: 0 } });
    const said: string[] = [];
    const { ops, edit } = editing([t]);
    toggleWaiting([t], edit, { error: noop, info: (m) => said.push(m) });
    expect(said).toEqual(['“Docs” no longer waits.']);
    expect(ops).toEqual([{ op: 'update', id: 't1', patch: { waiting: null } }]);
    let undo: TodoOp[] = [];
    toggleWaiting([t], { ...edit, stopped: (_said, u) => (undo = u) }, report);
    expect(undo).toEqual([{ op: 'update', id: 't1', patch: { waiting: { note: '', since: 0 }, dependsOn: [] } }]);
    // Without every todo it can't tell what a todo waits on, so it does nothing.
    toggleWaiting([t], { run: (op) => ops.push(op) }, report);
    expect(ops).toHaveLength(2);
  });
});

describe('actions by name', () => {
  it('shows the main window’s keys beside what they run, and runs the same action for them', () => {
    const can: SessionCan = { ...base, keys: true, rename: noop, pin: { label: 'Pin session', run: noop } };
    const keys = sessionMenu(lane(), session(), can).flatMap((i) => (i !== 'separator' && !('items' in i) && i.accelerator ? [`${i.label}=${i.accelerator}`] : []));
    expect(keys).toEqual(['Rename…=R', 'Mark as seen=E', 'Pin session=P', 'Archive=A']);
    // Sessions takes chords.
    const chords = sessionMenu(lane(), session(), { ...can, keys: 'chords' }).flatMap((i) => (i !== 'separator' && !('items' in i) && i.accelerator ? [i.accelerator] : []));
    expect(chords).toEqual(['Ctrl+Cmd+R', 'Cmd+E', 'Ctrl+Cmd+P', 'Ctrl+Cmd+0', 'Ctrl+Cmd+W', 'Shift+Cmd+E', 'Ctrl+Cmd+A']);
    // Elsewhere no window takes those keys, so none show.
    expect(sessionMenu(lane(), session(), { ...can, keys: false }).some((i) => i !== 'separator' && 'accelerator' in i && i.accelerator)).toBe(false);
    const acts = sessionActions(lane(), session(), can);
    expect(keyed(acts, 'a')?.action).toBe(acts.archive);
    expect(keyed(acts, 'x')).toBeNull();
    // A key for an action that doesn't apply does nothing.
    expect(keyed(sessionActions(lane(), session(), base), 'r')).toBeNull();
  });

  it('runs the same actions for their chords, where a box takes the letters', () => {
    const can: SessionCan = { ...base, keys: 'chords', rename: noop, pin: { label: 'Pin session', run: noop } };
    const acts = sessionActions(lane(), session(), can);
    const press = (key: string, mods: { ctrl?: boolean; alt?: boolean; shift?: boolean; cmd?: boolean } = {}) => chorded(acts, { key, code: '', metaKey: mods.cmd ?? true, ctrlKey: !!mods.ctrl, altKey: !!mods.alt, shiftKey: !!mods.shift });
    expect(press('e')).toBe(acts.seen);
    expect(press('a', { ctrl: true })).toBe(acts.archive);
    expect(press('r', { ctrl: true })).toBe(acts.rename);
    expect(press('p', { ctrl: true })).toBe(acts.pin);
    // The letter alone is typed, ⌘A selects all, and other chords are someone else's.
    expect(press('a', { cmd: false })).toBeNull();
    expect(press('a')).toBeNull();
    expect(press('e', { ctrl: true })).toBeNull();
    // ⇧⌘E, beside ⌘E, marks it done.
    expect(press('E', { shift: true })).toBe(acts.done);
    expect(press('e', { shift: true, ctrl: true })).toBeNull();
    expect(press('k', { ctrl: true })).toBeNull();
    // A chord for an action that doesn't apply does nothing.
    expect(chorded(sessionActions(lane(), session(), base), { key: 'r', code: '', metaKey: true, ctrlKey: true, altKey: false, shiftKey: false })).toBeNull();
  });

  it('snoozes on ⌃⌘1 to ⌃⌘7, one for each of the menu’s times, and shows them there', () => {
    const left: string[] = [];
    const can: SessionCan = { ...base, keys: 'chords', left: (done) => left.push(done) };
    const acts = sessionActions(lane(), session(), can);
    const times = (acts.snooze?.items ?? []).flatMap((i) => (i !== 'separator' && !('items' in i) ? [i] : []));
    // The times say their chords (5 PM's only while it's offered), and Pick a time… none.
    const chords = times.map((i) => i.accelerator);
    expect(chords.slice(0, 4)).toEqual(['Ctrl+Cmd+1', 'Ctrl+Cmd+2', 'Ctrl+Cmd+3', 'Ctrl+Cmd+4']);
    expect(chords.slice(-3)).toEqual(['Ctrl+Cmd+6', 'Ctrl+Cmd+7', undefined]);
    const press = (key: string, ctrl = true) => chorded(acts, { key, code: '', metaKey: true, ctrlKey: ctrl, altKey: false, shiftKey: false });
    expect(press('1')).toBe(times[0]);
    expect(press('7')).toBe(times[times.length - 2]);
    // ⌘1 opens things, and ⌃⌘8 is the time its reply suggests, which this one has none of.
    expect(press('1', false)).toBeNull();
    expect(press('8')).toBeNull();
    setPlatform('linux');
    try {
      expect(chorded(acts, { key: '3', code: 'Digit3', metaKey: false, ctrlKey: true, altKey: true, shiftKey: false })).toBe(times[2]);
    } finally {
      setPlatform(null);
    }
    expect(keyed(acts, '2', true)).toEqual({ id: 'snooze', action: times[1] });
    expect(keyed(acts, '2')).toBeNull();
    expect(chordLetter('3')).toBe(true);
    expect(chordLetter('8')).toBe(true);
    expect(chordLetter('9')).toBe(false);
    press('2')?.run();
    expect(left).toEqual(['snoozed']);
    // Where no window takes the chords, the menu doesn't show them; one that can't be snoozed has none.
    const plain = sessionActions(lane(), session(), base).snooze?.items ?? [];
    expect(plain.some((i) => i !== 'separator' && 'accelerator' in i && i.accelerator)).toBe(false);
    expect(chorded(sessionActions(lane(), { ...session(), archived: true }, can), { key: '1', code: '', metaKey: true, ctrlKey: true, altKey: false, shiftKey: false })).toBeNull();
  });

  it('offers a snooze until the time its reply says the work goes on, first in the menu and on ⌃⌘8', () => {
    const left: string[] = [];
    const can: SessionCan = { ...base, keys: 'chords', left: (done) => left.push(done) };
    const until = Date.now() + 3 * 24 * 3_600_000;
    const waits = { ...session(), insight: { waitsUntil: until } };
    const press = (acts: ReturnType<typeof sessionActions>) => chorded(acts, { key: '8', code: '', metaKey: true, ctrlKey: true, altKey: false, shiftKey: false });
    const acts = sessionActions(lane(), waits, can);
    const [first, rule] = acts.snooze?.items ?? [];
    expect(first !== 'separator' && first.label).toMatch(/^Until .+, as Jev read its reply$/);
    expect(first !== 'separator' && !('items' in first) && first.accelerator).toBe('Ctrl+Cmd+8');
    expect(rule).toBe('separator');
    expect(press(acts)).toBe(first);
    expect(keyed(acts, '8', true)).toEqual({ id: 'snooze', action: first });
    // It leaves what waits on you as any snooze does.
    press(acts)?.run();
    expect(left).toEqual(['snoozed']);
    // Snoozed already, or archived, nothing is suggested.
    expect(press(sessionActions(lane(), { ...waits, snoozedUntil: until - 1000 }, can))).toBeNull();
    expect(press(sessionActions(lane(), { ...waits, archived: true }, can))).toBeNull();
  });

  it('offers a snooze until its usage limit lifts, first in the menu and on ⌃⌘8', () => {
    const left: string[] = [];
    const can: SessionCan = { ...base, keys: 'chords', left: (done) => left.push(done) };
    const lifts = Date.now() + 5 * 3_600_000;
    const stopped = { ...session(), status: 'error' as const, limit: { meter: 'week', resetsAt: lifts, at: Date.now() - 60_000, moveTo: null, continues: true } };
    const acts = sessionActions(lane(), stopped, can);
    const [first] = acts.snooze?.items ?? [];
    expect(first !== 'separator' && first.label).toMatch(/^Until .+, when its limit lifts$/);
    const pressed = chorded(acts, { key: '8', code: '', metaKey: true, ctrlKey: true, altKey: false, shiftKey: false });
    expect(pressed).toBe(first);
    pressed?.run();
    expect(left).toEqual(['snoozed']);
  });

  it('closes a session’s terminal on ⌃⌘W, which no letter alone does, and says it left', () => {
    const left: string[] = [];
    const can: SessionCan = { ...base, keys: 'chords', left: (done) => left.push(done) };
    const acts = sessionActions(lane(), session(), can);
    expect(acts.close?.label).toBe('Close terminal');
    expect(chorded(acts, { key: 'w', code: '', metaKey: true, ctrlKey: true, altKey: false, shiftKey: false })).toBe(acts.close);
    // ⌘W is the window's, and the letter alone is nothing's.
    expect(chorded(acts, { key: 'w', code: '', metaKey: true, ctrlKey: false, altKey: false, shiftKey: false })).toBeNull();
    // On Linux ⌃⌘ is Ctrl+Alt (platform.ts).
    setPlatform('linux');
    try {
      expect(chordSigns(CHORDS.close ?? '')).toBe('Ctrl+Alt+W');
      expect(chorded(acts, { key: 'w', code: '', metaKey: false, ctrlKey: true, altKey: true, shiftKey: false })).toBe(acts.close);
      expect(chorded(acts, { key: 'w', code: '', metaKey: false, ctrlKey: true, altKey: false, shiftKey: false })).toBeNull();
    } finally {
      setPlatform(null);
    }
    expect(keyed(acts, 'w')).toBeNull();
    expect(keyed(acts, 'w', true)?.action).toBe(acts.close);
    expect(keyed(acts, 'a', true)?.action).toBe(acts.archive);
    expect(chordLetter('w')).toBe(true);
    expect(chordLetter('k')).toBe(false);
    expect(chordSigns('Ctrl+Cmd+W')).toBe('⌃⌘W');
    expect(chordSigns('Cmd+E')).toBe('⌘E');
    // The keys for getting from one session to the next (queue.ts) are said the same way.
    expect(chordSigns('Alt+Shift+Down')).toBe('⌥⇧↓');
    expect(chordSigns('Ctrl+Shift+-')).toBe('⌃⇧-');
    expect(chordSigns('Esc')).toBe('esc');
    acts.close?.run();
    expect(left).toEqual(['closed']);
    // One with no terminal of its own has nothing to close.
    expect(chorded(sessionActions(lane(), { ...session(), live: false }, can), { key: 'w', code: '', metaKey: true, ctrlKey: true, altKey: false, shiftKey: false })).toBeNull();
  });

  it('asks first what becomes of helpers whose work isn’t over, and ends them only when told', async () => {
    const ask = vi.mocked(doneAhead);
    ask.mockReset();
    ask.mockResolvedValue({ done: true, message: 'Done.', at: 5 });
    const settled = () => new Promise((r) => setTimeout(r, 0));
    const boss = session();
    const helper = (key: string, over: Partial<Session>) => session({ key, id: key, title: key, helper: { parent: boss.key }, ...over });
    const finished = helper('claude:c:f', { status: 'idle' });
    const sessions = [
      boss,
      helper('claude:c:w', { status: 'working' }),
      helper('claude:c:n', { status: 'needs', live: false }),
      helper('claude:c:bg', { status: 'idle', background: 'npm test' }),
      finished,
      helper('claude:c:a', { status: 'working', archived: true }),
      helper('claude:c:o', { status: 'working', helper: { parent: 'claude:c:other' } }),
    ];
    const lanes = [lane({ sessions })];
    // At work, waiting on you, or with a command left running; not one done, archived or another's.
    expect(unfinishedHelpers(lanes, [boss.key]).map((s) => s.key)).toEqual(['claude:c:w', 'claude:c:n', 'claude:c:bg']);
    // Nor one picked to be marked done with it.
    expect(unfinishedHelpers(lanes, [boss.key, 'claude:c:w']).map((s) => s.key)).toEqual(['claude:c:n', 'claude:c:bg']);
    // Those the helpers started are asked about too, at any depth: under one whose work is over,
    // which ends with it, and under one whose work isn't, which ending would stop.
    const nested = [...sessions, helper('claude:c:review', { status: 'working', helper: { parent: 'claude:c:f' } }), helper('claude:c:inner', { status: 'working', helper: { parent: 'claude:c:w' } }), helper('claude:c:check', { status: 'idle', helper: { parent: 'claude:c:w' } })];
    const shown = unfinishedHelpers([lane({ sessions: nested })], [boss.key]);
    expect(shown.map((s) => s.key)).toEqual(['claude:c:w', 'claude:c:n', 'claude:c:bg', 'claude:c:inner', 'claude:c:review']);
    // Ended, everything under them goes, done or not; kept, as shown.
    expect(helpersChosen([lane({ sessions: nested })], shown, true).end).toEqual(['claude:c:w', 'claude:c:n', 'claude:c:bg', 'claude:c:inner', 'claude:c:review', 'claude:c:check']);
    expect(helpersChosen([lane({ sessions: nested })], shown, false)).toEqual({ end: [], keep: ['claude:c:w', 'claude:c:n', 'claude:c:bg', 'claude:c:inner', 'claude:c:review'] });
    const asked: DoneHelpersAsk[] = [];
    const doneHelpers = { lanes, ask: (a: DoneHelpersAsk) => void asked.push(a) };
    const left: string[] = [];
    sessionActions(lane(), boss, { ...base, doneHelpers, left: (d) => left.push(d) }).done?.run();
    await settled();
    // Nothing goes until it's answered.
    expect(ask).not.toHaveBeenCalled();
    expect(left).toEqual([]);
    expect(asked.map((a) => [a.sessions.map((s) => s.key), a.helpers.map((h) => h.key)])).toEqual([[[boss.key], ['claude:c:w', 'claude:c:n', 'claude:c:bg']]]);
    asked[0].then(true);
    await settled();
    expect(ask).toHaveBeenLastCalledWith(boss.key, false, undefined, { end: ['claude:c:w', 'claude:c:n', 'claude:c:bg'], keep: [] });
    expect(left).toEqual(['done']);
    // With none whose work isn't over, or no dialog to ask in, it goes at once, and they're kept.
    ask.mockClear();
    sessionActions(lane(), boss, { ...base, doneHelpers: { lanes: [lane({ sessions: [boss, finished] })], ask: doneHelpers.ask } }).done?.run();
    sessionActions(lane(), boss, base).done?.run();
    await settled();
    expect(ask.mock.calls).toEqual([
      [boss.key, false, undefined, NONE_CHOSEN],
      [boss.key, false, undefined, NONE_CHOSEN],
    ]);
    expect(asked).toHaveLength(1);
    // Picked together, they're asked about once, and the answer goes for each.
    ask.mockClear();
    const other = session({ key: 'claude:c:9', id: 'e9', title: 'Other' });
    sessionsActions([{ lane: lane(), session: boss }, { lane: lane(), session: other }], { report, doneHelpers }).done?.run();
    expect(asked[1].sessions.map((s) => s.key)).toEqual([boss.key, other.key]);
    asked[1].then(false);
    await settled();
    // Kept as you chose, though one finished meanwhile.
    const keep = { end: [], keep: ['claude:c:w', 'claude:c:n', 'claude:c:bg'] };
    expect(ask.mock.calls.map((c) => [c[0], c[3]])).toEqual([
      [boss.key, keep],
      [other.key, keep],
    ]);
  });

  it('marks done on ⇧⌘E, held by work under way until it’s asked again', async () => {
    const ask = vi.mocked(doneAhead);
    ask.mockReset();
    const said: string[] = [];
    const left: string[] = [];
    const undone: string[] = [];
    const can: SessionCan = { ...base, keys: 'chords', report: { error: (m) => said.push(`error: ${m}`), info: (m) => said.push(m) }, left: (d) => left.push(d) };
    const settled = () => new Promise((r) => setTimeout(r, 0));
    const answer = (r: Done) => ask.mockResolvedValueOnce(r);
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(1_000_000);
    try {
      // Done: it goes on to the next, which says it was done, and the window says nothing more
      // unless something didn't go as asked; the main window offers its Undo.
      answer({ done: true, message: 'Done with “Fix login”: its terminal is closed and it’s archived.', at: 5 });
      sessionActions(lane(), session(), can).done?.run();
      await settled();
      expect(ask).toHaveBeenLastCalledWith('claude:c:1', false, undefined, NONE_CHOSEN);
      expect(left).toEqual(['done']);
      expect(said).toEqual([]);
      answer({ done: true, message: 'Done with “Fix login”: it’s archived. It’s still open in the Claude app.', noted: true, at: 5 });
      sessionActions(lane(), session(), can).done?.run();
      await settled();
      expect(said).toEqual(['Done with “Fix login”: it’s archived. It’s still open in the Claude app.']);
      said.length = 0;
      answer({ done: true, message: 'Done with “Fix login”: it’s archived.', at: 6 });
      sessionActions(lane(), session(), { ...can, undoDone: (m) => undone.push(m) }).done?.run();
      await settled();
      expect(undone).toEqual(['Done with “Fix login”: it’s archived.']);

      // Working: held here, asking nothing of the engine; again soon after, it stops.
      said.length = 0;
      left.length = 0;
      ask.mockClear();
      const busy = session({ key: 'claude:c:2', status: 'working' });
      sessionActions(lane(), busy, can).done?.run();
      expect(ask).not.toHaveBeenCalled();
      expect(left).toEqual([]);
      expect(said).toEqual(['“Fix login” is still working. Mark it done again to end that too.']);
      answer({ done: true, message: 'Done with “Fix login”: its terminal is closed and it’s archived.', at: 7 });
      sessionActions(lane(), busy, can).done?.run();
      await settled();
      expect(ask).toHaveBeenLastCalledWith('claude:c:2', true, undefined, NONE_CHOSEN);
      expect(left).toEqual(['done']);

      // Held by the engine (a command left running in the background), then asked too late: held
      // again, and only the next one soon after ends it.
      said.length = 0;
      ask.mockClear();
      const bg = session({ key: 'claude:c:3' });
      answer({ done: false, message: '“Fix login” left `npm run dev` running in the background.' });
      sessionActions(lane(), bg, can).done?.run();
      await settled();
      expect(said).toEqual(['“Fix login” left `npm run dev` running in the background. Mark it done again to end that too.']);
      vi.setSystemTime(1_000_000 + DONE_HOLD_MS + 1);
      answer({ done: false, message: '“Fix login” left `npm run dev` running in the background.' });
      sessionActions(lane(), bg, can).done?.run();
      await settled();
      expect(ask).toHaveBeenLastCalledWith('claude:c:3', false, undefined, NONE_CHOSEN);
      answer({ done: true, message: 'Done with “Fix login”: its terminal is closed and it’s archived.', at: 8 });
      sessionActions(lane(), bg, can).done?.run();
      await settled();
      expect(ask).toHaveBeenLastCalledWith('claude:c:3', true, undefined, NONE_CHOSEN);

      // What the engine turns down is said as an error.
      said.length = 0;
      ask.mockRejectedValueOnce('That session isn’t on the board any more.');
      sessionActions(lane(), session({ key: 'claude:c:4' }), can).done?.run();
      await settled();
      expect(said).toEqual(['error: That session isn’t on the board any more.']);
    } finally {
      vi.useRealTimers();
    }
  });

  it('opens a new terminal in a session’s folder on ⌃`, and shows it beside the menu’s terminal', () => {
    const opened: string[] = [];
    const here = lane({ targets: [...lane().targets, fx.target({ id: 'l|editor', label: 'Open in Cursor' })] });
    const editors = { apps: { editor: 'Cursor', editorId: 'cursor', editors: [{ id: 'cursor', name: 'Cursor' }] }, icons: {} };
    const can: SessionCan = { ...base, keys: 'chords', editors, open: (_l, target) => opened.push(target) };
    const acts = sessionActions(here, session(), can);
    const press = (over: Partial<KeyboardEvent> = {}) => chorded(acts, { key: '`', code: 'Backquote', metaKey: false, ctrlKey: true, altKey: false, shiftKey: false, ...over });
    expect(press()).toBe(acts.terminal);
    // A layout where ` is a dead key goes by where the key is.
    expect(press({ key: 'Dead' })).toBe(acts.terminal);
    // ` alone is typed, and ⌘` goes through the app's windows.
    expect(press({ ctrlKey: false })).toBeNull();
    expect(press({ metaKey: true, ctrlKey: false })).toBeNull();
    acts.terminal?.run();
    expect(opened).toEqual(['s|claude:c:1|terminal']);
    // The menu has it once, as the folder's first, with its key where the window takes it.
    const folder = sessionMenu(here, session(), can).find((i): i is MenuGroup => i !== 'separator' && 'items' in i && i.label === 'Open folder in');
    expect(folder?.items[0]).toMatchObject({ label: 'Ghostty', accelerator: 'Ctrl+`' });
    expect(labels(sessionMenu(here, session(), can)).filter((l) => l.includes('Ghostty'))).toEqual(['Respond in Ghostty']);
    const plain = sessionActions(here, session(), { ...can, keys: undefined }).folder as MenuGroup;
    expect((plain.items[0] as MenuAction).accelerator).toBeUndefined();
    // Another device's folder has no terminal on this Mac.
    const away = lane({ ...here, device: { name: 'Mini', short: 'mini', color: 'blue', online: true, lastSeen: null } });
    expect(sessionActions(away, session(), can).terminal).toBeUndefined();
  });

  it('opens a new terminal in a worktree on ⌃`, next to its default open in the menu', () => {
    const opened: string[] = [];
    const open = (_l: Lane, target: string) => opened.push(target);
    const busy = lane({ defaultTarget: 's|claude:c:1|focus' });
    const acts = laneActions(busy, { report, open });
    acts.terminal?.run();
    expect(opened).toEqual(['l|terminal']);
    const shown = laneMenu(busy, { report, open, keys: true }).flatMap((i) => (i !== 'separator' && !('items' in i) ? [`${i.label}${i.accelerator ? `=${i.accelerator}` : ''}`] : []));
    expect(shown.slice(0, 3)).toEqual(['Respond in Ghostty', 'Open in Ghostty=Ctrl+`', 'Reveal in Finder']);
    // With no session the terminal is the default open, there once, with its key.
    const idle = laneMenu(lane(), { report, open, keys: true }).flatMap((i) => (i !== 'separator' && !('items' in i) ? [`${i.label}${i.accelerator ? `=${i.accelerator}` : ''}`] : []));
    expect(idle.slice(0, 2)).toEqual(['Open in Ghostty=Ctrl+`', 'Reveal in Finder']);
    // Another device's worktree opens its default as before, and has no terminal here.
    const away = lane({ device: { name: 'Mini', short: 'mini', color: 'blue', online: true, lastSeen: null } });
    expect(laneActions(away, { report, open }).terminal).toBeUndefined();
    expect(laneActions(away, { report, open }).open?.label).toBe('Open in Ghostty');
  });
});

describe('suggested replies’ keys', () => {
  const press = (code: string, over: Partial<KeyboardEvent> = {}) => ({ key: code.slice(-1), code, metaKey: true, ctrlKey: false, altKey: false, shiftKey: false, ...over });

  it('sends each by ⌘ and its number, by the key’s place', () => {
    expect(SUGGESTION_CHORDS.map(chordSigns)).toEqual(['⌘1', '⌘2', '⌘3', '⌘4', '⌘5', '⌘6']);
    expect(suggestionChord(press('Digit1'))).toBe(0);
    expect(suggestionChord(press('Digit6'))).toBe(5);
    // A layout that types another character there still has it there.
    expect(suggestionChord(press('Digit2', { key: 'é' }))).toBe(1);
  });

  it('leaves every other key alone', () => {
    expect(suggestionChord(press('Digit7'))).toBeNull();
    expect(suggestionChord(press('Digit1', { metaKey: false }))).toBeNull();
    // A view's ⌃ and digit, a project's ⌥⌘, a snooze's ⌃⌘ and the screenshots' ⇧⌘.
    expect(suggestionChord(press('Digit1', { metaKey: false, ctrlKey: true }))).toBeNull();
    expect(suggestionChord(press('Digit1', { altKey: true }))).toBeNull();
    expect(suggestionChord(press('Digit1', { ctrlKey: true }))).toBeNull();
    expect(suggestionChord(press('Digit3', { shiftKey: true }))).toBeNull();
  });
});
