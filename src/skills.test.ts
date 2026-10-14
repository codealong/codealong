import { describe, expect, it } from 'vitest';
import { completed, leftOutWords, matching, mentionAt, mentionsIn, namedBy, repoLanes, sessionSkills, skillLeftOutWords, skillNamed, skillOffers, skillRows, skillStart, skillWhere, startSkills, writtenFor } from './skills';
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

  it('leaves the full stop that ends a sentence out of the name, and in the text', () => {
    expect(writtenFor('Fix it, then /ship.', skills, 'codex')).toBe('Fix it, then $ship.');
    expect(writtenFor('Run /ship. Then wait.', skills, 'codex')).toBe('Run $ship. Then wait.');
    expect(writtenFor('Run /ship... then wait', skills, 'codex')).toBe('Run $ship... then wait');
    expect(skillNamed('Fix it, then /ship.', skills)?.name).toBe('ship');
    // Only for a skill: what names none stays as typed.
    expect(writtenFor('Clear /tmp.', skills, 'codex')).toBe('Clear /tmp.');
    expect(writtenFor('Then /probe.', skills, 'claude')).toBe('Then /probe.');
  });

  it('keeps a dot inside a name, and a name that ends in one, and passes over paths still', () => {
    const dotted = [...skills, skill('v1.2'), skill('ship.it'), skill('odd.'), skill('review')];
    expect(writtenFor('Run /v1.2 now, then /v1.2.', dotted, 'codex')).toBe('Run $v1.2 now, then $v1.2.');
    expect(writtenFor('/ship.it and /ship.', dotted, 'codex')).toBe('$ship.it and $ship.');
    // A name with the full stop that's a skill itself is that skill.
    expect(writtenFor('Try /odd.', dotted, 'codex')).toBe('Try $odd.');
    expect(writtenFor('Read /review/notes.md.', dotted, 'codex')).toBe('Read /review/notes.md.');
    expect(skillNamed('Read /review/notes.md.', dotted)).toBeUndefined();
  });
});

describe('namedBy', () => {
  const known = (name: string) => ['ship', 'v1.2'].includes(name);

  it('gives the mention as it names a skill, ending before a sentence’s full stop', () => {
    const [m] = mentionsIn('then /ship.');
    expect(m).toEqual({ name: 'ship.', start: 5, end: 11 });
    expect(namedBy(m, known)).toEqual({ name: 'ship', start: 5, end: 10 });
    expect(namedBy(mentionsIn('/v1.2.')[0], known)).toEqual({ name: 'v1.2', start: 0, end: 5 });
    expect(namedBy(mentionsIn('/v1.2')[0], known)).toEqual({ name: 'v1.2', start: 0, end: 5 });
    expect(namedBy(mentionsIn('/v1.')[0], known)).toBeNull();
    expect(namedBy(mentionsIn('/.')[0], known)).toBeNull();
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
    expect(rows.map((r) => `${skillWhere(r)}/${r.skill.name}`)).toEqual(['app/ship', 'app/shape', 'web/publish', 'web/ship']);
    expect(rows[0].marks).toEqual([[1, 3]]);
    // Ones kept off the menu come after the ones meant to be typed, however often they ran.
    expect(skillRows(listed, [main, webMain], '', false, 9).map((r) => r.skill.name)).toEqual(['ship', 'publish', 'shape', 'rules', 'ship']);
    expect(skillRows(listed, [main, webMain], '', false, 2)).toHaveLength(2);
    expect(skillRows(null, [main], 'sh', false, 9)).toEqual([]);
  });

  it('with the name finished, lists only the skills of that name', () => {
    expect(skillRows(listed, [main, webMain], 'ship', true, 9).map(skillWhere)).toEqual(['app', 'web']);
    expect(skillRows(listed, [main, webMain], 'shape', true, 9).map(skillWhere)).toEqual(['app']);
    expect(skillRows(listed, [main, webMain], 'sh', true, 9)).toEqual([]);
  });

  it('lists the accounts’ own once by name, with the accounts that have each', () => {
    const withAccounts: SkillsListed = {
      ...listed,
      accounts: [
        { account: 'claude', skills: [skill('design', { description: 'Make a design', written: { claude: '/design' } }), skill('ship', { uses: 1, written: { claude: '/ship' } })] },
        { account: 'claude2', skills: [skill('design', { description: 'the second’s', written: { claude: '/design' } })] },
        // Each account's are its own harness's.
        { account: 'codex', skills: [skill('shrink', { written: { codex: '$shrink' } })] },
        { account: 'claude3', skills: [skill('shrink', { written: { claude: '/shrink' } })] },
      ],
    };
    const rows = skillRows(withAccounts, [main, webMain], 'sh', false, 9);
    // A repo's comes before an account's of the same name; every other name's first before it.
    expect(rows.map((r) => `${skillWhere(r)}/${r.skill.name}`)).toEqual(['app/ship', 'app/shape', 'codex, claude3/shrink', 'web/publish', 'web/ship', 'claude/ship']);
    const design = skillRows(withAccounts, [main], 'design', true, 9);
    expect(design.map((r) => [r.lane, r.accounts, r.skill.description])).toEqual([[null, ['claude', 'claude2'], 'Make a design']]);
    // A start with it is offered only on the accounts that have it, whose agent reads it.
    const accounts = [
      { name: 'claude', harness: 'claude' as const },
      { name: 'claude2', harness: 'claude' as const },
      { name: 'codex', harness: 'codex' as const },
      { name: 'claude3', harness: 'claude' as const },
    ];
    expect(accounts.filter((a) => skillOffers(design[0], a)).map((a) => a.name)).toEqual(['claude', 'claude2']);
    expect(skillLeftOutWords(design[0], accounts)).toBe('Only claude and claude2 have /design, so codex and claude3 aren’t offered.');
    // One that accounts of two harnesses have is written for both.
    const shrink = skillRows(withAccounts, [main], 'shrink', true, 9)[0];
    expect(shrink.skill.written).toEqual({ codex: '$shrink', claude: '/shrink' });
    expect(accounts.filter((a) => skillOffers(shrink, a)).map((a) => a.name)).toEqual(['codex', 'claude3']);
    expect(skillLeftOutWords(shrink, accounts)).toBe('Only codex and claude3 have /shrink, so claude and claude2 aren’t offered.');
    // A repo's goes by where its agent reads skills, as before.
    expect(skillLeftOutWords(skillRows(withAccounts, [main], 'ship', true, 9)[0], accounts)).toBeNull();
  });

  it('writes a start’s prompt with the skills of the repo it starts in and of its account', () => {
    const withAccounts: SkillsListed = {
      folders: [{ folder: '/r/app', skills: [skill('probe', { written: { codex: '$probe' } })] }],
      accounts: [{ account: 'codex', skills: [skill('design', { written: { codex: '$design' } })] }],
    };
    const design = skillRows(withAccounts, [main], 'design', true, 9)[0];
    // An account's own picks no repo: the repo of where it starts goes by that repo's main checkout.
    const skills = startSkills(withAccounts, design, tree, 'codex', [main, webMain]);
    expect(writtenFor('/design the page, then /probe it', skills, 'codex')).toBe('$design the page, then $probe it');
    // Started in a repo whose skills weren't read, only the account's are known.
    expect(writtenFor('/design then /probe', startSkills(withAccounts, design, web, 'codex', [main, webMain]), 'codex')).toBe('$design then /probe');
    // A repo's row goes by its own repo.
    const probe = skillRows(withAccounts, [main], 'probe', true, 9)[0];
    expect(writtenFor('/probe and /design', startSkills(withAccounts, probe, main, 'codex', [main]), 'codex')).toBe('$probe and $design');
    // A repo's skill of the same name that only another harness reads doesn't hide the account's.
    const clash: SkillsListed = { ...withAccounts, folders: [{ folder: '/r/app', skills: [skill('design', { written: { claude: '/design' } }), skill('probe', { written: { codex: '$probe' } })] }] };
    const own = skillRows(clash, [], 'design', true, 9)[0];
    expect(writtenFor('/design the UI then /probe', startSkills(clash, own, tree, 'codex', [main]), 'codex')).toBe('$design the UI then $probe');
  });

  it('starts in a free reusable worktree of the repo, else a new one', () => {
    expect(skillStart(main, [main, tree, web, webMain])).toEqual({ lane: tree, newWorktree: false });
    expect(skillStart(webMain, [main, tree, web, webMain])).toEqual({ lane: webMain, newWorktree: true });
  });
});
