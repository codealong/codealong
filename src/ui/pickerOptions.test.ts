import { describe, expect, it } from 'vitest';
import { repoChoices } from '../quick';
import * as fx from '../testing/fixtures';
import type { Lane } from '../types';
import { NEW_WORKTREE, newWorktreeOption, REPO, repoSections, worktreeOption } from './pickerOptions';

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
