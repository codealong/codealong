import { describe, expect, it } from 'vitest';
import { blankSchedule, doesWords, fireCommand, nextWords, scheduleBySession, scheduleFolders, scheduleOf } from './schedules';
import type { Lane, ScheduleItem } from './types';

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

  it('lists the folders a schedule can start in', () => {
    const lane = (path: string, callsign: string, origin: Lane['origin'], device = false) => ({ id: path, path, project: 'Northwind', repo: 'app', callsign, origin, hidden: false, sessions: [], ...(device ? { device: { id: 'd', name: 'mini' } } : {}) }) as unknown as Lane;
    const folders = scheduleFolders([lane('/w/b', 'brave-otter', 'claude-app'), lane('/w/a', 'app', 'main'), lane('/w/c', 'calm-heron', 'main', true)]);
    expect(folders.map((f) => f.label)).toEqual(['Northwind · app', 'Northwind · brave-otter']);
  });
});
