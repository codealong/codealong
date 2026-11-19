// A message Codealong sent a session by itself, in its conversation (SessionThread.tsx): a line
// saying what it's about, the PR or session it names as a link, and what happened, in a few
// words (glance.rs, from the engine), which a click opens to the whole message. The whole
// message is written for the agent, with session keys, commands to run and what not to do, and
// read as clutter over every one.

import { useState, type MouseEvent, type ReactNode } from 'react';
import { absolute, ago } from '../format';
import { prRefOf } from '../prLinks';
import type { Glance, GlanceAbout } from '../types';
import { Button, cx, Icon } from './primitives';
import { PrLink } from './PrLink';
import { SessionLink } from './SessionLink';

/** Each kind's icon, as the views draw a PR, a helper, a schedule, a review and carrying on. */
const ICON: Record<GlanceAbout, string> = { pr: 'pr', helper: 'robot', schedule: 'calendar', review: 'eye', 'carry-on': 'player-play', other: 'message' };

/** `children`: the whole message, as the conversation draws it, shown once the card is opened. */
export function CodealongCard({ glance: g, at, now, children }: { glance: Glance; at: number; now: number; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const pr = g.pr;
  const ref = pr?.url ? prRefOf(pr.url) : null;
  const named = pr && `#${pr.number} ${pr.title}`;
  // A click anywhere on the line opens it, but on its links, which go where they say.
  const toggle = (e: MouseEvent) => {
    if ((e.target as Element).closest('a, button')) return;
    setOpen(!open);
  };
  return (
    <div className={cx('ca-codealong-card', open && 'is-open')}>
      <div className="ca-codealong-head" onClick={toggle}>
        <span className="ca-codealong-icon" title="Codealong sent this by itself" role="img" aria-label="From Codealong">
          <Icon name={ICON[g.about] ?? ICON.other} size={13} />
        </span>
        <span className="ca-codealong-glance">
          {pr && ref && pr.url && (
            <PrLink url={pr.url} pr={ref}>
              {named}
            </PrLink>
          )}
          {pr && !ref && <span className="ca-codealong-named">{named}</span>}
          {g.session && <SessionLink name={g.session.key} unknown={<span className="ca-codealong-named">“{g.session.title}”</span>} />}
          {(pr || g.session) && g.line && ' '}
          <span className="ca-codealong-line">{g.line}</span>
        </span>
        {at > 0 && (
          <span className="ca-meta" title={absolute(at)}>
            {ago(at, now)}
          </span>
        )}
        <Button
          variant="ghost"
          size="sm"
          icon={open ? 'chevron-up' : 'chevron-down'}
          className="ca-codealong-open"
          aria-expanded={open}
          aria-label={open ? 'Hide the whole message' : 'Show the whole message'}
          title={open ? 'Hide the whole message' : 'Show the whole message, as the session got it'}
          onClick={() => setOpen(!open)}
        />
      </div>
      {open && <div className="ca-codealong-whole">{children}</div>}
    </div>
  );
}
