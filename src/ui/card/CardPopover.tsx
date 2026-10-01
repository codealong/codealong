// A card's popover: everything its face leaves out. The Canvas view opens it beside a card after
// the pointer rests on it (CanvasView.tsx), and a card pinned to the desktop opens it in a panel
// of its own beside the card (DesktopPopover.tsx, cardpop.rs); either way the pointer can move
// onto it, and its PR chips and session lines take clicks.

import type { CSSProperties, MouseEvent, PointerEvent, ReactNode } from 'react';
import { dueWords } from '../../due';
import { absolute, agoPhrase, ago, duration, tildePath } from '../../format';
import { HARNESS_WORD, sessionPlace } from '../../harness';
import type { Placed } from '../../lanes';
import { cameOf, originWord } from '../../lineage';
import { blockedBy, sectionOf, SECTION_WORD, snippet } from '../../todos';
import type { RefItem } from '../../canvas/model';
import type { Lane, Pr, Session, Todo } from '../../types';
import { BranchLabel, HarnessChip, Icon, LaneGlyph, PR_ICON, PrChip, Rich, STATUS_WORD, StatusGlyph } from '../primitives';
import { LabelChip, TodoGlyph } from '../TodoBits';
import type { CardData } from './data';

const cx = (...parts: (string | false | null | undefined)[]) => parts.filter(Boolean).join(' ');

function Row({ icon, children }: { icon: string; children: ReactNode }) {
  return (
    <div className="ca-cvp-row">
      <Icon name={icon} size={12} />
      <span>{children}</span>
    </div>
  );
}

export function SessionLine({ s, now }: { s: Session; now: number }) {
  return (
    <div className="ca-cvp-session">
      <StatusGlyph status={s.status} size={10} />
      <span className="ca-cvp-session-title">{s.title}</span>
      <span className="ca-cvp-dim">{ago(s.lastActivity, now)}</span>
    </div>
  );
}

/** Where a card is: on the canvas, or pinned to the desktop, where a click reads a session. */
export type CardPlace = 'canvas' | 'desktop';

interface CardDetailProps {
  item: Pick<RefItem, 'ref' | 'key' | 'title'>;
  data: CardData | null;
  now: number;
  terminal: string;
  /** Every todo, for the ones a session is on and what a todo waits on. */
  todos: Todo[];
  place: CardPlace;
  /** A PR chip's click. */
  onOpenPr?: (pr: Pr) => void;
  /** Every session on the board where it lives, for the one a session came from and the ones
   *  that came of it (`Session.origin`). */
  homes?: Map<string, Placed>;
  /** A session line's click: a worktree's or todo's sessions, or a session's kin. */
  onSession?: (lane: Lane, session: Session) => void;
  /** A session line's right-click: that session's menu, with Pin to desktop. */
  onSessionMenu?: (lane: Lane, session: Session, at: { x: number; y: number }) => void;
}

/** The popover's shell, the same wherever the card is. */
export function CardPopover({ style, children, onPointerEnter, onPointerLeave }: { style?: CSSProperties; children: ReactNode; onPointerEnter?: (e: PointerEvent) => void; onPointerLeave?: (e: PointerEvent) => void }) {
  return (
    <div className="ca-cv-popover" style={style} role="group" aria-label="More about this card" onPointerEnter={onPointerEnter} onPointerLeave={onPointerLeave}>
      {children}
    </div>
  );
}

/** The popover's body for a card: everything the card leaves out. */
export function CardDetail({ item, data, now, terminal, todos, place, homes, onOpenPr, onSession, onSessionMenu }: CardDetailProps) {
  const chip = (pr: Pr) => <PrChip pr={pr} onOpen={onOpenPr && (() => onOpenPr(pr))} />;
  // A session line takes a click where the card says what one does, and a right-click for its menu.
  const line = (lane: Lane, s: Session) => {
    if (!onSession && !onSessionMenu) return <SessionLine key={s.key} s={s} now={now} />;
    const menu = onSessionMenu
      ? (e: MouseEvent) => {
          e.preventDefault();
          e.stopPropagation();
          onSessionMenu(lane, s, { x: e.clientX, y: e.clientY });
        }
      : undefined;
    return (
      <button key={s.key} type="button" className="ca-cvp-sessionbtn" title={s.title} onClick={onSession && (() => onSession(lane, s))} onContextMenu={menu}>
        <SessionLine s={s} now={now} />
      </button>
    );
  };
  if (!data) {
    return (
      <>
        <div className="ca-cvp-title">{item.title ?? item.key}</div>
        <p className="ca-cvp-dim">The board no longer has this: it finished long ago, or its files are gone. Remove it from the canvas when you’re done with it.</p>
      </>
    );
  }
  switch (data.ref) {
    case 'worktree': {
      const l = data.lane;
      const own = l.sessions.filter((s) => !s.visitor && !s.archived);
      return (
        <>
          <div className="ca-cvp-head">
            <LaneGlyph lane={l} />
            <span className="ca-cvp-title">{l.callsign}</span>
          </div>
          <p className="ca-cvp-headline">
            <Rich text={l.headline} now={now} />
          </p>
          {l.detail && <p className="ca-cvp-excerpt">{l.detail}</p>}
          <div className="ca-cvp-meta">
            {l.branch && (
              <div className="ca-cvp-row">
                <BranchLabel branch={l.branch} ahead={l.git?.ahead} behind={l.git?.behind} dirty={l.git?.dirty} />
              </div>
            )}
            {l.pr && (
              <div className="ca-cvp-row">
                {chip(l.pr)}
                <span className="ca-cvp-clip">{l.pr.title}</span>
              </div>
            )}
            <Row icon="folder">{tildePath(l.path)}</Row>
            {l.git?.lastCommit && <Row icon="commit">{l.git.lastCommit}{l.git.lastCommitAt ? ` · ${agoPhrase(l.git.lastCommitAt, now)}` : ''}</Row>}
          </div>
          {own.length > 0 && (
            <div className="ca-cvp-list">
              {own.slice(0, 6).map((s) => line(l, s))}
              {own.length > 6 && <div className="ca-cvp-dim">and {own.length - 6} more</div>}
            </div>
          )}
          {place === 'desktop' && own.length > 0 && <div className="ca-cvp-hint">Click a session to read and answer it, double-click to open it</div>}
        </>
      );
    }
    case 'session': {
      const s = data.session;
      return (
        <>
          <div className="ca-cvp-head">
            <StatusGlyph status={s.status} />
            <span className="ca-cvp-title">{s.title}</span>
          </div>
          <div className="ca-cvp-status">
            {STATUS_WORD[s.status]}
            {s.statusSince ? ` · ${s.status === 'working' ? `for ${duration(s.statusSince, now)}` : agoPhrase(s.statusSince, now)}` : ''}
            {s.stalledSince ? ` · quiet since ${agoPhrase(s.stalledSince, now)}` : ''}
          </div>
          {s.detail && (s.status === 'working' || s.status === 'needs' || s.status === 'error') && (
            <p className="ca-cvp-headline">
              <Rich text={s.detail} now={now} />
            </p>
          )}
          {s.excerpt && !(s.detail ?? '').includes(s.excerpt.slice(0, 40)) && (
            <p className="ca-cvp-excerpt">
              <Rich text={snippet(s.excerpt, 360)} now={now} />
            </p>
          )}
          <div className="ca-cvp-meta">
            <div className="ca-cvp-row">
              <HarnessChip session={s} withStatus={false} />
              <span className="ca-cvp-dim">{HARNESS_WORD[s.harness]}</span>
            </div>
            <Row icon="folder">
              {data.lane.callsign}
              {s.branch && s.branch !== data.lane.callsign ? ` · ${s.branch}` : ''}
            </Row>
            {data.lane.pr && (
              <div className="ca-cvp-row">
                {chip(data.lane.pr)}
                <span className="ca-cvp-clip">{data.lane.pr.title}</span>
              </div>
            )}
            <Row icon="clock">
              Last prompt {agoPhrase(s.lastPromptAt, now)}
              {s.startedAt ? ` · started ${absolute(s.startedAt)}` : ''}
            </Row>
            {todos.filter((t) => t.sessions.includes(s.key)).map((t) => (
              <div key={t.id} className="ca-cvp-row">
                <TodoGlyph status={t.status} size={11} />
                <span className="ca-cvp-clip">{t.title || 'Untitled'}</span>
              </div>
            ))}
          </div>
          {homes && <Kin session={s} homes={homes} line={line} />}
          <div className="ca-cvp-hint">{place === 'desktop' ? `Click to read and answer it, double-click to open it in ${sessionPlace(s, terminal)}` : `Double-click to open in ${sessionPlace(s, terminal)}`}</div>
        </>
      );
    }
    case 'todo': {
      const t = data.todo;
      const section = sectionOf(t, todos);
      const due = t.due && t.status !== 'done' ? dueWords(t.due, now) : null;
      const blocked = t.status === 'todo' ? blockedBy(t, todos) : [];
      const text = t.description.replace(/!\[[^\]]*\]\([^)]*\)/g, '').trim();
      return (
        <>
          <div className="ca-cvp-head">
            <TodoGlyph status={t.status} waiting={!!t.waiting} />
            <span className="ca-cvp-title">{t.title || 'Untitled'}</span>
          </div>
          <div className="ca-cvp-status">
            {SECTION_WORD[section]}
            {due ? ` · ${due.text[0].toLowerCase()}${due.text.slice(1)}` : ''}
            {t.waiting?.note ? ` · ${t.waiting.note}` : ''}
            {blocked.length > 0 ? ` · waits on “${snippet(blocked[0].title || 'Untitled', 40)}”${blocked.length > 1 ? ` and ${blocked.length - 1} more` : ''}` : ''}
          </div>
          {data.labels.length > 0 && (
            <div className="ca-cvp-labels">
              {data.labels.map((l) => (
                <LabelChip key={l.id} label={l} />
              ))}
            </div>
          )}
          {text && <p className="ca-cvp-excerpt">{snippet(text, 420)}</p>}
          {data.sessions.length > 0 && (
            <div className="ca-cvp-list">
              {data.sessions.slice(0, 5).map(({ lane, session }) => line(lane, session))}
            </div>
          )}
          <div className="ca-cvp-hint">{data.sessions.length === 0 ? 'Double-click to start a session for it' : place === 'desktop' ? 'Click to read and answer its session, double-click to open it' : 'Double-click to open its session'}</div>
        </>
      );
    }
    case 'pr': {
      const p = data.pr;
      return (
        <>
          <div className="ca-cvp-head">
            <span className={cx('ca-lane-pricon', `ca-pr--${p.state}`)}>
              <Icon name={PR_ICON[p.state]} size={14} stroke={1.75} />
            </span>
            <span className="ca-cvp-title">
              #{p.number} {p.title}
            </span>
          </div>
          <div className="ca-cvp-meta">
            <div className="ca-cvp-row">
              {chip({ number: p.number, title: p.title, url: p.url, state: p.state, checks: p.checks, merge: p.merge, updatedAt: p.updatedAt, mergedAt: p.mergedAt, closedAt: p.closedAt })}
            </div>
            <Row icon="branch">
              {p.head}
              {p.base ? ` → ${p.base}` : ''}
            </Row>
            {p.author && <Row icon="user">{p.author}{p.mine ? ' (you)' : ''}</Row>}
            <Row icon="file">
              {p.changedFiles} files · +{p.additions} −{p.deletions}
            </Row>
            <Row icon="clock">Updated {agoPhrase(p.updatedAt, now)}</Row>
          </div>
          <div className="ca-cvp-hint">Double-click to open on GitHub</div>
        </>
      );
    }
  }
}

/** The most sessions that came of one listed in its popover. */
const KIN_SHOWN = 6;

/** Where a session came from and what came of it, as lines that take clicks: the popover's way to
 *  them, and, through their menus, to pinning them beside it on the desktop. */
function Kin({ session: s, homes, line }: { session: Session; homes: Map<string, Placed>; line: (lane: Lane, s: Session) => ReactNode }) {
  const from = s.origin ? homes.get(s.origin.key) : undefined;
  const kids = cameOf(s.key, homes.values());
  const forks = kids.filter((k) => k.session.origin?.how === 'fork');
  const started = kids.filter((k) => k.session.origin?.how !== 'fork');
  if (!from && kids.length === 0) return null;
  const group = (label: string, list: Placed[]) =>
    list.length > 0 && (
      <>
        <div className="ca-cvp-label">{label}</div>
        {list.slice(0, KIN_SHOWN).map((k) => line(k.lane, k.session))}
        {list.length > KIN_SHOWN && <div className="ca-cvp-dim">and {list.length - KIN_SHOWN} more</div>}
      </>
    );
  return (
    <div className="ca-cvp-list">
      {from && s.origin && group(originWord(s.origin.how), [from])}
      {group('Forked into', forks)}
      {group('Started', started)}
    </div>
  );
}
