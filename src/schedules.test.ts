import { describe, expect, it } from 'vitest';
import { ALL_PROJECTS } from './quick';
import { blankSchedule, doesWords, fireCommand, newScheduleFolder, nextWords, scheduleBySession, scheduleLanes, scheduleOf, scheduleProject, scheduleSessions, sessionFolder } from './schedules';
import * as fx from './testing/fixtures';
import type { Schedule, ScheduleItem } from './types';

const NOW = Date.UTC(2026, 9, 20, 12, 0);
const HOUR = 3_600_000;

function item(over: Partial<ScheduleItem>): ScheduleItem {
  return { ...blankSchedule('/Users/me/projects/northwind-app'), id: 's-1', name: 'Triage', whenWords: 'Weekdays at 9:00', runs: [], asked: 0, ...over };
}

describe('schedules', () => {
  it('says when one runs next', () => {
    expect(nextWords(item({ nextAt: NOW + 2 * HOUR }), NOW)).toBe('Next in 2h');
    expect(nextWords(item({ nextAt: NOW - 1000 }), NOW)).toBe('Due now');
    expect(nextWords(item({ nextAt: NOW + 2 * HOUR, paused: true }), NOW)).toBe('Paused');
    expect(nextWords(item({ when: { kind: 'watch', command: 'gh run list', every: '5m', mode: 'lines' }, nextAt: NOW + 5 * 60_000 }), NOW)).toBe('Looks in 5m');
    expect(nextWords(item({ when: { kind: 'fired' }, nextAt: undefined }), NOW)).toBe('');
  });

  it('says what one does', () => {
    const title = (k: string) => (k === 'claude:claude:a' ? 'Fix the header' : null);
    expect(doesWords(item({}), title)).toBe('New session in northwind-app on Auto');
    expect(doesWords(item({ run: { ...item({}).run, to: 'thread', account: 'reviewers' } }), title)).toBe('Keeps one session going in northwind-app on reviewers');
    expect(doesWords(item({ run: { ...item({}).run, folder: '', worktree: true } }), title)).toBe('New session in a chat, a new worktree each time, on Auto');
    expect(doesWords(item({ run: { ...item({}).run, to: 'session', session: 'claude:claude:a' } }), title)).toBe('Sends to “Fix the header”');
    expect(doesWords(item({ run: { ...item({}).run, to: 'session', session: 'gone' } }), title)).toBe('Sends to a session that’s gone');
  });

  it('knows which schedule a session came from', () => {
    const a = item({ id: 's-a', name: 'A', runs: [{ at: 10, why: '9:00', outcome: 'started', said: '', session: 'k1' }, { at: 5, why: '9:00', outcome: 'skipped', said: '', session: 'k2' }] });
    const b = item({ id: 's-b', name: 'B', thread: 'k3', runs: [{ at: 20, why: '9:00', outcome: 'started', said: '', session: 'k1' }] });
    const by = scheduleBySession([a, b]);
    expect(by.get('k1')).toEqual({ id: 's-b', name: 'B' });
    expect(by.has('k2')).toBe(false);
    expect(by.get('k3')).toEqual({ id: 's-b', name: 'B' });
  });

  it('saves only what a schedule is', () => {
    const saved = scheduleOf(item({ problem: 'x', nextAt: 1, check: { at: 1, ok: true, said: '', failures: 0 } }));
    expect(Object.keys(saved)).not.toContain('problem');
    expect(Object.keys(saved)).not.toContain('runs');
    expect(saved.name).toBe('Triage');
  });

  it('writes the command that fires one', () => {
    expect(fireCommand('deploys')).toBe("codealong schedule fire deploys --key <what it's for> --title <what happened>");
    expect(fireCommand('After a deploy')).toContain('fire "After a deploy" --key');
  });

  // Two projects' worktrees, another Mac's, a hidden one and the chats.
  const main = fx.lane({ id: '/w/app', callsign: 'app', origin: 'main', project: 'Northwind', lastActivity: fx.NOW - 5 * HOUR, sessions: [fx.session({ key: 'claude:claude:a', lastActivity: fx.NOW - 3 * HOUR })] });
  const otter = fx.lane({
    id: '/w/brave-otter',
    project: 'Northwind',
    lastActivity: fx.NOW - HOUR,
    sessions: [fx.session({ key: 'claude:claude:b', lastActivity: fx.NOW - HOUR }), fx.session({ key: 'codex:codex:c', lastActivity: fx.NOW - 2 * HOUR }), fx.session({ key: 'claude:claude:old', archived: true })],
  });
  const atlas = fx.lane({ id: '/w/atlas', callsign: 'atlas', origin: 'main', project: 'Atlas', lastActivity: fx.NOW, sessions: [fx.session({ key: 'claude:claude:d', lastActivity: fx.NOW - 10 * HOUR })] });
  const away = fx.lane({ id: '@mini:/w/app', path: '/w/app', project: 'Northwind', device: { id: 'mini', name: 'mini' } as never, sessions: [fx.session({ key: '@mini:claude:claude:e', lastActivity: fx.NOW })] });
  const hidden = fx.lane({ id: '/w/old', project: 'Northwind', hidden: true });
  const chats = fx.lane({ id: 'chats', path: '/Users/me/.codealong/chats', project: 'No project', startsChats: true, sessions: [fx.session({ key: 'claude:claude:chat', lastActivity: fx.NOW - 30 * 60_000 })] });
  const lanes = [main, otter, atlas, away, hidden, chats];
  const sendsTo = (session: string, folder = ''): Pick<Schedule, 'run'> => ({ run: { ...blankSchedule(folder).run, to: 'session', session } });

  it('offers this Mac’s worktrees, not its chats', () => {
    expect(scheduleLanes(lanes).map((l) => l.id)).toEqual(['/w/app', '/w/brave-otter', '/w/atlas']);
  });

  it('offers the sessions in its worktree first, the latest first', () => {
    const { here, rest } = scheduleSessions(lanes, '/w/brave-otter', '');
    expect(here.map((p) => p.session.key)).toEqual(['claude:claude:b', 'codex:codex:c']);
    expect(rest.map((p) => p.session.key)).toEqual(['claude:claude:chat', 'claude:claude:a', 'claude:claude:d']);
    // In no worktree, they're all the rest; an archived one only once it's picked.
    expect(scheduleSessions(lanes, '', '').here).toEqual([]);
    expect(scheduleSessions(lanes, '', 'claude:claude:old').rest.map((p) => p.session.key)).toContain('claude:claude:old');
  });

  it('takes a session’s worktree as its own', () => {
    expect(sessionFolder(lanes, 'codex:codex:c')).toBe('/w/brave-otter');
    expect(sessionFolder(lanes, 'claude:claude:chat')).toBe('');
    expect(sessionFolder(lanes, 'claude:claude:gone')).toBe('');
  });

  it('is in its worktree’s project, else its session’s', () => {
    expect(scheduleProject(blankSchedule('/w/atlas'), lanes)).toBe('Atlas');
    expect(scheduleProject(sendsTo('claude:claude:b'), lanes)).toBe('Northwind');
    // Its own worktree wins over its session's.
    expect(scheduleProject(sendsTo('claude:claude:b', '/w/atlas'), lanes)).toBe('Atlas');
    expect(scheduleProject(blankSchedule(''), lanes)).toBeNull();
    expect(scheduleProject(sendsTo('claude:claude:chat'), lanes)).toBeNull();
    expect(scheduleProject(blankSchedule('/w/gone'), lanes)).toBeNull();
  });

  it('starts a new one in the worktree selected, else where the quick panel would', () => {
    expect(newScheduleFolder(lanes, 'Northwind', otter, [])).toBe('/w/brave-otter');
    // A selection in another project isn't this one's: its main checkout leads.
    expect(newScheduleFolder(lanes, 'Northwind', atlas, [])).toBe('/w/app');
    expect(newScheduleFolder(lanes, 'Northwind', null, ['/w/brave-otter'])).toBe('/w/brave-otter');
    expect(newScheduleFolder(lanes, ALL_PROJECTS, { ...atlas }, [])).toBe('/w/atlas');
    expect(newScheduleFolder(lanes, ALL_PROJECTS, null, [])).toBe('');
    expect(newScheduleFolder(lanes, 'Northwind', chats, [])).toBe('/w/app');
  });
});
