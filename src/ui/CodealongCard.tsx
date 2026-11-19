// A message Codealong sent a session by itself, in its conversation (SessionThread.tsx): a line
// saying what it's about, the PR it names as a link, and what happened, in a few words a part,
// each helper it names as a link too (glance.rs, from the engine), which a click opens to the
// whole message. The whole message is written for the agent, with session keys, commands to run
// and what not to do, and read as clutter over every one. While ⌘F finds something in the whole
// message it shows open, as another session's cut-off message does, so the find can show it: the
// whole message is drawn closed too, hidden, so whether the find is in it goes by the words it
// shows (a PR's address shows as `app#7`), as the find reads them once it's open.

import { Fragment, useContext, useRef, useState, type MouseEvent, type ReactNode } from 'react';
import { absolute, ago } from '../format';
import { prRefOf } from '../prLinks';
import { shownText } from '../threadFind';
import type { Glance, GlanceAbout, TranscriptItem } from '../types';
import { says } from '../windowFind';
import { Button, cx, Icon } from './primitives';
import { PrLink } from './PrLink';
import { SessionLink } from './SessionLink';
import { wordsIn } from './ThreadFind';
import { FindContext } from './WindowFind';

/** Each kind's icon, as the views draw a PR, a helper, a schedule, a review and carrying on. */
const ICON: Record<GlanceAbout, string> = { pr: 'pr', helper: 'robot', schedule: 'calendar', review: 'eye', 'carry-on': 'player-play', other: 'message' };

/** `children`: the whole message, as the conversation draws it, shown once the card is opened. */
export function CodealongCard({ item, glance: g, now, children }: { item: TranscriptItem; glance: Glance; now: number; children: ReactNode }) {
  const [opened, setOpened] = useState(false);
  const finding = useContext(FindContext);
  // Whether the find is in it, by the words its whole message shows as the find reads them, drawn
  // hidden since its first render: read as the find's pattern comes, so the render that brings it
  // opens the card before the find looks, which passes over what's hidden. Its first render, with
  // nothing drawn yet, goes by its text.
  const whole = useRef<HTMLDivElement>(null);
  const found = !!finding && says(finding, whole.current ? wordsIn(whole.current).text : shownText(item));
  const open = opened || found;
  const pr = g.pr;
  const ref = pr?.url ? prRefOf(pr.url) : null;
  const named = pr && `#${pr.number} ${pr.title}`;
  // A click anywhere on the line opens it, but on its links, which go where they say, and while
  // the find holds it open, which a click would leave open once the find closes.
  const toggle = (e: MouseEvent) => {
    if (found || (e.target as Element).closest('a, button')) return;
    setOpened(!opened);
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
          {pr && g.parts.length > 0 && ' '}
          {g.parts.map((p, i) => (
            <Fragment key={i}>
              {i > 0 && ' · '}
              {p.session && (
                <>
                  <SessionLink name={p.session.key} unknown={<span className="ca-codealong-named">“{p.session.title}”</span>} />{' '}
                </>
              )}
              <span className="ca-codealong-line">{p.line}</span>
            </Fragment>
          ))}
        </span>
        {item.at > 0 && (
          <span className="ca-meta" title={absolute(item.at)}>
            {ago(item.at, now)}
          </span>
        )}
        {/* While the find is in it, it stays open: closing it would hide what the find shows. */}
        <Button
          variant="ghost"
          size="sm"
          icon={open ? 'chevron-up' : 'chevron-down'}
          className="ca-codealong-open"
          aria-expanded={open}
          aria-label={open ? 'Hide the whole message' : 'Show the whole message'}
          title={found ? 'It stays open while you find in it' : open ? 'Hide the whole message' : 'Show the whole message, as the session got it'}
          disabled={found}
          onClick={() => setOpened(!opened)}
        />
      </div>
      <div ref={whole} className="ca-codealong-whole" hidden={!open}>
        {children}
      </div>
    </div>
  );
}
