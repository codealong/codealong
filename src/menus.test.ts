import { describe, expect, it } from 'vitest';
import type { MenuSpec } from './contextMenu';
import { laneMenu, prMenu, sessionMenu, todoMenu, type SessionCan } from './menus';
import { openLabel } from './open';
import type { Report } from './report';
import type { Lane, Session } from './types';

const report: Report = { error: () => {}, info: () => {} };
const noop = () => {};

/** A menu as its labels: a submenu as its label and ›, a line as -. */
function labels(spec: MenuSpec): string[] {
  return spec.map((i) => (i === 'separator' ? '-' : 'items' in i ? `${i.label} ›` : i.label));
}

const session = (over: Partial<Session> = {}): Session =>
  ({
    key: 'claude:c:1',
    id: '4f1c2a9e',
    title: 'Fix login',
    account: 'claude',
    harness: 'claude',
    surface: 'cli',
    status: 'unread',
    live: true,
    pid: 42,
    hosted: false,
    attachCommand: null,
    resumeCommand: 'claude --resume 4f1c2a9e',
    openTarget: 's|claude:c:1|focus',
    archived: false,
    visitor: null,
    snoozedUntil: null,
    ...over,
  }) as Session;

const lane = (over: Partial<Lane> = {}): Lane =>
  ({
    id: '/r/api',
    project: 'Acme',
    callsign: 'api',
    path: '/r/api',
    branch: 'fix-login',
    device: null,
    pr: { number: 7, url: 'https://github.com/acme/api/pull/7' },
    preview: null,
    defaultTarget: 'l|terminal',
    targets: [
      { id: 'l|terminal', label: 'Open in Ghostty', disabled: false },
      { id: 'l|finder', label: 'Reveal in Finder', disabled: false },
      { id: 's|claude:c:1|focus', label: 'Respond in Ghostty', disabled: false },
    ],
    ...over,
  }) as unknown as Lane;

const base: SessionCan = { report, open: noop, terminal: 'Ghostty' };

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
      'Close terminal',
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
    expect(has(session({ attachCommand: 'claude attach 6e07c149', pid: null }), 'End background job')).toBe(true);
    expect(has(session({ live: false }), 'Close terminal')).toBe(false);
    expect(has(session({ surface: 'app' }), 'Close terminal')).toBe(false);
    expect(has(session({ harness: 'codex' }), 'Archive in Codex')).toBe(true);
    expect(has(session({ resumeCommand: null }), 'Fork in terminal')).toBe(false);
    expect(has(session({ status: 'working', stoppable: true }), 'Stop turn')).toBe(true);
    expect(has(session(), 'Stop turn')).toBe(false);
    // Renaming, where the window asks for a name and the harness keeps one.
    expect(has(session(), 'Rename…')).toBe(false);
    expect(has(session(), 'Rename…', { rename: noop })).toBe(true);
    expect(has(session({ key: 'antigravity:a:1', harness: 'antigravity' }), 'Rename…', { rename: noop })).toBe(false);
    // Archived: only unarchiving, no pins or snoozes.
    const archived = session({ archived: true });
    expect(has(archived, 'Unarchive')).toBe(true);
    for (const label of ['Pin session', 'Pin to desktop', 'Snooze ›']) expect(has(archived, label, { pin: { label: 'Pin session', run: noop }, desktop: new Set() })).toBe(false);
    // On another device: no snooze, no closing, and new sessions only where a window starts them there.
    const remote = session({ key: '@ab12:claude:c:1' });
    const remoteLane = lane({ id: '@ab12:/r/api' });
    const there = labels(sessionMenu(remoteLane, remote, base));
    expect(there).not.toContain('Snooze ›');
    expect(there).not.toContain('Close terminal');
    expect(there).not.toContain('New session here…');
    expect(labels(sessionMenu(remoteLane, remote, { ...base, newSession: noop }))).toContain('New session here…');
  });

  it('never starts, ends or doubles a line', () => {
    const spec = sessionMenu(lane({ pr: null }), session({ status: 'idle', resumeCommand: null, live: false }), base);
    const l = labels(spec);
    expect(l[0]).not.toBe('-');
    expect(l[l.length - 1]).not.toBe('-');
    expect(l.join('|')).not.toContain('-|-');
  });

  it('names where it opens when the worktree doesn’t list it', () => {
    expect(openLabel(lane(), session(), 'Ghostty')).toBe('Respond in Ghostty');
    // In the engine's words for how it opens.
    expect(openLabel(lane({ targets: [] }), session(), 'Ghostty')).toBe('Show in Ghostty');
    expect(openLabel(lane({ targets: [] }), session({ openTarget: 's|claude:c:1|resume' }), 'Ghostty')).toBe('Resume in Ghostty');
    expect(openLabel(lane({ targets: [] }), session({ openTarget: 's|claude:c:1|claude-app' }), 'Ghostty')).toBe('Open in Claude app');
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
      'Remove from desktop',
      '-',
      'Copy path',
      'Copy branch name',
      'Copy PR link',
    ]);
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
    expect(labels(todoMenu('t1', { report, start: noop, session: s, desktop: new Set() }))).toEqual(['Start session…', 'Open claude: Fix login', '-', 'Pin to desktop', '-', 'Copy todo ID']);
  });
});
