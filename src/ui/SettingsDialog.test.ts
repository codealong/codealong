import { describe, expect, it } from 'vitest';
import { movedMember } from './SettingsDialog';

describe('a fallback group’s order', () => {
  // An account can be in a group more than once, on different models.
  const members = [{ account: 'atlas', model: 'opus' }, { account: 'kestrel' }, { account: 'atlas', model: 'sonnet', effort: 'high' }];

  it('moves a member a place sooner or later, the rest keeping theirs', () => {
    expect(movedMember(members, 1, -1)).toEqual([{ account: 'kestrel' }, { account: 'atlas', model: 'opus' }, { account: 'atlas', model: 'sonnet', effort: 'high' }]);
    expect(movedMember(members, 1, 1)).toEqual([{ account: 'atlas', model: 'opus' }, { account: 'atlas', model: 'sonnet', effort: 'high' }, { account: 'kestrel' }]);
  });

  it('leaves them as they are at either end, or for a place past them', () => {
    expect(movedMember(members, 0, -1)).toBe(members);
    expect(movedMember(members, 2, 1)).toBe(members);
    expect(movedMember(members, 5, -1)).toBe(members);
  });
});
