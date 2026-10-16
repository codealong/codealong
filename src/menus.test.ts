import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { doneAhead, quietAhead } from './ahead';
import { getAccounts, moveSession, moveSessionTo, setPrPrimary, type Done } from './api';
import type { MenuAction, MenuGroup, MenuSpec } from './contextMenu';
import { carryOnIn, chordLetter, chordSigns, chorded, DONE_HOLD_MS, SUGGESTION_CHORDS, suggestionChord, doneTitle, keyed, laneActions, laneMenu, lanesActions, lanesMenu, prMenu, prsActions, prsMenu, sessionActions, sessionMenu, sessionsActions, sessionsMenu, todoMenu, todoPanelMenu, todosMenu, toggleWaiting, type SessionCan, type SessionsCan, type TodoEdit } from './menus';
import { openLabel } from './open';
import type { Report } from './report';
import * as fx from './testing/fixtures';
import type { TodoOp } from './todos';
import type { Label, Lane, PrItem, Session, Todo } from './types';

// What the engine says to Mark as done: done, unless a test says otherwise.
vi.mock('./ahead', async (original) => ({ ...(await original<typeof import('./ahead')>()), doneAhead: vi.fn(), quietAhead: vi.fn(() => Promise.resolve()) }));
// No accounts to move to, unless a test says otherwise; what a move gives back, as a test says.
vi.mock('./api', async (original) => ({ ...(await original<typeof import('./api')>()), getAccounts: vi.fn(() => Promise.resolve([])), moveSession: vi.fn(), moveSessionTo: vi.fn(), setPrPrimary: vi.fn(() => Promise.resolve()) }));

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
  it('goes on where the account’s last session did, else in a terminal, as the box would', () => {
    const terminal = { place: 'terminal' as const, label: 'Terminal' };
    const background = { place: 'background' as const, label: 'Background' };
    const app = { place: 'codex-app' as const, label: 'Codex app' };
    expect(carryOnIn([background, terminal], 'background')).toBe(background);
    expect(carryOnIn([background, terminal], undefined)).toBe(terminal);
    // Where it last went on, if it can't go there now, isn't the place.
    expect(carryOnIn([background, terminal], 'codealong')).toBe(terminal);
    expect(carryOnIn([app], undefined)).toBe(app);
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
    // The main window's: its pins, todos and canvas.
    const main = labels(
      sessionMenu(lane(), session(), {
        ...base,
        pin: { label: 'Pin session', run: noop },
        desktop: new Set(['session:claude:c:1']),
        todos: [{ label: 'Add to todo', items: [] }],
        canvas: [{ label: 'Add to canvas', run: noop }],
      }),
    );
    // Every window's has everything the plainest one does, in the same order.
    const common = everywhere.filter((l) => l !== '-');
    for (const menu of [bar, main]) expect(menu.filter((l) => common.includes(l))).toEqual(common);
    expect(bar.slice(0, 2)).toEqual(['Fix login · api', '-']);
    expect(bar).toEqual(expect.arrayContaining(['Show in Codealong', 'Reply here', 'Pin to desktop']));
    expect(main).toEqual(expect.arrayContaining(['Add to todo ›', 'Pin session', 'Remove from desktop', 'Add to canvas']));
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
    expect(has(session({ key: 'antigravity:a:1', harness: 'antigravity' }), 'Rename…', { rename: noop })).toBe(false);
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
    // The accounts are read again a minute after they last were, as a session's menu is built.
    const readAccounts = async (list: Awaited<ReturnType<typeof getAccounts>>, at: number) => {
      accounts.mockResolvedValueOnce(list);
      vi.setSystemTime(at);
      sessionMenu(lane(), session(), base);
      await settled();
    };
    const c2 = { name: 'c2', harness: 'claude' as const, home: '~/.claude-c2', command: 'claude-c2', folderExists: true, signedIn: true, email: null, signInNote: null };
    try {
      // One turned off isn't offered.
      await readAccounts([{ ...c2, disabled: true }], later);
      expect(sessionActions(lane(), session({ key: 'claude:c:10' }), base).move).toBeUndefined();
      await readAccounts([c2], later + 61_000);
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
      'Snooze 2 sessions ›',
      'Quiet 2 sessions',
      'Close 1 session',
      'Mark 2 sessions as done',
      'Archive 2 sessions',
      '-',
      'Copy session IDs',
    ]);
    expect(sessionsActions([a, b, c], { report }).takes).toEqual({ stop: 0, seen: 1, unread: 1, snooze: 2, close: 1, done: 2, archive: 2 });
    // All archived: they come out of it together, and aren't pinned, snoozed or done.
    const d = { ...c, session: { ...c.session, key: 'claude:c:4', id: 'd4' } };
    expect(labels(sessionsMenu([c, d], { report, pin: { label: 'Pin 2 sessions', run: noop } }))).toEqual(['Unarchive 2 sessions', '-', 'Copy session IDs']);
  });

  it('runs on the chords one session’s actions do, shown beside them', () => {
    const acts = sessionsActions([a, b], { report, keys: 'chords' });
    expect(keyed(acts, 'a', true)?.action).toBe(acts.archive);
    expect(keyed(acts, 'w', true)?.action).toBe(acts.close);
    expect(keyed(acts, '2', true)?.id).toBe('snooze');
    // Rename is one at a time.
    expect(keyed(acts, 'r', true)).toBeNull();
    const chords = sessionsMenu([a, b], { report, keys: 'chords' }).flatMap((i) => (i !== 'separator' && !('items' in i) && i.accelerator ? [i.accelerator] : []));
    expect(chords).toEqual(['Cmd+E', 'Ctrl+Cmd+U', 'Ctrl+Cmd+W', 'Shift+Cmd+E', 'Ctrl+Cmd+A']);
    expect(keyed(acts, 'u', true)?.action).toBe(acts.unread);
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
      expect(ask).toHaveBeenLastCalledWith('claude:c:5', true, undefined);
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

  it('offers no approval where the window can’t give one, nor where none can be given', () => {
    expect(prsActions([asked, other], { report }).approve).toBeUndefined();
    expect(labels(prsMenu([mine, merged], { report, approve: noop }))).toEqual(['Open 2 PRs on GitHub', '-', 'Copy links', 'Copy branch names']);
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

describe('laneMenu', () => {
  it('opens, shows, starts, pins and copies', () => {
    expect(labels(laneMenu(lane(), { report, open: noop, reveal: noop, desktop: new Set(['worktree:/r/api']) }))).toEqual([
      'Open in Ghostty',
      'Reveal in Finder',
      'Open PR #7 on GitHub',
      'Show in Codealong',
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
      'Show in Codealong',
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
    expect(labels(menu)).toEqual(['Start session…', 'Show in Codealong', '-', 'Todo', 'In progress', 'Done', 'Ready for review', '-', 'Due date ›', '-', 'Pin to desktop', '-', 'Copy as prompt', 'Copy todo ID']);
    // No picker here, and the day it's due is checked.
    expect(labels(submenu(menu, 'Due date'))).toEqual(['Today', 'Tomorrow', 'Monday', 'Next Friday', '-', 'Remove due date']);
    expect(checked(submenu(menu, 'Due date'))).toEqual(['Today']);
    pick(menu, 'Due date', 'Remove due date');
    expect(ops).toEqual([{ op: 'update', id: 't1', patch: { due: null } }]);
  });

  it('acts on all of several todos, and checks what they all are', () => {
    const a = fx.todo({ id: 'a', status: 'done', due: '2026-10-05' });
    const b = fx.todo({ id: 'b', status: 'done', due: '2026-10-05' });
    const { ops, edit } = editing([a, b], { pickDate: noop });
    const menu = todosMenu([a, b], { report, start: noop, edit, remove: noop });
    // Done ones can't wait.
    expect(labels(menu)).toEqual(['Start one session for 2 todos…', '-', 'Todo', 'In progress', 'Done', 'Due date ›', '-', 'Copy as prompt', 'Copy todo IDs', '-', 'Delete 2 todos']);
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
    expect(labels(todoPanelMenu(t, { report, edit: editing([t]).edit, remove: noop }))).toEqual(['Move to project ›', 'Copy as prompt', 'Copy todo ID', '-', 'Delete todo']);
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
    expect(chords).toEqual(['Ctrl+Cmd+R', 'Cmd+E', 'Ctrl+Cmd+P', 'Ctrl+Cmd+W', 'Shift+Cmd+E', 'Ctrl+Cmd+A']);
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

  it('snoozes on ⌃⌘1 to ⌃⌘4, one for each of the menu’s times, and shows them there', () => {
    const left: string[] = [];
    const can: SessionCan = { ...base, keys: 'chords', left: (done) => left.push(done) };
    const acts = sessionActions(lane(), session(), can);
    const times = (acts.snooze?.items ?? []).flatMap((i) => (i !== 'separator' && !('items' in i) ? [i] : []));
    expect(times.map((i) => i.accelerator)).toEqual(['Ctrl+Cmd+1', 'Ctrl+Cmd+2', 'Ctrl+Cmd+3', 'Ctrl+Cmd+4']);
    const press = (key: string, ctrl = true) => chorded(acts, { key, code: '', metaKey: true, ctrlKey: ctrl, altKey: false, shiftKey: false });
    expect(press('1')).toBe(times[0]);
    expect(press('4')).toBe(times[3]);
    // ⌘1 opens things, and there's no fifth time.
    expect(press('1', false)).toBeNull();
    expect(press('5')).toBeNull();
    expect(keyed(acts, '2', true)).toEqual({ id: 'snooze', action: times[1] });
    expect(keyed(acts, '2')).toBeNull();
    expect(chordLetter('3')).toBe(true);
    expect(chordLetter('7')).toBe(false);
    press('2')?.run();
    expect(left).toEqual(['snoozed']);
    // Where no window takes the chords, the menu doesn't show them; one that can't be snoozed has none.
    const plain = sessionActions(lane(), session(), base).snooze?.items ?? [];
    expect(plain.some((i) => i !== 'separator' && 'accelerator' in i && i.accelerator)).toBe(false);
    expect(chorded(sessionActions(lane(), { ...session(), archived: true }, can), { key: '1', code: '', metaKey: true, ctrlKey: true, altKey: false, shiftKey: false })).toBeNull();
  });

  it('closes a session’s terminal on ⌃⌘W, which no letter alone does, and says it left', () => {
    const left: string[] = [];
    const can: SessionCan = { ...base, keys: 'chords', left: (done) => left.push(done) };
    const acts = sessionActions(lane(), session(), can);
    expect(acts.close?.label).toBe('Close terminal');
    expect(chorded(acts, { key: 'w', code: '', metaKey: true, ctrlKey: true, altKey: false, shiftKey: false })).toBe(acts.close);
    // ⌘W is the window's, and the letter alone is nothing's.
    expect(chorded(acts, { key: 'w', code: '', metaKey: true, ctrlKey: false, altKey: false, shiftKey: false })).toBeNull();
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
      expect(ask).toHaveBeenLastCalledWith('claude:c:1', false);
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
      expect(ask).toHaveBeenLastCalledWith('claude:c:2', true);
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
      expect(ask).toHaveBeenLastCalledWith('claude:c:3', false);
      answer({ done: true, message: 'Done with “Fix login”: its terminal is closed and it’s archived.', at: 8 });
      sessionActions(lane(), bg, can).done?.run();
      await settled();
      expect(ask).toHaveBeenLastCalledWith('claude:c:3', true);

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
