import { describe, expect, it } from 'vitest';
import type { AccountSkill } from '../types';
import { skillAction, skillState } from './AgentsSection';

function skill(over: Partial<AccountSkill>): AccountSkill {
  return { name: 'claude', harness: 'claude', path: '/Users/you/.claude/skills/codealong', installed: true, current: true, edited: false, ...over };
}

describe('skill rows', () => {
  it('says where a copy stands', () => {
    expect(skillState(skill({})).label).toBe('Installed');
    expect(skillState(skill({ current: false }))).toEqual({ label: 'Update available', tone: 'attn' });
    expect(skillState(skill({ installed: false, current: false })).label).toBe('Not installed');
    expect(skillState(skill({ path: null, installed: false, current: false })).label).toBe('No skills folder known');
  });

  it('leaves an edited copy alone until you replace it', () => {
    expect(skillState(skill({ edited: true }))).toEqual({ label: 'Edited', tone: 'none' });
    expect(skillState(skill({ edited: true, current: false }))).toEqual({ label: 'Edited, update available', tone: 'none' });
    expect(skillAction(skill({ edited: true }))).toBe('Restore');
    expect(skillAction(skill({ edited: true, current: false }))).toBe('Update anyway');
    expect(skillAction(skill({ current: false }))).toBe('Update');
  });
});
