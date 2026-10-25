// The end of what waits on you, where the notch and the waiting window work through it one at a
// time (NotchSession.tsx, Waiting.tsx), as the iPhone's end page has it (EndView.swift). Opened
// with nothing waiting it says so; gone on from the last one it says nothing else waits, with how
// that one went, its check drawing in. What comes to wait on you next takes its place by itself
// (useWaitingQueue.ts `first`), so it says that too.

import { cx, Icon } from './primitives';

export function QueueEnd({ said, working, className }: {
  /** How the last one went, when dealing with it is what ended what waits (useWaitingQueue.ts
   *  `ended`); null when it opened on nothing. */
  said: string | null;
  /** How many sessions are at work, where nothing beside it says so. */
  working?: number;
  className?: string;
}) {
  const atWork = working === undefined || working === 0 ? null : working === 1 ? '1 session is working.' : `${working} sessions are working.`;
  return (
    <div className={cx('ca-queue-end', said !== null && 'is-through', className)} role="status">
      <Icon name="circle-check" size={28} className="ca-queue-end-mark" />
      <h1 className="ca-queue-end-title">{said !== null ? 'Nothing else waits on you' : 'Nothing waits on you'}</h1>
      {said !== null && <p className="ca-queue-end-said">{said}</p>}
      <p className="ca-queue-end-hint">{[atWork, 'What waits on you next shows up here as it comes.'].filter(Boolean).join(' ')}</p>
    </div>
  );
}
