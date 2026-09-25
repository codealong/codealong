// Words for Jev's read of a session (insights.rs): how far its work got, and how risky the
// command it waits on looks. Kept free of React so the rules have tests.

import type { Session, SessionInsight } from './types';

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

/** How far the work got, from the end of the last reply. None for no code change. */
export function landingLabel(i: SessionInsight | undefined): InsightLabel | null {
  const l = i?.landing && LANDING[i.landing];
  if (!l) return null;
  return { label: l.label, tone: l.tone, title: `Jev’s read of the last reply: ${l.says}.` };
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
  return { label: level.label, tone: level.tone, title: `Jev’s read of the command waiting for your approval: it ${level.says}. Check it before you approve.` };
}
