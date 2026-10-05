import { describe, expect, it } from 'vitest';
import { completed, leftOutWords, matching, repoLanes, sessionSkills, skillRows, skillStart, slashAt, writtenFor } from './skills';
import * as fx from './testing/fixtures';
import type { Skill, SkillsListed } from './types';

const skill = (name: string, over: Partial<Skill> = {}): Skill => ({ name, description: `${name} things`, typed: true, byModel: true, written: { claude: `/${name}`, codex: `$${name}` }, uses: 0, lastUsed: 0, ...over });

describe('slashAt', () => {
  it('reads a name being typed, and one with what follows it', () => {
    expect(slashAt('/')).toEqual({ name: '', rest: '', open: true });
    expect(slashAt('/sh')).toEqual({ name: 'sh', rest: '', open: true });
    expect(slashAt('/ship ')).toEqual({ name: 'ship', rest: '', open: false });
    expect(slashAt('  /figma:figma-use the header\nand the footer')).toEqual({ name: 'figma:figma-use', rest: 'the header\nand the footer', open: false });
  });

  it('is nothing for text that only has a slash in it', () => {
    expect(slashAt('fix /ship')).toBeNull();
    expect(slashAt('/Users/me/file.txt is wrong')).toBeNull();
    expect(slashAt('')).toBeNull();
  });
});

describe('matching', () => {
  const skills = [skill('ship'), skill('figma:shaders'), skill('worship'), skill('db')];

  it('lists those that start with what’s typed, then those that have it', () => {
    expect(matching(skills, 'sh').map((s) => s.name)).toEqual(['ship', 'figma:shaders', 'worship']);
    expect(matching(skills, 'SH').map((s) => s.name)).toEqual(['ship', 'figma:shaders', 'worship']);
    expect(matching(skills, '').map((s) => s.name)).toEqual(['ship', 'figma:shaders', 'worship', 'db']);
    expect(matching(skills, 'zz')).toEqual([]);
  });
});

describe('writtenFor', () => {
  const skills = [skill('ship'), skill('probe', { written: { codex: '$probe' } })];

  it('writes a skill’s name as the harness takes it', () => {
    expect(writtenFor('/ship the fix', skills, 'claude')).toBe('/ship the fix');
    expect(writtenFor('/ship the fix', skills, 'codex')).toBe('$ship the fix');
    expect(writtenFor('/ship', skills, 'codex')).toBe('$ship');
    expect(writtenFor(' /ship\nand then rest', skills, 'codex')).toBe(' $ship\nand then rest');
  });

  it('leaves what isn’t one of the harness’s skills as typed', () => {
    expect(writtenFor('/compact', skills, 'codex')).toBe('/compact');
    expect(writtenFor('/probe it', skills, 'claude')).toBe('/probe it');
    expect(writtenFor('/shipping', skills, 'codex')).toBe('/shipping');
    expect(writtenFor('run /ship', skills, 'codex')).toBe('run /ship');
  });
});

describe('completed', () => {
  it('finishes the name and keeps what followed', () => {
    expect(completed('/sh', skill('ship'))).toBe('/ship ');
    expect(completed('/sh the fix', skill('ship'))).toBe('/ship the fix');
  });
});

describe('sessionSkills', () => {
  it('lists the worktree’s before the account’s, each name once, for the session’s harness', () => {
    const listed: SkillsListed = {
      folders: [{ folder: '/r/app', skills: [skill('ship'), skill('claude-only', { written: { claude: '/claude-only' } })] }],
      accounts: [{ account: 'codex', skills: [skill('ship', { description: 'the account’s' }), skill('imagegen', { written: { codex: '$imagegen' } })] }],
    };
    expect(sessionSkills(listed, 'codex').map((s) => [s.name, s.description])).toEqual([
      ['ship', 'ship things'],
      ['imagegen', 'imagegen things'],
    ]);
    expect(sessionSkills(null, 'codex')).toEqual([]);
  });
});

describe('leftOutWords', () => {
  const accounts = [
    { name: 'claude', harness: 'claude' as const },
    { name: 'claude2', harness: 'claude' as const },
    { name: 'codex', harness: 'codex' as const },
    { name: 'opencode', harness: 'opencode' as const },
  ];

  it('says which accounts a skill leaves out, and why', () => {
    expect(leftOutWords(skill('ship'), accounts)).toBe('/ship isn’t where OpenCode reads skills, so opencode isn’t offered.');
    expect(leftOutWords(skill('probe', { written: { codex: '$probe' } }), accounts)).toBe('/probe isn’t where Claude Code and OpenCode read skills, so claude, claude2 and opencode aren’t offered.');
    expect(leftOutWords(skill('ship'), accounts.slice(0, 3))).toBeNull();
  });
});

describe('the quick panel’s skills', () => {
  const main = fx.lane({ id: '/r/app', origin: 'main' });
  const tree = fx.lane({ id: '/r/app-fix', reuse: 'free' });
  const web = fx.lane({ id: '/r/web-fix', repo: 'web' });
  const webMain = fx.lane({ id: '/r/web', repo: 'web', origin: 'main' });
  const away = fx.lane({ id: '@ab12:/r/far', repo: 'far', device: { short: 'ab12', name: 'Mini', color: 'blue' } as never });
  const listed: SkillsListed = {
    folders: [
      { folder: '/r/app', skills: [skill('ship', { uses: 5 }), skill('shape'), skill('rules', { typed: false, uses: 90 })] },
      { folder: '/r/web', skills: [skill('ship', { uses: 2 }), skill('publish')] },
    ],
    accounts: [],
  };

  it('reads each repo’s skills from its main checkout', () => {
    expect(repoLanes([tree, main, web, webMain, away]).map((l) => l.id)).toEqual(['/r/app', '/r/web']);
    // A repo with no main checkout listed goes by a worktree of it.
    expect(repoLanes([web]).map((l) => l.id)).toEqual(['/r/web-fix']);
  });

  it('lists every repo’s matches, the likeliest first', () => {
    const rows = skillRows(listed, [main, webMain], 'sh', false, 9);
    expect(rows.map((r) => `${r.lane.repo}/${r.skill.name}`)).toEqual(['app/ship', 'app/shape', 'web/publish', 'web/ship']);
    expect(rows[0].marks).toEqual([[1, 3]]);
    // Ones kept off the menu come after the ones meant to be typed, however often they ran.
    expect(skillRows(listed, [main, webMain], '', false, 9).map((r) => r.skill.name)).toEqual(['ship', 'publish', 'shape', 'rules', 'ship']);
    expect(skillRows(listed, [main, webMain], '', false, 2)).toHaveLength(2);
    expect(skillRows(null, [main], 'sh', false, 9)).toEqual([]);
  });

  it('with the name finished, lists only the skills of that name', () => {
    expect(skillRows(listed, [main, webMain], 'ship', true, 9).map((r) => r.lane.repo)).toEqual(['app', 'web']);
    expect(skillRows(listed, [main, webMain], 'shape', true, 9).map((r) => r.lane.repo)).toEqual(['app']);
    expect(skillRows(listed, [main, webMain], 'sh', true, 9)).toEqual([]);
  });

  it('starts in a free reusable worktree of the repo, else a new one', () => {
    expect(skillStart(main, [main, tree, web, webMain])).toEqual({ lane: tree, newWorktree: false });
    expect(skillStart(webMain, [main, tree, web, webMain])).toEqual({ lane: webMain, newWorktree: true });
  });
});
