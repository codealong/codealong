// Label chips for a session and a worktree: what a session waits on you for (lanes.ts), and the
// read of them (insights.ts), Jev's or an agent's tag: how far the work got, how risky a waiting
// command looks, and a worktree that looks finished.

import { landingLabel, riskLabel, type InsightLabel } from '../insights';
import { sessionFlag } from '../lanes';
import type { Lane, Session } from '../types';
import { Chip } from './primitives';

function Insight({ l }: { l: InsightLabel }) {
  return (
    <Chip className={`ca-insight is-${l.tone}`} title={l.title}>
      {l.label}
    </Chip>
  );
}

/** What the session waits on you for: asks you, needs approval, stuck, or an error. */
export function FlagChip({ session }: { session: Session }) {
  const l = sessionFlag(session);
  return l ? <Insight l={l} /> : null;
}

export function ProviderFlagChip({ session }: { session: Session }) {
  return session.flagged ? <Insight l={{ label: 'Flagged', tone: 'bad', title: 'The provider flagged a message in this conversation. Automatic messages, resumes and account moves are blocked by default in Settings › Automation. You can still reply yourself.' }} /> : null;
}

/** The session's risk while it waits on you, else how far its work got. Not after a lost
 *  connection ended its turn: Jev read Claude Code's error, which it takes for being blocked. */
export function InsightChips({ session }: { session: Session }) {
  const risk = riskLabel(session);
  const landing = risk || (session.connectionLost && session.status !== 'working') ? null : landingLabel(session.insight);
  const l = risk ?? landing;
  return <><ProviderFlagChip session={session} />{l && <Insight l={l} />}</>;
}

export function LooksFinished({ lane }: { lane: Lane }) {
  if (!lane.looksFinished) return null;
  return <Insight l={{ label: 'Looks finished', tone: 'quiet', title: 'Every session here reads as done (by Jev, or the agent that tagged it), and nothing is running. It may be ready to clean up.' }} />;
}
