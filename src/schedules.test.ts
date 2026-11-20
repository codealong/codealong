import { describe, expect, it } from 'vitest';
import { ALL_PROJECTS } from './quick';
import { asksRunsOn, blankSchedule, doesWords, fireCommand, firstSchedule, newScheduleFolder, nextWords, preferOptions, runsOnce, scheduleBySession, scheduleLane, scheduleLanes, scheduleListing, scheduleName, scheduleOf, scheduleProject, scheduleSections, scheduleSessions, sessionFolder, type ScheduleGrouping, type ScheduleSort } from './schedules';
import * as fx from './testing/fixtures';
import type { DevicesView, Schedule, ScheduleItem } from './types';

const NOW = Date.UTC(2026, 9, 20, 12, 0);
const HOUR = 3_600_000;

function item(over: Partial<ScheduleItem>): ScheduleItem {
  return { ...blankSchedule('/Users/me/projects/northwind-app'), id: 's-1', name: 'Triage', whenWords: 'Weekdays at 9:00', runs: [], asked: 0, ...over };
}

const UNGROUPED: ScheduleGrouping = { by: 'none', lanes: [], pinned: [], opened: new Set(), whole: false };
/** The view's first schedule, listed in that order with no groups. */
const firstOf = (items: ScheduleItem[], sort: ScheduleSort = 'next') => firstSchedule(scheduleListing(items, sort, UNGROUPED))?.id;

describe('schedules', () => {
  it('says when one runs next', () => {
    expect(nextWords(item({ nextAt: NOW + 2 * HOUR }), NOW)).toBe('Next in 2h');
    expect(nextWords(item({ nextAt: NOW - 1000 }), NOW)).toBe('Due now');
    expect(nextWords(item({ nextAt: NOW + 2 * HOUR, paused: true }), NOW)).toBe('Paused');
    expect(nextWords(item({ when: { kind: 'watch', command: 'gh run list', every: '5m', mode: 'lines' }, nextAt: NOW + 5 * 60_000 }), NOW)).toBe('Looks in 5m');
    expect(nextWords(item({ when: { kind: 'fired' }, nextAt: undefined }), NOW)).toBe('');
  });

  it('says when one runs next on the strip over its panel, beside the queue’s Next', () => {
    const strip = (over: Partial<ScheduleItem>) => nextWords(item(over), NOW, 'strip');
    expect(strip({ nextAt: NOW + 2 * HOUR })).toBe('Next run in 2h');
    expect(strip({ when: { kind: 'watch', command: 'gh run list', every: '5m', mode: 'lines' }, nextAt: NOW + 5 * 60_000 })).toBe('Next look in 5m');
    expect(strip({ nextAt: NOW - 1000 })).toBe('Due now');
    expect(strip({ nextAt: NOW + 2 * HOUR, paused: true })).toBe('Paused');
    expect(strip({ finished: 100, nextAt: NOW + HOUR })).toBe('Finished');
    expect(strip({ when: { kind: 'fired' }, nextAt: undefined })).toBe('Runs when it’s fired');
    expect(strip({ nextAt: undefined })).toBe('');
    // One that can't run says so, as its panel's tag does, whatever its time or how it's run.
    expect(strip({ when: { kind: 'fired' }, nextAt: undefined, problem: 'Its worktree is gone' })).toBe('Can’t run');
    expect(strip({ nextAt: NOW + 2 * HOUR, paused: true, problem: 'Its worktree is gone' })).toBe('Can’t run');
    // Its row keeps its own words.
    expect(nextWords(item({ nextAt: NOW + 2 * HOUR, problem: 'Its worktree is gone' }), NOW)).toBe('Next in 2h');
  });

  it('says what one does', () => {
    const title = (k: string) => (k === 'claude:claude:a' ? 'Fix the header' : null);
    expect(doesWords(item({}), title)).toBe('New session in northwind-app on Auto');
    expect(doesWords(item({ run: { ...item({}).run, to: 'thread', account: 'reviewers' } }), title)).toBe('Keeps one session going in northwind-app on reviewers');
    expect(doesWords(item({ run: { ...item({}).run, folder: '', worktree: true } }), title)).toBe('New session in a chat, a new worktree each time, on Auto');
    expect(doesWords(item({ run: { ...item({}).run, to: 'session', session: 'claude:claude:a' } }), title)).toBe('Sends to “Fix the header”');
    // Gone, a new session takes its place (runner.rs `in_place_of`), and the next runs go to that.
    expect(doesWords(item({ run: { ...item({}).run, to: 'session', session: 'gone' } }), title)).toBe('Sends to a session that’s gone, so a new one starts in northwind-app');
    expect(doesWords(item({ run: { ...item({}).run, to: 'session', session: 'gone' }, thread: 'claude:claude:a' }), title)).toBe('Sends to “Fix the header”');
  });

  it('folds the finished ones apart, the latest to finish first', () => {
    const a = item({ id: 's-a', name: 'A' });
    const b = item({ id: 's-b', name: 'B', finished: 100 });
    const c = item({ id: 's-c', name: 'C', finished: 300, once: true });
    const { active, finished } = scheduleSections([a, b, c]);
    expect(active.map((x) => x.id)).toEqual(['s-a']);
    expect(finished.map((x) => x.id)).toEqual(['s-c', 's-b']);
    // A finished one runs next never, paused or not.
    expect(nextWords(item({ finished: 100, nextAt: NOW + HOUR, paused: true }), NOW)).toBe('');
  });

  it('knows which run once', () => {
    expect(runsOnce(item({}))).toBe(false);
    expect(runsOnce(item({ once: true }))).toBe(true);
    // A `once <date>` time is one too, read as the engine reads a time: any case, commas as spaces.
    expect(runsOnce(item({ when: { kind: 'time', rule: 'once 2026-10-21 15:00' } }))).toBe(true);
    expect(runsOnce(item({ when: { kind: 'time', rule: ' Once,2026-10-21 3pm' } }))).toBe(true);
    expect(runsOnce(item({ when: { kind: 'time', rule: 'weekdays 9:00' } }))).toBe(false);
    // Only a time is read that way: a watch's command may start with any word.
    expect(runsOnce(item({ when: { kind: 'watch', command: 'once', every: '5m', mode: 'lines' } }))).toBe(false);
  });

  it('lists the ones that run once apart, the next to come first', () => {
    const watch = { kind: 'watch' as const, command: './deploys', every: '5m', mode: 'lines' as const };
    const daily = item({ id: 's-daily', nextAt: NOW + HOUR });
    const later = item({ id: 's-later', once: true, nextAt: NOW + 5 * HOUR });
    // A watch's next look is no run, however soon.
    const deploy = item({ id: 's-deploy', once: true, when: watch, nextAt: NOW + 60_000 });
    const dated = item({ id: 's-dated', when: { kind: 'time', rule: 'once 2026-10-20 15:00' }, nextAt: NOW + 3 * HOUR });
    const paused = item({ id: 's-paused', once: true, paused: true });
    const weekly = item({ id: 's-weekly', when: { kind: 'time', rule: 'mon 10:00' }, nextAt: NOW + 30 * 60_000 });
    const done = item({ id: 's-done', once: true, finished: 100 });
    const { active, oneOff, finished } = scheduleSections([daily, later, deploy, dated, paused, weekly, done]);
    // The ones that go on running come the same way, by default.
    expect(active.map((x) => x.id)).toEqual(['s-weekly', 's-daily']);
    expect(oneOff.map((x) => x.id)).toEqual(['s-dated', 's-later', 's-deploy', 's-paused']);
    expect(finished.map((x) => x.id)).toEqual(['s-done']);
    // Until one is picked the view shows the first it lists, not the first there is.
    expect(firstOf([done, later, dated, daily])).toBe('s-daily');
    expect(firstOf([done, later, dated])).toBe('s-dated');
    expect(firstOf([done])).toBe('s-done');
    expect(firstOf([])).toBeUndefined();
  });

  it('orders them as the View menu says, the finished the latest to finish first whatever it says', () => {
    const run = (at: number) => ({ at, why: '9:00', outcome: 'started' as const, said: '' });
    const watch = { kind: 'watch' as const, command: './deploys', every: '5m', mode: 'lines' as const };
    const triage = item({ id: 's-triage', name: 'Triage', createdAt: 1, nextAt: NOW + 5 * HOUR, runs: [run(NOW - 3 * HOUR)] });
    const deploys = item({ id: 's-deploys', name: 'deploys', createdAt: 2, when: watch, nextAt: NOW + 60_000, runs: [run(NOW - HOUR)] });
    const standup = item({ id: 's-standup', name: 'Standup 10', createdAt: 3, nextAt: NOW + HOUR });
    const paused = item({ id: 's-paused', name: 'Standup 9', createdAt: 4, nextAt: NOW + 30 * 60_000, paused: true, runs: [run(NOW - 2 * HOUR)] });
    const done = item({ id: 's-done', name: 'Archive', createdAt: 5, finished: 100 });
    const xs = [triage, deploys, standup, paused, done];
    const order = (sort: ScheduleSort) => scheduleSections(xs, sort).active.map((x) => x.id);
    // The next run first; a watch's next look and a paused one's time are no run, so they come
    // after, as they were made.
    expect(order('next')).toEqual(['s-standup', 's-triage', 's-deploys', 's-paused']);
    // The latest run first, the ones that haven't run after.
    expect(order('last')).toEqual(['s-deploys', 's-paused', 's-triage', 's-standup']);
    expect(order('added')).toEqual(['s-paused', 's-standup', 's-deploys', 's-triage']);
    // By name in any case, numbers as numbers: 9 before 10.
    expect(order('name')).toEqual(['s-deploys', 's-paused', 's-standup', 's-triage']);
    expect(scheduleSections(xs, 'name').finished.map((x) => x.id)).toEqual(['s-done']);
    expect(firstOf(xs, 'name')).toBe('s-deploys');
    // The ones it can't tell apart come as they were made, not as the file has them: one your
    // machines share is added to it as it arrives.
    const shared = item({ id: 's-shared', createdAt: 0, when: watch });
    expect(scheduleSections([...xs, shared], 'next').active.map((x) => x.id)).toEqual(['s-standup', 's-triage', 's-shared', 's-deploys', 's-paused']);
  });

  it('saves whether it runs once', () => {
    expect(blankSchedule('/w').once).toBe(false);
    expect(scheduleOf(item({ once: true, finishedAt: 5 })).once).toBe(true);
    // An orchestrator stays one through an edit, its leave to message and start sessions with it.
    expect(scheduleOf(item({ orchestrates: { project: 'Atlas', send: true } })).orchestrates).toEqual({ project: 'Atlas', send: true });
    expect('orchestrates' in scheduleOf(item({}))).toBe(false);
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
    const saved = scheduleOf(item({ problem: 'x', nextAt: 1, check: { at: 1, ok: true, said: '', failures: 0 }, runsOn: { kind: 'one', prefer: 'bb' }, runsOnWords: 'Next on Mini' }));
    expect(Object.keys(saved)).not.toContain('problem');
    expect(Object.keys(saved)).not.toContain('runs');
    expect(Object.keys(saved)).not.toContain('runsOnWords');
    expect(saved.name).toBe('Triage');
    // Which machines run it stays as it was: an edit doesn't make it this machine's alone.
    expect(saved.runsOn).toEqual({ kind: 'one', prefer: 'bb' });
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

  it('runs in its worktree, else its session’s, on this Mac', () => {
    expect(scheduleLane(blankSchedule('/w/atlas'), lanes)?.id).toBe('/w/atlas');
    expect(scheduleLane(sendsTo('claude:claude:b'), lanes)?.id).toBe('/w/brave-otter');
    expect(scheduleLane(sendsTo('claude:claude:b', '/w/atlas'), lanes)?.id).toBe('/w/atlas');
    // A chat of its own, a chat's session, another Mac's, and one that's gone are in none here.
    expect(scheduleLane(blankSchedule(''), lanes)).toBeNull();
    expect(scheduleLane(sendsTo('claude:claude:chat'), lanes)).toBeNull();
    expect(scheduleLane(sendsTo('@mini:claude:claude:e'), lanes)).toBeNull();
    expect(scheduleLane(blankSchedule('/w/gone'), lanes)).toBeNull();
  });

  it('groups them under the worktree or repo each runs in, the pinned ones first', () => {
    const at = (id: string, folder: string, over: Partial<ScheduleItem> = {}) => item({ ...blankSchedule(folder), id, name: id, whenWords: '', runs: [], asked: 0, ...over });
    const xs = [
      at('s-atlas', '/w/atlas', { nextAt: NOW + HOUR }),
      at('s-chat', '', { nextAt: NOW + 2 * HOUR }),
      at('s-otter', '/w/brave-otter', { nextAt: NOW + 3 * HOUR }),
      at('s-app', '/w/app', { nextAt: NOW + 4 * HOUR }),
      at('s-once', '/w/atlas', { once: true, nextAt: NOW + 5 * HOUR }),
      at('s-done', '/w/atlas', { finished: 100 }),
    ];
    const grouping = (by: ScheduleGrouping['by'], pinned: string[] = []): ScheduleGrouping => ({ by, lanes, pinned, opened: new Set(), whole: false });
    const heads = (l: ReturnType<typeof scheduleListing>) => l.grouped?.sections.map((s) => `${s.id}: ${s.groups.map((g) => `${g.key || 'none'} ${g.rows.map((x) => x.id).join(' ')}`).join(', ')}`);
    expect(scheduleListing(xs, 'next', UNGROUPED).grouped).toBeNull();
    // Each worktree once, in the order of its first; the one in none last; the one-offs and the
    // finished grouped among themselves.
    expect(heads(scheduleListing(xs, 'next', grouping('worktree')))).toEqual(['active: /w/atlas s-atlas, /w/brave-otter s-otter, /w/app s-app, none s-chat', 'oneOff: /w/atlas s-once', 'finished: /w/atlas s-done']);
    // By repo, the worktrees of one repo are one group (the fixtures' are all acme/app's).
    const byRepo = scheduleListing(xs, 'next', grouping('repo'));
    expect(byRepo.grouped?.sections[0].groups.map((g) => g.rows.map((x) => x.id).join(' '))).toEqual(['s-atlas s-otter s-app', 's-chat']);
    // A pinned worktree leads, and the view's first schedule is its first.
    const pinned = scheduleListing(xs, 'next', grouping('worktree', ['/w/app']));
    expect(pinned.grouped?.pinned.map((g) => g.rows.map((x) => x.id).join(' '))).toEqual(['s-app']);
    expect(firstSchedule(pinned)?.id).toBe('s-app');
    // A group lists five and folds the rest, but never one with a session at work under it.
    const many = Array.from({ length: 7 }, (_, i) => at(`s-${i}`, '/w/atlas', { nextAt: NOW + i * HOUR }));
    const folded = scheduleListing(many, 'next', { ...grouping('worktree'), awake: (x) => x.id === 's-6' });
    const g = folded.grouped?.sections[0].groups[0];
    expect(g?.rows.map((x) => x.id)).toEqual(['s-0', 's-1', 's-2', 's-3', 's-4', 's-6']);
    expect(g?.folded).toBe(1);
  });

  it('runs in the worktree the engine found for a folder written from your home', () => {
    const kestrel = fx.lane({ id: '/Users/me/code/kestrel', project: 'Kestrel' });
    const theirs = fx.lane({ id: '/Users/sam/code/kestrel', project: 'Kestrel' });
    const shared = { ...blankSchedule('~/code/kestrel'), worktree: '/Users/me/code/kestrel' };
    expect(scheduleLane(shared, [theirs, kestrel])?.id).toBe('/Users/me/code/kestrel');
    expect(scheduleProject(shared, [theirs, kestrel])).toBe('Kestrel');
    // The view doesn't read `~` itself: which home it is is the engine's to say.
    expect(scheduleLane(blankSchedule('~/code/kestrel'), [theirs, kestrel])).toBeNull();
  });

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

  it('is where it runs here, for one another machine shared whose checkout is elsewhere here', () => {
    const shared = { ...blankSchedule('~/code/atlas'), runsIn: '/w/atlas' };
    expect(scheduleProject(shared, lanes)).toBe('Atlas');
    expect(doesWords(shared, () => null)).toBe('New session in atlas on Auto');
  });

  it('is an orchestrator’s project, and All projects’ is in none', () => {
    expect(scheduleProject({ ...blankSchedule(''), orchestrates: { project: 'Atlas' } }, lanes)).toBe('Atlas');
    expect(scheduleProject({ ...blankSchedule('/w/atlas'), orchestrates: {} }, lanes)).toBeNull();
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

  it('offers this machine and your others to prefer, not phones', () => {
    const device = (key: string, name: string, kind: 'mac' | 'phone') => ({ key, short: key, name, color: 'blue', online: true, via: null, since: null, lastSeen: null, remoteControl: false, app: '', linkedBack: true, loading: false, usage: [], kind }) as DevicesView['devices'][number];
    const devices = { enabled: true, me: { key: 'aa', short: 'aa', name: 'Studio', tailscale: 'running', tailnet: null, address: null, iroh: 'online', remoteControl: false, error: null }, devices: [device('bb', 'Mini', 'mac'), device('cc', 'Phone', 'phone')], phones: [], found: [], asks: [], lanes: [], prSessions: {} } as DevicesView;
    expect(preferOptions(devices, '').map((o) => o.label)).toEqual(['No machine in particular', 'This machine, Studio', 'Mini']);
    expect(preferOptions(devices, 'dd').at(-1)).toEqual({ id: 'dd', label: 'A machine that’s no longer linked' });
    expect(asksRunsOn(devices, undefined)).toBe(true);
    expect(asksRunsOn({ ...devices, devices: [] }, undefined)).toBe(false);
    expect(asksRunsOn(null, { kind: 'every' })).toBe(true);
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

describe('scheduleName', () => {
  it('calls an orchestrator Orchestrator, whatever project its stored name says, and the rest by their names', () => {
    expect(scheduleName({ name: 'Northwind orchestrator', orchestrates: { project: 'Northwind' } })).toBe('Orchestrator');
    expect(scheduleName({ name: 'Orchestrator', orchestrates: {} })).toBe('Orchestrator');
    expect(scheduleName({ name: 'Weekly hygiene' })).toBe('Weekly hygiene');
    // A name you gave one yourself stays.
    expect(scheduleName({ name: 'Night watch', orchestrates: { project: 'Northwind' } })).toBe('Night watch');
    expect(scheduleName({ name: 'Northwind Orchestrator', orchestrates: { project: 'Northwind' } })).toBe('Northwind Orchestrator');
    expect(scheduleName({ name: 'Northwind  Ops orchestrator', orchestrates: { project: 'Northwind Ops' } })).toBe('Orchestrator');
  });
});
