import { afterEach, describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { setPlatform } from '../platform';
import { ALL_PROJECTS, CHATS, repoChoices, repoWorktrees } from '../quick';
import * as fx from '../testing/fixtures';
import type { Lane, Load } from '../types';
import { HERE, machineMenu, NEW_WORKTREE, newWorktreeOption, projectMenu, REPO, repoSections, worktreeMenu, worktreeOption } from './pickerOptions';

afterEach(() => setPlatform(null));

const lane = (id: string, extra: Partial<Lane> = {}): Lane => fx.lane({ id, sessions: [], branch: id, lastActivity: 0, ...extra });

describe('repoSections', () => {
  it('keeps repoChoices’ order across projects, each named after its project there', () => {
    // A and C have pinned worktrees; B is A's project's, and the latest.
    const repos = repoChoices([lane('a', { repo: 'a', project: 'Acme' }), lane('b', { repo: 'b', project: 'Acme', lastActivity: 9 }), lane('c', { repo: 'c', project: 'Globex' })], ['a', 'c']);
    const [section] = repoSections(repos, (r) => `in ${r.repo}`);
    expect(section.options.map((o) => [o.label, o.detail])).toEqual([
      ['Acme · a', 'in a'],
      ['Globex · c', 'in c'],
      ['Acme · b', 'in b'],
    ]);
    expect(section.options[0].id).toBe(`${REPO}${repos[0].key}`);
    // In one project, the repo's name alone.
    expect(repoSections(repoChoices([lane('a', { repo: 'a' })], []), () => '')[0].options.map((o) => [o.label, o.detail])).toEqual([['a', undefined]]);
  });
});

describe('worktreeOption', () => {
  it('names the main checkout main, with what a reusable worktree holds and its branch', () => {
    expect([worktreeOption(lane('/r/atlas', { origin: 'main', callsign: 'atlas', branch: 'main', reuse: 'open' }))].map((o) => [o.label, o.detail])).toEqual([['main', 'Work under way · main']]);
    expect([worktreeOption(lane('/r/atlas-2', { callsign: 'atlas-2', branch: 'atlas-2', reuse: 'free' }))].map((o) => [o.label, o.detail])).toEqual([['atlas-2', 'Ready for new work']]);
    expect(worktreeOption(lane('/r/fix', { callsign: 'fix', branch: 'fix-login' })).detail).toBe('fix-login');
    expect(newWorktreeOption('Claude Code makes one from main', '', true)).toMatchObject({ id: NEW_WORKTREE, label: 'New worktree', disabled: true });
  });
});

describe('worktreeMenu', () => {
  const main = lane('/r/atlas', { origin: 'main', callsign: 'atlas', branch: 'main' });
  const free = lane('/r/atlas-2', { reuse: 'free', lastActivity: 3 });
  const pinned = lane('/r/atlas-pin', { lastActivity: 2 });
  const busy = lane('/r/atlas-busy', { reuse: 'open', lastActivity: 1 });
  const other = lane('/r/atlas-other');
  const inRepo = repoWorktrees([main, free, pinned, busy, other], [pinned.id]);
  const ids = (sections: ReturnType<typeof worktreeMenu>) => sections.map((s) => [s.key, s.options.map((o) => o.id), !!s.folded]);

  it('lists the main checkout, then New worktree, then the ones ready and pinned, with the rest folded away', () => {
    const fresh = newWorktreeOption('Claude Code makes one from main', '', false);
    expect(ids(worktreeMenu(inRepo, fresh))).toEqual([
      ['new-work', [main.id, NEW_WORKTREE, free.id, pinned.id], false],
      ['under-way', [busy.id], true],
      ['others', [other.id], true],
    ]);
    expect(worktreeMenu(inRepo, fresh).map((s) => s.head)).toEqual([undefined, 'Work under way', 'Other worktrees']);
  });

  it('has no New worktree line where none can be asked for', () => {
    expect(ids(worktreeMenu(inRepo, null))[0]).toEqual(['new-work', [main.id, free.id, pinned.id], false]);
  });
});

describe('projectMenu', () => {
  const projects = [
    { name: 'Northwind', lanes: 2, repos: [] },
    { name: 'Atlas', lanes: 1, repos: [] },
  ];
  const counts = { Northwind: { needs: 2, unread: 1 }, [ALL_PROJECTS]: { needs: 2, unread: 1 } };
  /** What a line shows after its name, as text: what waits there, then its key. */
  const trail = (o: { trail?: unknown }) => renderToStaticMarkup(o.trail as never).replace(/<svg.*?<\/svg>/g, '').replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();

  it('lists the projects, then All projects and General chat set apart, General chat found by what it’s for', () => {
    const [mine, apart] = projectMenu(projects, counts, 'Alt+Cmd+/');
    expect(mine.options.map((o) => [o.id, o.label, o.detail])).toEqual([
      ['Northwind', 'Northwind', undefined],
      ['Atlas', 'Atlas', undefined],
    ]);
    expect(apart.head).toBeTruthy();
    expect(apart.options.map((o) => [o.id, o.label, o.detail])).toEqual([
      [ALL_PROJECTS, 'All projects', undefined],
      [CHATS, 'General chat', 'A question for none of your repos'],
    ]);
    expect(apart.options[1].text).toBe('General chat question no project no repo');
  });

  it('says what waits on you in each and the key that goes to it, as the platform has it', () => {
    const said = () => projectMenu(projects, counts, 'Alt+Cmd+/').flatMap((s) => s.options.map(trail));
    setPlatform('mac');
    expect(said()).toEqual(['2 1 ⌥⌘1', '⌥⌘2', '2 1 ⌥⌘0', '⌥⌘/']);
    setPlatform('linux');
    // ⌥⌘ and a digit is Alt and the digit there (platform.ts `LINUX`), and a global shortcut's ⌘ is Super.
    expect(said()).toEqual(['2 1 Alt+1', 'Alt+2', '2 1 Alt+0', 'Alt+Super+/']);
  });

  it('gives General chat no key while its shortcut isn’t set, and a tenth project none', () => {
    const many = Array.from({ length: 10 }, (_, i) => ({ name: `P${i + 1}`, lanes: 1, repos: [] }));
    setPlatform('mac');
    const [mine, apart] = projectMenu(many, {}, null);
    expect([trail(mine.options[8]), trail(mine.options[9]), trail(apart.options[1])]).toEqual(['⌥⌘9', '', '']);
  });
});

describe('machineMenu', () => {
  const MINI = 'a1b2c3d4e5';
  const STUDIO = 'f6e7d8c9b0';
  const here = [lane('/r/northwind/app', { project: 'Northwind' }), lane('/r/northwind/api', { project: 'Northwind' }), lane('/r/atlas/site', { project: 'Atlas' }), lane('/r/chats', { project: 'Elsewhere', startsChats: true, chats: true })];
  const there = [lane(`@${MINI}:/r/northwind/app`, { project: 'Northwind' }), lane(`@${MINI}:/r/atlas/site`, { project: 'Atlas' }), lane(`@${STUDIO}:/r/northwind/app`, { project: 'Northwind' })];
  const mini = fx.deviceView({ short: MINI, name: 'Mac mini', online: true, remoteControl: true });
  const studio = fx.deviceView({ short: STUDIO, name: 'Mac Studio', online: false, lastSeen: null });
  const devices = fx.devicesView({ devices: [mini, studio], lanes: there });
  const lines = (current: string, macs = [mini, studio]) => machineMenu(devices, macs, here, null, current, fx.NOW)[0].options.map((o) => [o.id, o.label, o.detail, !!o.disabled]);

  it('lists this machine first, then the others, each with its worktrees in the project picked', () => {
    expect(lines('Northwind')).toEqual([
      [HERE, 'MacBook Pro', 'This machine · 2 worktrees in Northwind', false],
      [MINI, 'Mac mini', '1 worktree in Northwind', false],
      [STUDIO, 'Mac Studio', 'Away', true],
    ]);
  });

  it('counts every project’s worktrees on All projects and on General chat, whose chats are in none', () => {
    expect(lines(ALL_PROJECTS).map((l) => l[2])).toEqual(['This machine · 3 worktrees', '2 worktrees', 'Away']);
    expect(lines(CHATS).map((l) => l[2])).toEqual(['This machine · 3 worktrees', '2 worktrees', 'Away']);
  });

  it('says why a session can’t start on a machine now, which can’t be picked', () => {
    const closed = fx.deviceView({ short: MINI, name: 'Mac mini', online: true, remoteControl: false });
    const [, line] = machineMenu(devices, [closed], here, null, 'Northwind', fx.NOW)[0].options;
    expect([line.detail, line.disabled, line.title]).toEqual(['Remote control is off there', true, 'Remote control is off on Mac mini; turn it on there in Settings › Devices to start sessions from here.']);
  });

  it('calls this machine This machine until it has a name, and says how busy each is where that’s known', () => {
    const busy: Load = { level: 'busy', perCpu: 1.2, cpus: 10, memoryFree: 30, memoryShort: false };
    const unnamed = fx.devicesView({ me: { ...devices.me, name: '' }, devices: [{ ...mini, load: busy }, { ...studio, load: busy }], lanes: there });
    const options = machineMenu(unnamed, unnamed.devices, here, busy, 'Northwind', fx.NOW)[0].options;
    expect([options[0].label, options[0].text]).toEqual(['This machine', 'This machine this machine this mac']);
    // A machine that's away doesn't say how busy it was.
    expect(options.map((o) => !!o.trail)).toEqual([true, true, false]);
    expect(machineMenu(devices, [mini], here, null, 'Northwind', fx.NOW)[0].options.map((o) => !!o.trail)).toEqual([false, false]);
  });
});
