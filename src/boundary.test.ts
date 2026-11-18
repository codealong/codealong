import { describe, expect, it } from 'vitest';

import { outsideMembers } from './boundary';
import type { RoutingGroupCfg, SignIn } from './types';

const group: RoutingGroupCfg = { name: 'authorized', strategy: 'usage', boundary: true, members: [{ account: 'atlas-work' }, { account: 'kestrel' }, { account: 'northwind' }] };
const signIn = (account: string, plan: string, commercial: boolean, verified: boolean, note?: string): SignIn => ({ account, email: null, organization: null, organizationName: null, plan, commercial, verified, asOf: 1, note });

describe('outsideMembers', () => {
  it('names each member whose sign-in doesn’t let it in, and why', () => {
    const out = outsideMembers(group, [signIn('atlas-work', 'team', true, true), signIn('kestrel', 'max', false, true), signIn('northwind', 'enterprise', true, false, 'From its files only: offline.')]);
    expect(out.map((o) => o.account)).toEqual(['kestrel', 'northwind']);
    expect(out[0].why).toBe('kestrel is signed in on a max plan, which isn’t under its provider’s commercial terms, so it’s outside authorized.');
    expect(out[1].why).toContain('hasn’t confirmed it (From its files only: offline)');
  });

  it('says nothing of a group that isn’t a boundary, or a sign-in not read yet', () => {
    expect(outsideMembers({ ...group, boundary: false }, [signIn('kestrel', 'max', false, true)])).toEqual([]);
    expect(outsideMembers(group, [])).toEqual([]);
  });
});
