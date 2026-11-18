// A boundary group's members whose sign-in doesn't let them in (boundary.rs, signin.rs), in
// words for Settings › Routing: a member counts as in the group only while its provider says its
// sign-in is on a commercial plan.

import type { RoutingGroupCfg, SignIn } from './types';

/** Why each member of a boundary group is outside it now, in the group's order; none for a
 *  group that isn't a boundary, or a member whose sign-in hasn't been read yet. */
export function outsideMembers(group: RoutingGroupCfg, signIns: SignIn[]): { account: string; why: string }[] {
  if (!group.boundary) return [];
  return (group.members ?? []).flatMap((m) => {
    const s = signIns.find((x) => x.account === m.account);
    if (!s || (s.commercial && s.verified)) return [];
    const plan = s.plan ? `a ${s.plan} plan` : 'a plan Codealong can’t read';
    const why = !s.commercial
      ? `${m.account} is signed in on ${plan}, which isn’t under its provider’s commercial terms, so it’s outside ${group.name}.`
      : `${m.account}’s files say it’s on ${plan}, but its provider hasn’t confirmed it${s.note ? ` (${s.note.replace(/\.$/, '')})` : ''}, so it’s outside ${group.name} for now.`;
    return [{ account: m.account, why }];
  });
}
