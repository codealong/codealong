import { describe, expect, it } from 'vitest';
import { calmerMachine, loadNumbers, loadSentence } from './machineLoad';
import * as fx from './testing/fixtures';
import type { DeviceView, Lane, Load, LoadLevel } from './types';

const load = (level: LoadLevel, perCpu: number | null = level === 'calm' ? 0.3 : level === 'busy' ? 2 : 27): Load => ({ level, perCpu, cpus: 10, memoryFree: 40, memoryShort: false });

const mac = (short: string, name: string, l: Load | null, over: Partial<DeviceView> = {}): DeviceView => ({
  key: short.repeat(4),
  short,
  name,
  color: 'teal',
  online: true,
  via: 'tailscale',
  since: 0,
  lastSeen: null,
  remoteControl: true,
  app: '0.25.0',
  linkedBack: true,
  loading: false,
  usage: [],
  kind: 'mac',
  load: l,
  ...over,
});

// This machine's lanes, and another machine's, which come with `@<short>:` before their ids.
const here: Lane[] = [fx.lane({ id: '/r/app', project: 'Acme', repo: 'app' }), fx.lane({ id: '/r/chats', project: 'Acme', repo: 'chats', startsChats: true })];
const there = (short: string, repo = 'app'): Lane[] => [fx.lane({ id: `@${short}:/m/${repo}`, project: 'Acme', repo, repoSlug: `acme/${repo}` }), fx.lane({ id: `@${short}:/m/chats`, project: 'Acme', repo: 'chats', repoSlug: null, startsChats: true })];

describe('the machine the quick panel offers', () => {
  it('is a calm one with the repo, while the start goes to a strained one', () => {
    const offered = calmerMachine({ on: null, lane: here[0] }, { name: 'MacBook', load: load('strained'), lanes: here }, [mac('aa', 'Mac mini', load('calm'))], there('aa'));
    expect(offered?.on).toBe('aa');
    expect(offered?.name).toBe('Mac mini');
    expect(offered?.lane.id).toBe('@aa:/m/app');
  });

  it('is none while the start goes to a machine that isn’t strained', () => {
    for (const l of [load('calm'), load('busy'), null]) expect(calmerMachine({ on: null, lane: here[0] }, { name: 'MacBook', load: l, lanes: here }, [mac('aa', 'Mac mini', load('calm'))], there('aa'))).toBeNull();
  });

  it('is never a busy or strained machine, nor one whose load isn’t known', () => {
    for (const l of [load('busy'), load('strained'), null]) expect(calmerMachine({ on: null, lane: here[0] }, { name: 'MacBook', load: load('strained'), lanes: here }, [mac('aa', 'Mac mini', l)], there('aa'))).toBeNull();
  });

  it('needs the same repo there, or chats for a chat, and a repo picked', () => {
    const strained = { name: 'MacBook', load: load('strained'), lanes: here };
    expect(calmerMachine({ on: null, lane: here[0] }, strained, [mac('aa', 'Mac mini', load('calm'))], there('aa', 'web'))).toBeNull();
    expect(calmerMachine({ on: null, lane: here[1] }, strained, [mac('aa', 'Mac mini', load('calm'))], there('aa', 'web'))?.lane.id).toBe('@aa:/m/chats');
    expect(calmerMachine({ on: null, lane: null }, strained, [mac('aa', 'Mac mini', load('calm'))], there('aa'))).toBeNull();
  });

  it('goes by the GitHub repo, not the name each machine gives it, and prefers its main checkout', () => {
    const strained = { name: 'MacBook', load: load('strained'), lanes: here };
    const from = fx.lane({ id: '/r/api', project: 'Acme', repo: 'api', repoSlug: 'northwind/api' });
    const other = fx.lane({ id: '@aa:/m/api', project: 'Acme', repo: 'api', repoSlug: 'kestrel/api' });
    const mini = [mac('aa', 'Mac mini', load('calm'))];
    expect(calmerMachine({ on: null, lane: from }, strained, mini, [other])).toBeNull();
    // Renamed since that machine cloned it, under another folder name there.
    const renamed = fx.lane({ id: '@aa:/m/old-api', project: 'Acme', repo: 'old-api', repoSlug: 'northwind/old-api', repoSlugNow: 'Northwind/API', origin: 'main' });
    const worktree = fx.lane({ id: '@aa:/m/api-wt', project: 'Acme', repo: 'old-api', repoSlug: 'northwind/old-api', repoSlugNow: 'northwind/api', origin: 'script' });
    expect(calmerMachine({ on: null, lane: from }, strained, mini, [worktree, renamed, other])?.lane.id).toBe('@aa:/m/old-api');
    // A folder with no GitHub repo goes by its project and name.
    const folder = fx.lane({ id: '/r/notes', project: 'Acme', repo: 'notes', repoSlug: null });
    expect(calmerMachine({ on: null, lane: folder }, strained, mini, [fx.lane({ id: '@aa:/m/notes', project: 'Acme', repo: 'notes', repoSlug: null })])?.lane.id).toBe('@aa:/m/notes');
    expect(calmerMachine({ on: null, lane: folder }, strained, mini, [fx.lane({ id: '@aa:/m/notes', project: 'Acme', repo: 'notes', repoSlug: 'kestrel/notes' })])).toBeNull();
  });

  it('can be started on from here: connected and taking remote control', () => {
    const strained = { name: 'MacBook', load: load('strained'), lanes: here };
    expect(calmerMachine({ on: null, lane: here[0] }, strained, [mac('aa', 'Mac mini', load('calm'), { online: false })], there('aa'))).toBeNull();
    expect(calmerMachine({ on: null, lane: here[0] }, strained, [mac('aa', 'Mac mini', load('calm'), { remoteControl: false })], there('aa'))).toBeNull();
  });

  it('is the least loaded of the calm ones', () => {
    const macs = [mac('aa', 'Mac mini', load('calm', 0.7)), mac('bb', 'Studio', load('calm', 0.1))];
    expect(calmerMachine({ on: null, lane: here[0] }, { name: 'MacBook', load: load('strained'), lanes: here }, macs, [...there('aa'), ...there('bb')])?.on).toBe('bb');
  });

  it('is this machine when the start goes to another that’s strained', () => {
    const offered = calmerMachine({ on: 'aa', lane: there('aa')[0] }, { name: 'MacBook', load: load('calm'), lanes: here }, [mac('aa', 'Mac mini', load('strained'))], there('aa'));
    expect(offered?.on).toBeNull();
    expect(offered?.lane.id).toBe('/r/app');
  });
});

describe('a load in words', () => {
  it('says its level and numbers', () => {
    expect(loadSentence({ level: 'strained', perCpu: 27, cpus: 10, memoryFree: 40, memoryShort: true }, 'Mac mini')).toBe('Mac mini is strained: load 27 per CPU of 10, 40% of memory free, memory getting short.');
    // Windows keeps no load average.
    expect(loadNumbers({ level: 'calm', perCpu: null, cpus: 8, memoryFree: 55, memoryShort: false })).toBe('55% of memory free');
    expect(loadSentence({ level: 'calm', perCpu: null, cpus: 8, memoryFree: null, memoryShort: false }, 'This machine')).toBe('This machine is calm.');
  });
});
