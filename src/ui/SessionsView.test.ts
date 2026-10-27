import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import type { Session } from '../types';
import { helpersUnder, sessionSections, sortSessions, waitingQueue, type SessionItem, type SessionSection } from './SessionsView';

const item = (key: string, lastActivity: number, status: Session['status'] = 'idle', lastPromptAt = lastActivity, statusSince: number | null = null, startedAt: number | null = null): SessionItem => ({
  kind: 'agent',
  key: `lane::${key}`,
  lane: fx.lane(),
  session: fx.session({ key, lastActivity, status, lastPromptAt, statusSince, startedAt }),
});

const keys = (rows: SessionItem[]) => rows.map((r) => r.session.key);

describe('sortSessions by status', () => {
  it('orders the rest by local day, newest first', () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const rows = [item('old', at(10, 9)), item('today2', at(24, 14)), item('yday', at(23, 23)), item('today1', at(24, 1)), item('week', at(19, 12))];
    expect(keys(sortSessions(rows, 'status', now))).toEqual(['today2', 'today1', 'yday', 'week', 'old']);
  });

  it('keeps a day ahead of an older prompt: last active today comes before prompted yesterday', () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const rows = [item('yday', at(23, 20), 'idle', at(23, 20)), item('woke', at(24, 9), 'idle', at(20, 9))];
    expect(keys(sortSessions(rows, 'status', now))).toEqual(['woke', 'yday']);
  });

  it("puts what needs you, your turns and working agents first, in the bar's order", () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const rows = [
      item('idle', at(24, 14)),
      item('busy-old', at(24, 14), 'working', at(24, 9), at(24, 9)),
      item('busy-new', at(24, 12), 'working', at(24, 11), at(24, 11)),
      item('turn', at(20, 10), 'unread'),
      item('blocked', at(24, 12), 'needs', at(24, 11), at(24, 12)),
      item('failed', at(23, 8), 'error', at(23, 8), at(23, 8)),
      item('stopped', at(23, 9), 'stopped'),
    ];
    // Each the longest first.
    expect(keys(sortSessions(rows, 'status', now))).toEqual(['failed', 'blocked', 'turn', 'busy-old', 'busy-new', 'idle', 'stopped']);
  });

  it('keeps snoozed sessions after Working, the soonest back first, unless they run', () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const snoozed = (r: SessionItem, until: number): SessionItem => ({ ...r, session: { ...r.session, snoozedUntil: until } });
    const rows = [
      item('blocked', at(24, 12), 'needs', at(24, 11), at(24, 12)),
      snoozed(item('later', at(24, 12), 'needs', at(24, 11), at(24, 12)), at(24, 18)),
      snoozed(item('soon', at(24, 13), 'unread'), at(24, 16)),
      snoozed(item('quiet', at(20, 9)), at(24, 17)),
      snoozed(item('busy', at(24, 14), 'working', at(24, 14), at(24, 14)), at(24, 16)),
      item('idle', at(24, 14)),
    ];
    expect(keys(sortSessions(rows, 'status', now))).toEqual(['blocked', 'busy', 'soon', 'quiet', 'later', 'idle']);
  });
});

describe('sortSessions by start', () => {
  it('lists the newest start first, whatever the status, snoozed ones in place too', () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const late = item('snoozed', at(24, 12), 'unread', at(24, 12), null, at(23, 12));
    const rows = [
      item('old-busy', at(24, 14), 'working', at(24, 14), at(24, 14), at(20, 9)),
      item('new-idle', at(24, 13), 'idle', at(24, 13), null, at(24, 13)),
      item('blocked', at(24, 12), 'needs', at(24, 11), at(24, 12), at(22, 9)),
      { ...late, session: { ...late.session, snoozedUntil: at(24, 18) } },
    ];
    expect(keys(sortSessions(rows, 'started', now))).toEqual(['new-idle', 'snoozed', 'blocked', 'old-busy']);
  });

  it("goes by the last prompt where a session's start isn't known", () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const rows = [item('known', at(24, 14), 'idle', at(24, 14), null, at(22, 9)), item('unknown', at(24, 10), 'idle', at(23, 9))];
    expect(keys(sortSessions(rows, 'started', now))).toEqual(['unknown', 'known']);
  });
});

describe('sessionSections', () => {
  const now = new Date(2026, 8, 24, 15, 0).getTime();
  const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
  const heads = (sections: SessionSection[]) => sections.map((s) => `${s.label} ${keys(s.rows).join(',')}`);

  it('heads the status order as the other views do, leaving out headings with nothing under them', () => {
    const later = item('later', at(24, 12), 'needs', at(24, 11), at(24, 12));
    const rows = [
      item('old', at(10, 9)),
      item('busy', at(24, 14), 'working', at(24, 14), at(24, 14)),
      item('blocked', at(24, 12), 'needs', at(24, 11), at(24, 12)),
      { ...later, session: { ...later.session, snoozedUntil: at(24, 18) } },
      item('today', at(24, 14)),
    ];
    const sections = sessionSections(rows, 'status', now);
    expect(heads(sections)).toEqual(['Needs you blocked', 'Working busy', 'Snoozed later', 'Today today', 'Older old']);
    // What wants you or runs heads as a bay does; the rest as a section.
    expect(sections.map((s) => s.bay ?? s.icon ?? null)).toEqual(['needs', 'working', 'zzz', null, null]);
    expect(keys(sortSessions(rows, 'status', now))).toEqual(sections.flatMap((s) => keys(s.rows)));
  });

  it('heads the start order by the day each started, whatever its status', () => {
    const rows = [
      item('old-busy', at(24, 14), 'working', at(24, 14), at(24, 14), at(20, 9)),
      item('new-idle', at(24, 13), 'idle', at(24, 13), null, at(24, 13)),
      item('yday', at(24, 12), 'needs', at(24, 11), at(24, 12), at(23, 9)),
      item('long-ago', at(24, 10), 'idle', at(24, 10), null, at(2, 9)),
    ];
    expect(heads(sessionSections(rows, 'started', now))).toEqual(['Today new-idle', 'Yesterday yday', 'Last 7 days old-busy', 'Older long-ago']);
  });
});

describe('helpers in Sessions', () => {
  const now = new Date(2026, 8, 24, 15, 0).getTime();
  const at = (h: number) => new Date(2026, 8, 24, h, 0).getTime();
  const helper = (r: SessionItem, parent: string): SessionItem => ({ ...r, session: { ...r.session, helper: { parent } } });
  const rows = (s: SessionSection[]) => s.flatMap((x) => x.rows.map((r) => `${x.id} ${r.nested ? '  ' : ''}${r.session.key}`));

  it('lists each helper under the session it works for, in the engine’s order, where that session goes', () => {
    const list = [
      item('boss', at(13), 'idle', at(13)),
      helper(item('slow', at(14), 'working', at(10), at(10)), 'boss'),
      helper(item('quick', at(14), 'working', at(12), at(12)), 'boss'),
      item('other', at(12), 'working', at(11), at(11)),
    ];
    expect(rows(sessionSections(list, 'status', now))).toEqual(['working other', 'today boss', 'today   quick', 'today   slow']);
    // By start too, under the day the session started.
    expect(rows(sessionSections(list, 'started', now)).filter((r) => r.includes('boss') || r.includes('  '))).toEqual(['today boss', 'today   quick', 'today   slow']);
  });

  it('takes the session into Needs you with a helper that waits on you, by the helper’s wait', () => {
    const list = [item('blocked', at(12), 'needs', at(11), at(12)), item('boss', at(13), 'idle', at(13)), helper(item('asks', at(14), 'needs', at(10), at(11)), 'boss')];
    expect(rows(sessionSections(list, 'status', now))).toEqual(['needs boss', 'needs   asks', 'needs blocked']);
    // One that waits on you itself goes by its own wait.
    const waiting = [item('boss', at(13), 'needs', at(12), at(13)), helper(item('asks', at(14), 'needs', at(10), at(10)), 'boss'), item('blocked', at(12), 'needs', at(11), at(12))];
    expect(rows(sessionSections(waiting, 'status', now))).toEqual(['needs blocked', 'needs boss', 'needs   asks']);
  });

  it('lists a helper whose session isn’t listed, and a helper’s helper under the session at the top', () => {
    const list = [helper(item('alone', at(14), 'needs', at(10), at(11)), 'gone'), item('boss', at(13), 'idle', at(13)), helper(item('kid', at(14), 'working', at(12), at(12)), 'boss'), helper(item('grandkid', at(14), 'needs', at(12), at(12)), 'kid')];
    expect(rows(sessionSections(list, 'status', now))).toEqual(['needs alone', 'needs boss', 'needs   grandkid', 'needs   kid']);
  });

  it('says which helpers work in another worktree than their session’s, which their rows name', () => {
    const elsewhere = helper(item('there', at(14), 'working', at(12), at(12)), 'boss');
    const list = [item('boss', at(13), 'idle', at(13)), helper(item('here', at(14), 'working', at(11), at(11)), 'boss'), { ...elsewhere, lane: fx.lane({ id: 'other' }) }];
    const under = sessionSections(list, 'status', now).flatMap((s) => s.rows).filter((r) => r.nested);
    expect(under.map((r) => `${r.session.key} ${r.elsewhere ? 'elsewhere' : 'here'}`)).toEqual(['there elsewhere', 'here here']);
  });
});

describe('sessions another one started, in Sessions', () => {
  const now = new Date(2026, 8, 24, 15, 0).getTime();
  const at = (h: number) => new Date(2026, 8, 24, h, 0).getTime();
  const from = (r: SessionItem, key: string, how: 'started' | 'fork' = 'started'): SessionItem => ({ ...r, session: { ...r.session, origin: { key, how } } });
  const helper = (r: SessionItem, parent: string): SessionItem => ({ ...r, session: { ...r.session, helper: { parent } } });
  const rows = (s: SessionSection[]) => s.flatMap((x) => x.rows.map((r) => `${x.id} ${r.nested ? '  ' : ''}${r.session.key}`));

  it('lists one under the session that started it only when asked to, and a fork on its own', () => {
    const list = [item('boss', at(13), 'idle', at(13)), from(item('kid', at(12), 'idle', at(12)), 'boss'), from(item('copy', at(11), 'idle', at(11)), 'boss', 'fork')];
    expect(rows(sessionSections(list, 'status', now))).toEqual(['today boss', 'today kid', 'today copy']);
    expect(rows(sessionSections(list, 'status', now, 'under'))).toEqual(['today boss', 'today   kid', 'today copy']);
    expect(rows(sessionSections(list, 'started', now, 'under'))).toEqual(['today boss', 'today   kid', 'today copy']);
  });

  it('takes its session as far up as it is itself, where a helper at work wouldn’t', () => {
    const boss = item('boss', at(10), 'idle', at(10));
    const other = item('other', at(13), 'working', at(11), at(11));
    const working = from(item('kid', at(14), 'working', at(12), at(12)), 'boss');
    expect(rows(sessionSections([boss, working, other], 'status', now, 'under'))).toEqual(['working other', 'working boss', 'working   kid']);
    const turn = from(item('kid', at(14), 'unread', at(12), at(14)), 'boss');
    expect(rows(sessionSections([boss, turn, other], 'status', now, 'under'))).toEqual(['unread boss', 'unread   kid', 'working other']);
    const asks = from(item('kid', at(14), 'needs', at(12), at(14)), 'boss');
    expect(rows(sessionSections([boss, asks, other], 'status', now, 'under'))).toEqual(['needs boss', 'needs   kid', 'working other']);
    // A helper of it at work stays with its session, as it does under a session of yours.
    const check = helper(item('check', at(14), 'working', at(12), at(12)), 'kid');
    const idleKid = from(item('kid', at(11), 'idle', at(11)), 'boss');
    expect(rows(sessionSections([boss, idleKid, check, other], 'status', now, 'under'))).toEqual(['working other', 'today boss', 'today   check', 'today   kid']);
  });

  it('lists one on its own when its session isn’t listed, and sessions that name each other both', () => {
    expect(rows(sessionSections([from(item('kid', at(12), 'idle', at(12)), 'gone')], 'status', now, 'under'))).toEqual(['today kid']);
    const loop = [from(item('a', at(13), 'idle', at(13)), 'b'), from(item('b', at(12), 'idle', at(12)), 'a')];
    expect(rows(sessionSections(loop, 'status', now, 'under'))).toEqual(['today a', 'today b']);
  });

  it('lists the sessions a pinned session started under it, with their helpers, when asked to', () => {
    const list = [item('boss', at(13), 'idle', at(13)), from(item('kid', at(12), 'idle', at(12)), 'boss'), helper(item('check', at(14), 'working', at(11), at(11)), 'kid')];
    expect(keys(helpersUnder(list, new Set(['boss']), 'under').get('boss') ?? [])).toEqual(['check', 'kid']);
    expect(helpersUnder(list, new Set(['boss'])).has('boss')).toBe(false);
  });
});

describe('waitingQueue', () => {
  it('lists what waits on you in the status order, pinned ones too, and leaves snoozed ones to wait', () => {
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const rows = [
      item('idle', at(24, 14)),
      item('busy', at(24, 12), 'working', at(24, 11), at(24, 11)),
      item('turn-old', at(24, 9), 'unread', at(24, 9), at(24, 9)),
      item('turn-new', at(24, 13), 'unread', at(24, 13), at(24, 13)),
      item('blocked', at(24, 12), 'needs', at(24, 11), at(24, 12)),
      item('failed', at(23, 8), 'error', at(23, 8), at(23, 8)),
    ];
    const later = item('later', at(24, 12), 'needs', at(24, 11), at(24, 10));
    rows.push({ ...later, session: { ...later.session, snoozedUntil: at(24, 18) } });
    expect(waitingQueue(rows).map((r) => r.session.key)).toEqual(['failed', 'blocked', 'turn-old', 'turn-new']);
  });
});
