import { describe, expect, it } from 'vitest';
import { CODEALONG_PROJECT, improvePrompt, improveTitle, improveWords, remoteImproveLane, withCodealongProject } from './improve';
import type { Attachment, Config, Lane } from './types';

const shot = (name: string, hash: string): Attachment => ({ kind: 'image', name, path: `images/${hash}.png` });

describe('improvePrompt', () => {
  it('puts the pictures after what you wrote, each named for its window', () => {
    const shots = [shot('Main window', 'aaa'), shot('Floating bar', 'bbb')];
    expect(improvePrompt('  Make the bar taller.  ', shots)).toBe('Make the bar taller.\n\nCodealong’s windows as they were when this was asked:\n![Main window](images/aaa.png)\n![Floating bar](images/bbb.png)');
  });

  it('is what you wrote alone without pictures, and the pictures alone without words', () => {
    expect(improvePrompt('Fix it', [])).toBe('Fix it');
    expect(improvePrompt('', [shot('Main window', 'aaa')])).toBe('Codealong’s windows as they were when this was asked:\n![Main window](images/aaa.png)');
  });

  it('keeps a window’s name inside its brackets', () => {
    expect(improvePrompt('x', [shot('Odd [name]\nhere', 'c')])).toContain('![Odd namehere](images/c.png)');
  });
});

describe('withCodealongProject', () => {
  const cfg = { projects: [{ name: 'Kestrel', repos: ['github.com/kestrel/*'], folders: [] }] } as unknown as Config;

  it('adds a Codealong project for the source’s repo after the others', () => {
    const out = withCodealongProject(cfg, { root: '/me/projects/codealong', repo: 'github.com/me/codealong' });
    expect(out.projects).toEqual([cfg.projects[0], { name: CODEALONG_PROJECT, repos: ['github.com/me/codealong'], folders: [] }]);
    // The config given is left as it was.
    expect(cfg.projects).toHaveLength(1);
  });

  it('lists the folder of a checkout with no remote', () => {
    const out = withCodealongProject(cfg, { root: '/me/projects/codealong', repo: null });
    expect(out.projects[1]).toEqual({ name: CODEALONG_PROJECT, repos: [], folders: ['/me/projects/codealong'] });
  });

  it('adds to a project already named Codealong, once', () => {
    const have = { projects: [{ name: CODEALONG_PROJECT, repos: ['github.com/me/codealong-ios'], folders: [] }] } as unknown as Config;
    const once = withCodealongProject(have, { root: '/r', repo: 'github.com/me/codealong' });
    expect(once.projects).toEqual([{ name: CODEALONG_PROJECT, repos: ['github.com/me/codealong-ios', 'github.com/me/codealong'], folders: [] }]);
    expect(withCodealongProject(once, { root: '/r', repo: 'github.com/me/codealong' })).toEqual(once);
  });
});

describe('remoteImproveLane', () => {
  const lane = (id: string, more: Partial<Lane>): Lane => ({ id, project: 'Tools', repo: 'codealong', path: id, callsign: id, origin: 'other', bay: 'idle', glyph: 'idle', parked: false, hidden: false, headline: '', sessions: [], lastActivity: 0, looksFinished: false, targets: [], ...more } as Lane);

  const mini = { name: 'Mini', short: 'abc', color: 'blue' as const, online: true, lastSeen: null };

  it('names a shown main checkout of the source among the other Macs’ worktrees, a connected Mac’s first', () => {
    const wt = lane('@abc:/m/codealong/wt', { codealong: true, device: mini });
    const main = lane('@abc:/m/codealong', { codealong: true, origin: 'main', device: mini });
    const hiddenMain = lane('@def:/n/codealong', { codealong: true, origin: 'main', hidden: true, device: { ...mini, short: 'def', online: false } });
    expect(remoteImproveLane([lane('@abc:/m/other', {}), wt, main])?.id).toBe('@abc:/m/codealong');
    expect(remoteImproveLane([hiddenMain, wt])?.id).toBe('@abc:/m/codealong/wt');
    expect(remoteImproveLane([hiddenMain])?.id).toBe('@def:/n/codealong');
    // A connected Mac's worktree before a disconnected one's main checkout.
    expect(remoteImproveLane([lane('@def:/n/codealong', { codealong: true, origin: 'main', device: { ...mini, online: false } }), wt])?.id).toBe('@abc:/m/codealong/wt');
    expect(remoteImproveLane([lane('@abc:/m/other', {}), lane('@abc:/chats', { codealong: true, startsChats: true })])).toBeNull();
  });

});

describe('improveWords', () => {
  it('says nothing while the board is read or when the source is in a project', () => {
    expect(improveWords(undefined, true)).toBeNull();
    expect(improveWords({ project: 'Codealong', laneId: '/p/codealong' }, false)).toBeNull();
  });

  it('says the session starts on the Mac that has the source, without a button', () => {
    const remote = { id: '@abc:/m/codealong', codealong: true, device: { name: 'Mini', short: 'abc', color: 'blue', online: true, lastSeen: null } } as unknown as Lane;
    const words = improveWords({}, false, remote);
    expect(words?.add).toBe(false);
    expect(words?.text).toMatch(/^Codealong’s source is on Mini, not on this Mac, so the session starts there and the pictures go with it/);
  });

  it('offers a project for a source in none, and says where to clone one otherwise', () => {
    const found = improveWords({ source: { root: '/me/projects/codealong', repo: 'github.com/me/codealong' } }, false);
    expect(found?.add).toBe(true);
    expect(found?.text).toContain('/me/projects/codealong');
    const none = improveWords({}, false);
    expect(none?.add).toBe(false);
    expect(none?.text).toContain('github.com/codealong/codealong');
    expect(improveWords(undefined, false)?.add).toBe(false);
  });
});

describe('improveTitle', () => {
  it('names the shortcut when there is one', () => {
    expect(improveTitle('⌥⌘,')).toMatch(/pictures of its windows as they are now \(⌥⌘,\)$/);
    expect(improveTitle(null)).not.toContain('(');
  });
});
