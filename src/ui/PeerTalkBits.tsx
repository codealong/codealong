// Peer-to-peer messaging and ownership chips for sessions: shows when two non-helper sessions are
// messaging each other back and forth (`Session.peers`, chatter.rs) and clarifies which session
// owns a shared todo or PR (`Todo.primary`, `PrItem.primary`).

import { useSyncExternalStore } from 'react';
import { sessionNamed } from '../codealongLinks';
import { agoPhrase } from '../format';
import type { Lane, PeerTalk, Pr, Session, Todo } from '../types';
import { linkHost } from './PrLink';
import { Chip, cx } from './primitives';

/** Matches chatter.rs WARN_MESSAGES / WARN_FLIPS: bidirectional ping-pong past the threshold. */
export function isPeerChatter(p: PeerTalk): boolean {
  return p.count >= 4 && p.flips >= 2;
}

/** Compact badge on a session list row when it has active peer ping-pong chatter. */
export function PeerChatterChip({ session }: { session: Session }) {
  const hot = (session.peers ?? []).filter(isPeerChatter);
  if (hot.length === 0) return null;
  const top = hot[0];
  const tip = hot.map((p) => `Back-and-forth with “${p.title}”: ${p.count} messages (${p.flips} flips) in the last 2 hours`).join('\n');
  return (
    <Chip className="ca-insight is-warn" title={tip}>
      ↔ {top.count}
    </Chip>
  );
}

/** Clickable peer-messaging chips in a session's header: jump straight to the other session. */
export function PeerTalkChips({
  session,
  now,
  onShowSession,
  onSessionMenu,
}: {
  session: Session;
  now: number;
  onShowSession?: (lane: Lane, session: Session) => void;
  onSessionMenu?: (lane: Lane, session: Session) => void;
}) {
  const peers = session.peers ?? [];
  const names = useSyncExternalStore(linkHost.subscribe, linkHost.sessions, linkHost.sessions);
  if (peers.length === 0) return null;
  return (
    <>
      {peers.slice(0, 2).map((p) => {
        const hit = sessionNamed(names, p.key);
        const warn = isPeerChatter(p);
        const show = hit ? (onShowSession ?? linkHost.actions()?.showSession) : undefined;
        const menu = hit ? (onSessionMenu ?? linkHost.actions()?.sessionMenu) : undefined;
        const tip = warn
          ? `Exchanged ${p.count} messages (${p.flips} flips) with “${p.title}” in the last 2 hours (last ${agoPhrase(p.lastAt, now)}). Consolidate under one owner to avoid ping-pong`
          : `${p.count} messages with “${p.title}” in the last 2 hours (last ${agoPhrase(p.lastAt, now)})`;
        return (
          <Chip
            key={p.key}
            className={cx('ca-peertalkchip', warn && 'is-warn')}
            title={tip}
            does="show it"
            act={hit && show ? () => show(hit.lane, hit.session) : undefined}
            onMenu={hit && menu ? () => menu(hit.lane, hit.session) : undefined}
          >
            <span aria-hidden="true">↔</span>
            <span>{p.count} msgs</span>
            <span aria-hidden="true">·</span>
            <span className="ca-chipname">{hit?.session.title || p.title}</span>
          </Chip>
        );
      })}
    </>
  );
}

/** Inline owner badge inside a multi-session todo chip when this session is its primary owner. */
export function TodoOwnerTag({ todo, sessionKey }: { todo: Todo; sessionKey: string }) {
  if (!todo.primary || todo.sessions.length < 2) return null;
  if (todo.primary.session !== sessionKey) return null;
  return (
    <span className="ca-owner-tag is-owner" title="This session is the primary owner of this todo">
      Owner
    </span>
  );
}

/** Clickable indicator when another session is the primary owner of a todo or open PR this session
 *  is also linked to, so you immediately know which session owns the task. One chip an owner,
 *  saying all it owns (`Todo and #12 owner:`) whole, and the owner's title cut to the chip. */
export function OtherOwnerChips({
  session,
  prs,
  todo,
  onShowSession,
}: {
  session: Session;
  prs: Pr[];
  todo?: Todo;
  onShowSession?: (lane: Lane, session: Session) => void;
}) {
  const names = useSyncExternalStore(linkHost.subscribe, linkHost.sessions, linkHost.sessions);
  const prPrimaryKey = useSyncExternalStore(
    linkHost.subscribe,
    () =>
      prs
        .map((p) => {
          const item = linkHost.pr(p.url);
          if (!item || item.state === 'merged' || item.state === 'closed') return '';
          return item.primary && item.primary !== session.key ? `${p.number}:${item.primary}` : '';
        })
        .filter(Boolean)
        .join('\n'),
    () => '',
  );
  const prOwners = prPrimaryKey
    ? prPrimaryKey.split('\n').map((row) => {
        const i = row.indexOf(':');
        return { what: `#${row.slice(0, i)}`, ownerKey: row.slice(i + 1) };
      })
    : [];

  const owners: { key: string; what: string[] }[] = [];
  const owns = (what: string, key: string) => {
    const o = owners.find((x) => x.key === key);
    if (!o) owners.push({ key, what: [what] });
    else if (!o.what.includes(what)) o.what.push(what);
  };
  if (todo?.primary && todo.primary.session !== session.key) owns('Todo', todo.primary.session);
  for (const p of prOwners) owns(p.what, p.ownerKey);
  if (owners.length === 0) return null;

  return (
    <>
      {owners.slice(0, 2).map((o) => {
        const hit = sessionNamed(names, o.key);
        const ownerTitle = hit?.session.title ?? o.key;
        const show = hit ? (onShowSession ?? linkHost.actions()?.showSession) : undefined;
        const what = o.what.length > 1 ? `${o.what.slice(0, -1).join(', ')} and ${o.what[o.what.length - 1]}` : o.what[0];
        return (
          <Chip key={o.key} className="ca-ownerchip" title={`${what} ${o.what.length > 1 ? 'are' : 'is'} owned by “${ownerTitle}”`} does="open the owning session" act={hit && show ? () => show(hit.lane, hit.session) : undefined}>
            <span>{what} owner:</span>
            <span className="ca-chipname">{ownerTitle}</span>
          </Chip>
        );
      })}
    </>
  );
}
