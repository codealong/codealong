// Chips for Jev's read of a session and a worktree (insights.ts): how far the work got, how risky
// a waiting command looks, and a worktree that looks finished.

import { landingLabel, riskLabel, type InsightLabel } from '../insights';
import type { Lane, Session } from '../types';

function Chip({ l }: { l: InsightLabel }) {
  return (
    <span className={`ca-chip ca-insight is-${l.tone}`} title={l.title}>
      {l.label}
    </span>
  );
}

/** The session's risk while it waits on you, else how far its work got. */
export function InsightChips({ session }: { session: Session }) {
  const risk = riskLabel(session);
  const landing = risk ? null : landingLabel(session.insight);
  const l = risk ?? landing;
  return l ? <Chip l={l} /> : null;
}

export function LooksFinished({ lane }: { lane: Lane }) {
  if (!lane.looksFinished) return null;
  return <Chip l={{ label: 'Looks finished', tone: 'quiet', title: 'Jev reads every session here as done, and nothing is running. It may be ready to clean up.' }} />;
}
