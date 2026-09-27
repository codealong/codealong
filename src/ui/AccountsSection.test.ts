import { describe, expect, it } from 'vitest';
import type { AccountSkill, FoundAccount } from '../types';
import { foundNote, skillTag, startPlaces } from './AccountsSection';

const NOW = Date.UTC(2026, 8, 25, 12);
const DAY = 24 * 60 * 60 * 1000;

function found(over: Partial<FoundAccount>): FoundAccount {
  return { name: 'claude-work', harness: 'claude', home: '~/.claude-work', command: null, via: null, state: 'active', signedIn: false, folderExists: true, email: null, lastUsed: null, new: true, ...over };
}

describe('foundNote', () => {
  it('says who is signed in and when it was used', () => {
    expect(foundNote(found({ signedIn: true, email: 'me@example.com', lastUsed: NOW - 3 * DAY }), NOW)).toBe('Signed in as me@example.com, used 3d ago');
    expect(foundNote(found({ lastUsed: NOW - 2 * 60 * 60 * 1000 }), NOW)).toBe('Not signed in, used 2h ago');
  });

  it('says a quiet sign-in has no sessions lately', () => {
    expect(foundNote(found({ state: 'signedIn', signedIn: true }), NOW)).toBe('Signed in, no sessions yet');
    expect(foundNote(found({ state: 'signedIn', signedIn: true, lastUsed: NOW - 60 * DAY }), NOW)).toBe('Signed in, last used 60d ago');
  });

  it('tells an installed harness from a folder that needs a sign-in', () => {
    expect(foundNote(found({ state: 'idle', folderExists: false }), NOW)).toBe('Installed, not set up yet');
    expect(foundNote(found({ state: 'idle' }), NOW)).toBe('Not signed in');
  });

  it('names your wrapper or alias', () => {
    expect(foundNote(found({ state: 'idle', command: 'cw', via: '~/.zshrc' }), NOW)).toBe('Not signed in · starts with cw from ~/.zshrc');
    // A command Codealong made up isn't yours, so it isn't named here.
    expect(foundNote(found({ state: 'idle', command: 'CLAUDE_CONFIG_DIR=~/.claude-work claude' }), NOW)).toBe('Not signed in');
  });
});

describe('skillTag', () => {
  const skill = (over: Partial<AccountSkill>): AccountSkill => ({ name: 'claude', harness: 'claude', path: '/Users/you/.claude/skills/codealong', installed: true, current: true, edited: false, ...over });

  it('shows only once the skill is there', () => {
    expect(skillTag(undefined)).toBeNull();
    expect(skillTag(skill({ installed: false, current: false }))).toBeNull();
    expect(skillTag(skill({}))).toEqual({ label: 'Skill installed', tone: 'ok' });
  });

  it('says when the copy is edited or behind', () => {
    expect(skillTag(skill({ edited: true }))?.label).toBe('Skill edited');
    expect(skillTag(skill({ edited: true, current: false }))?.label).toBe('Skill edited');
    expect(skillTag(skill({ current: false }))).toEqual({ label: 'Skill out of date', tone: 'attn' });
  });
});

describe('startPlaces', () => {
  it('offers each app only for the account in its default folder', () => {
    expect(startPlaces('claude', '~/.claude')).toEqual(['terminal', 'background', 'claude-app']);
    expect(startPlaces('claude', '/Users/me/.claude/')).toEqual(['terminal', 'background', 'claude-app']);
    expect(startPlaces('claude', '~/.claude2')).toEqual(['terminal', 'background']);
    expect(startPlaces('codex', '~/.codex')).toEqual(['codex-app']);
    expect(startPlaces('codex', '~/.codex2')).toEqual([]);
    expect(startPlaces('opencode', '~/.local/share/opencode')).toEqual(['terminal']);
  });
});
