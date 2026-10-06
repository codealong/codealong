import { describe, expect, it } from 'vitest';
import { completed, leftOutWords, matching, mentionAt, mentionsIn, repoLanes, sessionSkills, skillNamed, skillRows, skillStart, writtenFor } from './skills';
import * as fx from './testing/fixtures';
import type { Skill, SkillsListed } from './types';

const skill = (name: string, over: Partial<Skill> = {}): Skill => ({ name, description: `${name} things`, typed: true, byModel: true, written: { claude: `/${name}`, codex: `$${name}` }, uses: 0, lastUsed: 0, ...over });

describe('mentionsIn and mentionAt', () => {
  it('finds a slash and a name at the start or after a space, wherever it is', () => {
    expect(mentionsIn('/')).toEqual([{ name: '', start: 0, end: 1 }]);
    expect(mentionsIn('/sh')).toEqual([{ name: 'sh', start: 0, end: 3 }]);
    expect(mentionsIn('fix the login then /ship it\nand /figma:figma-use')).toEqual([
      { name: 'ship', start: 19, end: 24 },
      { name: 'figma:figma-use', start: 32, end: 48 },
    ]);
    expect(mentionsIn('  /ship')).toEqual([{ name: 'ship', start: 2, end: 7 }]);
  });

  it('passes over a slash inside a word or a path', () => {
    expect(mentionsIn('/Users/me/file.txt is wrong')).toEqual([]);
    expect(mentionsIn('Read /review/notes.md and /review/')).toEqual([]);
    expect(mentionsIn('see src/app.ts and a/b')).toEqual([]);
    expect(mentionsIn('/ship\\it or /ship~ or /ship@2')).toEqual([]);
    expect(mentionsIn('')).toEqual([]);
    // Typing a path past its first part leaves no name being typed.
    expect(mentionAt('/review/', 8)).toBeNull();
  });

  it('ends a name at punctuation that ends a word in prose', () => {
    expect(mentionsIn('then /ship, and (/db) or /review!').map((m) => m.name)).toEqual(['ship', 'review']);
    expect(mentionsIn('then “run /ship” now').map((m) => m.name)).toEqual(['ship']);
    expect(mentionsIn('/ship; then /db?')).toEqual([
      { name: 'ship', start: 0, end: 5 },
      { name: 'db', start: 12, end: 15 },
    ]);
  });

  it('gives the one the caret is in or just after', () => {
    const text = 'then /sh and /db';
    expect(mentionAt(text, 8)?.name).toBe('sh');
    expect(mentionAt(text, 6)?.name).toBe('sh');
    expect(mentionAt(text, 5)).toBeNull();
    expect(mentionAt(text, 9)).toBeNull();
    expect(mentionAt(text, 16)?.name).toBe('db');
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

  it('writes each skill’s name as the harness takes it, wherever it is', () => {
    expect(writtenFor('/ship the fix', skills, 'claude')).toBe('/ship the fix');
    expect(writtenFor('/ship the fix', skills, 'codex')).toBe('$ship the fix');
    expect(writtenFor('/ship', skills, 'codex')).toBe('$ship');
    expect(writtenFor(' /ship\nand then rest', skills, 'codex')).toBe(' $ship\nand then rest');
    expect(writtenFor('fix it, then /ship and /probe it', skills, 'codex')).toBe('fix it, then $ship and $probe it');
  });

  it('leaves what isn’t one of the harness’s skills as typed', () => {
    expect(writtenFor('/compact', skills, 'codex')).toBe('/compact');
    expect(writtenFor('/probe it', skills, 'claude')).toBe('/probe it');
    expect(writtenFor('/shipping', skills, 'codex')).toBe('/shipping');
    expect(writtenFor('src/ship is a folder', skills, 'codex')).toBe('src/ship is a folder');
  });

  it('leaves a path whose first part names a skill as typed', () => {
    const withReview = [...skills, skill('review')];
    expect(writtenFor('Read /review/notes.md', withReview, 'codex')).toBe('Read /review/notes.md');
    expect(writtenFor('/ship/out.log then /review, please', withReview, 'codex')).toBe('/ship/out.log then $review, please');
    expect(skillNamed('Read /review/notes.md', withReview)).toBeUndefined();
  });

  it('names the first skill the text names', () => {
    expect(skillNamed('fix it, then /probe and /ship', skills)?.name).toBe('probe');
    expect(skillNamed('/compact then /ship', skills)?.name).toBe('ship');
    expect(skillNamed('/shi', skills)).toBeUndefined();
  });
});

describe('completed', () => {
  it('finishes the name and keeps what’s around it', () => {
    expect(completed('/sh', mentionAt('/sh', 3)!, skill('ship'))).toEqual({ text: '/ship ', caret: 6 });
    expect(completed('/sh the fix', mentionAt('/sh the fix', 3)!, skill('ship'))).toEqual({ text: '/ship the fix', caret: 6 });
    expect(completed('fix it then /sh now', mentionAt('fix it then /sh now', 15)!, skill('ship'))).toEqual({ text: 'fix it then /ship now', caret: 18 });
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

  it('offers the account’s skill when the worktree’s of that name is for another harness', () => {
    const listed: SkillsListed = {
      folders: [{ folder: '/r/app', skills: [skill('deploy', { written: { claude: '/deploy' } })] }],
      accounts: [{ account: 'codex', skills: [skill('deploy', { description: 'the account’s', written: { codex: '$deploy' } })] }],
    };
    expect(sessionSkills(listed, 'codex').map((s) => [s.name, s.description])).toEqual([['deploy', 'the account’s']]);
    expect(sessionSkills(listed, 'claude').map((s) => [s.name, s.description])).toEqual([['deploy', 'deploy things']]);
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
