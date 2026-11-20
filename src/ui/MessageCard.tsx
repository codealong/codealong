// A message in a session's conversation that's neither yours nor its agent's (SessionThread.tsx):
// one Codealong sent by itself, or another session's (its side conversation's among them). Each is
// a quiet card, a line at its head saying what it is and who it's from, the sender a link, and
// when, then the message:
// - Codealong's is its line alone, what the message comes to in a few words (glance.rs, from the
//   engine), the PR and each helper it names a link, until a click opens it to the whole message,
//   which is written for the agent: session keys, commands to run, what not to do.
// - Another session's shows its first lines under its line, a long one cut until it's opened.
// While ⌘F finds something in the whole message it shows open, so the find can show it. Whether
// it does goes by the words the message shows as the find reads them (ThreadFind.tsx `wordsIn`),
// drawn since the card's first render (hidden or cut) and read in the render that brings the
// find's pattern, so the card opens before the find looks, which passes over what's hidden or cut.

import { Fragment, useContext, useLayoutEffect, useRef, useState, type MouseEvent, type ReactNode } from 'react';
import { absolute, ago } from '../format';
import { prRefOf } from '../prLinks';
import { showSide, showSides, sideForMessage, useKnownSides } from '../side';
import { shownText } from '../threadFind';
import type { Glance, GlanceAbout, TranscriptItem } from '../types';
import { says } from '../windowFind';
import { Button, cx, Icon } from './primitives';
import { PrLink } from './PrLink';
import { SessionLink } from './SessionLink';
import { wordsIn } from './ThreadFind';
import { FindContext } from './WindowFind';

/** What a card shows closed: its line alone (Codealong's), or the message's first lines too. */
type Closed = 'line' | 'start';

/** The card: `title` after `icon` at its head, then when (`at`), then the button that opens it
 *  where there's more to show; `children`, the whole message as the conversation draws it, under
 *  it. A right-click on its line is one on the message: its menu (Copy message), but on a link,
 *  which has its own. */
function MessageCard({ item, at, now, icon, says: iconSays, title, closed, children }: { item: TranscriptItem; at: number; now: number; icon: string; says: string; title: ReactNode; closed: Closed; children: ReactNode }) {
  const [opened, setOpened] = useState(false);
  const finding = useContext(FindContext);
  // Its first render, with nothing drawn yet, goes by its text.
  const body = useRef<HTMLDivElement>(null);
  const found = !!finding && says(finding, body.current ? wordsIn(body.current).text : shownText(item));
  const open = opened || found;
  // Whether there's more than its first lines, measured while it's cut, as the panel's width lays
  // it out: its width, and its text's height as images in it come in.
  const [long, setLong] = useState(false);
  useLayoutEffect(() => {
    const el = body.current;
    if (closed !== 'start' || !el || open) return;
    const measure = () => setLong(el.scrollHeight > el.clientHeight + 1);
    measure();
    const watch = new ResizeObserver(measure);
    watch.observe(el);
    if (el.firstElementChild) watch.observe(el.firstElementChild);
    return () => watch.disconnect();
  }, [closed, open, item.text]);
  const opens = closed === 'line' || long;
  const cut = closed === 'start' && !open;
  // A click anywhere on the line opens it, but on its links, which go where they say, and while
  // the find holds it open, which a click would leave open once the find closes.
  const toggle = (e: MouseEvent) => {
    if (!opens || found || (e.target as Element).closest('a, button')) return;
    setOpened(!opened);
  };
  // The message's own handler builds its menu (SessionThread.tsx `Said`), hidden or cut as it is.
  const menu = (e: MouseEvent) => {
    const said = body.current?.querySelector(':scope > .ca-thread-said');
    if (e.defaultPrevented || !said) return;
    e.preventDefault();
    said.dispatchEvent(new window.MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: e.clientX, clientY: e.clientY }));
  };
  return (
    <div className={cx('ca-msgcard', open && 'is-open')}>
      <div className={cx('ca-msgcard-head', opens && 'is-opens')} onClick={toggle} onContextMenu={menu}>
        <span className="ca-msgcard-icon" title={iconSays} role="img" aria-label={iconSays}>
          <Icon name={icon} size={13} />
        </span>
        <span className="ca-msgcard-title">{title}</span>
        {at > 0 && (
          <span className="ca-meta" title={absolute(at)}>
            {ago(at, now)}
          </span>
        )}
        {/* While the find is in it, it stays open: closing it would hide what the find shows. */}
        {opens && (
          <Button
            variant="ghost"
            size="sm"
            icon={open ? 'chevron-up' : 'chevron-down'}
            className="ca-msgcard-open"
            aria-expanded={open}
            aria-label={open ? 'Show less' : 'Show the whole message'}
            title={found ? 'It stays open while you find in it' : open ? 'Show less' : 'Show the whole message, as the session got it'}
            disabled={found}
            onClick={() => setOpened(!opened)}
          />
        )}
      </div>
      <div ref={body} className={cx('ca-msgcard-body', cut && 'is-cut', cut && long && 'is-long')} hidden={closed === 'line' && !open}>
        {children}
      </div>
    </div>
  );
}

/** Each kind's icon, as the views draw a PR, a helper, a schedule, a review and carrying on. */
const ICON: Record<GlanceAbout, string> = { pr: 'pr', helper: 'robot', schedule: 'calendar', review: 'eye', 'carry-on': 'player-play', other: 'message' };

/** What Codealong sent by itself: what it comes to in a line, where the engine said (glance.rs),
 *  which opens to the whole; one an older engine sent, without that line, by its first lines. */
export function CodealongCard({ item, at, glance: g, now, children }: { item: TranscriptItem; at: number; glance?: Glance; now: number; children: ReactNode }) {
  return (
    <MessageCard item={item} at={at} now={now} icon={g ? (ICON[g.about] ?? ICON.other) : ICON.other} says="Codealong sent this by itself" title={g ? <GlanceLine glance={g} /> : 'Codealong'} closed={g ? 'line' : 'start'}>
      {children}
    </MessageCard>
  );
}

/** A glance's words: the PR, then each part, the session it's about a link before it. */
function GlanceLine({ glance: g }: { glance: Glance }) {
  const pr = g.pr;
  const ref = pr?.url ? prRefOf(pr.url) : null;
  const named = pr && `#${pr.number} ${pr.title}`;
  return (
    <>
      {pr && ref && pr.url && (
        <PrLink url={pr.url} pr={ref}>
          {named}
        </PrLink>
      )}
      {pr && !ref && <span className="ca-msgcard-named">{named}</span>}
      {pr && g.parts.length > 0 && ' '}
      {g.parts.map((p, i) => (
        <Fragment key={i}>
          {i > 0 && ' · '}
          {p.session && (
            <>
              <SessionLink name={p.session.key} unknown={<span className="ca-msgcard-named">“{p.session.title}”</span>} />{' '}
            </>
          )}
          <span className="ca-msgcard-line">{p.line}</span>
        </Fragment>
      ))}
    </>
  );
}

/** Another session's message (its side conversation's among them): who it's from, then its
 *  first lines, a long one cut until it's opened. */
export function PeerCard({ item, at, now, sessionKey, children }: { item: TranscriptItem; at: number; now: number; sessionKey?: string; children: ReactNode }) {
  return (
    <MessageCard item={item} at={at} now={now} icon="message-circle" says="Another session sent this" title={<PeerLabel item={item} sessionKey={sessionKey} at={at} />} closed="start">
      {children}
    </MessageCard>
  );
}

/** Who another session's message is from, as its card says. */
export function peerLabel(it: TranscriptItem): string {
  return it.from ? `From “${it.from}”` : 'From another session';
}

/** Who another session's message is from, the sender a link to its session where the message said
 *  which one sent it (SessionLink.tsx), or to the session's side conversation where it came from
 *  one (side.ts). One this window doesn't list is its name, as before. */
function PeerLabel({ item, sessionKey, at }: { item: TranscriptItem; sessionKey?: string; at: number }) {
  if (item.fromKey) {
    const name = item.from ? `“${item.from}”` : 'another session';
    return (
      <>
        From{' '}
        <SessionLink name={item.fromKey} unknown={name}>
          {name}
        </SessionLink>
      </>
    );
  }
  if (sessionKey && (item.fromSide || item.from === 'Side conversation')) {
    return <SidePeerLabel sessionKey={sessionKey} item={item} at={at} />;
  }
  return <>{peerLabel(item)}</>;
}

/** Links `From “Side conversation”` to the side conversation it came from (or the session's side
 *  conversations list for older messages without an id when more than one could match). */
function SidePeerLabel({ sessionKey, item, at }: { sessionKey: string; item: TranscriptItem; at: number }) {
  const { open, kept } = useKnownSides(sessionKey);
  const target = sideForMessage(open, kept, item.fromSide, at);
  if (!target) return <>{peerLabel(item)}</>;
  const name = item.from ? `“${item.from}”` : 'Side conversation';
  const title =
    target.kind === 'side'
      ? target.question
        ? `On the side: ${target.question}\nClick to show it above the box.`
        : 'Click to show this side conversation above the box.'
      : 'Click to show this session’s side conversations above the box.';
  return (
    <>
      From{' '}
      <button
        type="button"
        className="ca-sidelink"
        title={title}
        onClick={(e) => {
          e.stopPropagation();
          if (target.kind === 'side') showSide(sessionKey, target);
          else showSides(sessionKey, { kind: 'list' });
        }}
      >
        {name}
      </button>
    </>
  );
}
