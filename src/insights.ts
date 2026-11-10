// Words for the read of a session (insights.rs): how far its work got, how its last reply leaves
// things with you, and how risky the command it waits on looks, each saying who read it: Jev, or
// the agent that tagged it (`codealong session tag`); and that it's finished, as the engine
// works that out from the read and what it knows of the session's work (board.rs `finished_of`).
// Kept free of React so the rules have tests.

import type { Ending, Session, SessionInsight } from './types';

export type Tone = 'good' | 'info' | 'warn' | 'bad' | 'quiet';

export interface InsightLabel {
  label: string;
  tone: Tone;
  title: string;
}

const LANDING: Record<string, { label: string; tone: Tone; says: string }> = {
  landed: { label: 'Landed', tone: 'good', says: 'the change is merged or on the main branch' },
  pushed: { label: 'Pushed', tone: 'info', says: 'the change is pushed to a branch that isn’t merged yet' },
  committed: { label: 'Committed', tone: 'warn', says: 'the change is committed but not pushed' },
  uncommitted: { label: 'Not committed', tone: 'warn', says: 'files changed but nothing is committed' },
  blocked: { label: 'Blocked', tone: 'bad', says: 'the agent couldn’t finish' },
};

/** Who read the session, for a tooltip: the agent that tagged it, else Jev. */
export function readBy(i: SessionInsight | undefined): string {
  if (!i?.by) return 'Jev’s read';
  return `Tagged by ${i.byTitle || 'an agent'}`;
}

/** What the agent that tagged it said, for the end of a tooltip; nothing for Jev's read. */
function noteWords(i: SessionInsight | undefined): string {
  return i?.by && i.note ? ` ${i.note.replace(/[.!?]?$/, '.')}` : '';
}

/** How far the work got, from the end of the last reply. None for no code change. */
export function landingLabel(i: SessionInsight | undefined): InsightLabel | null {
  const l = i?.landing && LANDING[i.landing];
  if (!l) return null;
  return { label: l.label, tone: l.tone, title: `${readBy(i)} of the last reply: ${l.says}.${noteWords(i)}` };
}

const RISK: { label: string; tone: Tone; says: string }[] = [
  { label: 'Reads only', tone: 'quiet', says: 'only reads or inspects' },
  { label: 'Local changes', tone: 'info', says: 'changes files or state on this Mac' },
  { label: 'Remote effects', tone: 'warn', says: 'pushes, deploys, publishes, sends or changes shared things' },
  { label: 'Destructive', tone: 'bad', says: 'deletes data or is hard to undo' },
];

/** How risky the command a session waits on you for looks. Only while it waits. */
export function riskLabel(s: Pick<Session, 'status' | 'insight'>): InsightLabel | null {
  const r = s.insight?.risk;
  if (s.status !== 'needs' || r == null) return null;
  const level = RISK[Math.max(0, Math.min(3, Math.round(r)))];
  return { label: level.label, tone: level.tone, title: `${readBy(s.insight)} of the command waiting for your approval: it ${level.says}. Check it before you approve.${noteWords(s.insight)}` };
}

/**
 * That a session is finished (board.rs `finished_of`), with why: Finished when something besides
 * its last reply says so (a PR of its merged, a todo of its done, its own status line), Looks
 * finished when only the read of that reply does. Shown in place of how far its work got, which
 * is among its reasons. Only while its turn is over: one sent a prompt from a window is working
 * before the engine says so (ahead.ts), and no longer finished.
 */
export function finishedLabel(s: Pick<Session, 'status' | 'finished'>): InsightLabel | null {
  const f = s.finished;
  if (!f || !(s.status === 'unread' || s.status === 'idle' || s.status === 'stopped')) return null;
  const why = f.why.join(', ');
  if (f.sure) return { label: 'Finished', tone: 'good', title: `Its work is done and nothing is left for you: ${why}. Mark it as done once you’ve read it.` };
  return { label: 'Looks finished', tone: 'quiet', title: `Nothing seems left for you: ${why}. Nothing else says so, so read its reply before you mark it as done.` };
}

const ENDING: Partial<Record<Ending, { label: string; tone: Tone; says: string }>> = {
  handoff: { label: 'Next step is yours', tone: 'info', says: 'its work is done, and it leaves you a next step to take yourself' },
  waiting: { label: 'Waits on its own work', tone: 'quiet', says: 'it isn’t done, and goes on by itself once what it started ends' },
  offer: { label: 'Offers more', tone: 'quiet', says: 'its work is done, and it offers more if you want it' },
};

/**
 * How a turn that's yours to read leaves things with you, where that says more than Your turn
 * does: it hands you a next step, waits on what it started, or offers more. None for one that
 * asks you something, which is Needs you and says so (lanes.ts `sessionFlag`), for one that only
 * reports, and for a finished one, whose own label says more. Only while it's Your turn: once
 * you've seen it, you know.
 */
export function endingLabel(s: Pick<Session, 'status' | 'insight' | 'finished'>): InsightLabel | null {
  const l = s.insight?.ending && ENDING[s.insight.ending];
  if (!l || s.status !== 'unread' || s.finished) return null;
  return { label: l.label, tone: l.tone, title: `${readBy(s.insight)} of the last reply: ${l.says}.${noteWords(s.insight)}` };
}
