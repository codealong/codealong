import { describe, expect, it } from 'vitest';
import { memberOrder, movedMember } from './SettingsDialog';

describe('a fallback group’s order', () => {
  // config.toml can list an account again on another model; the editor has a row per account.
  const members = [{ account: 'atlas', model: 'opus' }, { account: 'kestrel' }, { account: 'atlas', model: 'sonnet' }];

  it('lists each account once, where it first comes', () => {
    expect(memberOrder(members)).toEqual(['atlas', 'kestrel']);
  });

  it('moves an account with all its members, which keep their own order', () => {
    expect(movedMember(members, 'kestrel', -1)).toEqual([{ account: 'kestrel' }, { account: 'atlas', model: 'opus' }, { account: 'atlas', model: 'sonnet' }]);
    expect(movedMember(members, 'atlas', 1)).toEqual([{ account: 'kestrel' }, { account: 'atlas', model: 'opus' }, { account: 'atlas', model: 'sonnet' }]);
  });

  it('leaves them as they are at either end, or for an account not in the group', () => {
    expect(movedMember(members, 'atlas', -1)).toBe(members);
    expect(movedMember(members, 'kestrel', 1)).toBe(members);
    expect(movedMember(members, 'northwind', 1)).toBe(members);
  });
});
