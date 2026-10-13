// A card's face: a worktree, session, todo or PR in two lines, small enough that a few dozen fit
// on screen and the shape of the work reads at a glance. The same face is placed on the canvas
// (CanvasCard.tsx) and pinned to the desktop (DesktopPin.tsx), each adding only where it goes and
// what a click does.

import type { CSSProperties, ReactNode } from 'react';
import { dueWords } from '../../due';
import { ago, duration } from '../../format';
import type { RefItem } from '../../canvas/model';
import type { Pr } from '../../types';
import { BranchLabel, cx, Drift, Icon, LaneGlyph, PR_ICON, PR_WORD, PrChip, Rich, SessionEmblem, SessionGlyph } from '../primitives';
import { LabelDot, TodoGlyph } from '../TodoBits';
import { DeviceBadge } from '../DeviceBadge';
import { sessionLine, type CardData } from './data';

interface CardFaceProps {
  item: Pick<RefItem, 'ref' | 'key' | 'title'>;
  data: CardData | null;
  now: number;
  style?: CSSProperties;
  /** More of the card's classes. */
  className?: string;
  /** Drawn under the card's two lines: a desktop card's sessions. */
  children?: ReactNode;
  /** A click on the worktree's PR chip shows the PR in Codealong, and a double-click opens it on
   *  GitHub: on the desktop, where a card takes clicks. */
  onPrShow?: (pr: Pr) => void;
  onPrOpen?: (pr: Pr) => void;
}

/** A card before the board has been read: its shape, and that it's on its way. */
export function CardWaiting({ className }: { className?: string }) {
  return (
    <div className={cx('ca-cv-card', className, 'is-gone')}>
      <div className="ca-cv-l2">Reading sessions…</div>
    </div>
  );
}

/** A card's face: placed on the canvas, or pinned to the desktop (DesktopPin.tsx). */
export function CardFace({ item, data, now, style, className, children, onPrShow, onPrOpen }: CardFaceProps) {
  const cls = (kind: string, extra?: string | false) => cx('ca-cv-card', `is-${kind}`, className, extra);
  const id = `${item.ref}:${item.key}`;
  const prChip = (pr: Pr | null | undefined) => pr && <PrChip pr={pr} compact onShow={onPrShow && (() => onPrShow(pr))} onOpen={onPrOpen && (() => onPrOpen(pr))} />;
  if (!data) {
    return (
      <div className={cls(item.ref, 'is-gone')} style={style} data-id={id}>
        <div className="ca-cv-l1">
          <Icon name={item.ref === 'worktree' ? 'folder' : item.ref === 'pr' ? 'pr' : item.ref === 'todo' ? 'list-check' : 'terminal'} size={12} />
          <span className="ca-cv-title">{item.title ?? item.key}</span>
        </div>
        <div className="ca-cv-l2">No longer on the board</div>
        {children}
      </div>
    );
  }
  switch (data.ref) {
    case 'worktree': {
      const l = data.lane;
      return (
        // A worktree heads its column: a folder, not a status, which its sessions below show.
        <div className={cls('worktree', l.device && !l.device.online && 'is-away')} style={style} data-id={id}>
          <div className="ca-cv-l1">
            {(l.bay === 'review' || l.bay === 'merged') && l.pr ? <LaneGlyph lane={l} /> : <Icon name="folder" size={14} />}
            <span className="ca-cv-title ca-cv-callsign">{l.callsign}</span>
            {l.device && <DeviceBadge device={l.device} now={now} />}
            {prChip(l.pr)}
          </div>
          <div className="ca-cv-l2">
            {l.branch && l.branch !== l.callsign ? (
              <BranchLabel branch={l.branch} />
            ) : (
              <span className="ca-cv-headline">
                <Rich text={l.headline} now={now} />
              </span>
            )}
            <Drift git={l.git} />
            <span className="ca-cv-age">{ago(l.lastActivity, now)}</span>
          </div>
          {children}
        </div>
      );
    }
    case 'session': {
      const s = data.session;
      return (
        <div className={cls('session', cx(`st-${s.status}`, data.lane.device && !data.lane.device.online && 'is-away'))} style={style} data-id={id}>
          <div className="ca-cv-l1">
            <SessionGlyph s={s} />
            <SessionEmblem emblem={s.emblem} size={16} />
            <span className="ca-cv-title">{s.title}</span>
            <span className="ca-cv-age">{s.status === 'working' && s.statusSince ? duration(s.statusSince, now) : ago(s.lastActivity, now)}</span>
          </div>
          <div className="ca-cv-l2">
            {data.lane.device && <DeviceBadge device={data.lane.device} now={now} />}
            <span className="ca-cv-account">
              <Icon name={s.surface === 'cli' ? 'terminal' : 'app'} size={11} />
              {s.account}
            </span>
            <span className="ca-cv-headline">
              <Rich text={sessionLine(s)} now={now} />
            </span>
            {prChip(data.lane.pr)}
          </div>
          {children}
        </div>
      );
    }
    case 'todo': {
      const t = data.todo;
      const due = t.due && t.status !== 'done' ? dueWords(t.due, now) : null;
      return (
        <div className={cls('todo', `td-${t.status}`)} style={style} data-id={id}>
          <div className="ca-cv-l1">
            <TodoGlyph status={t.status} waiting={!!t.waiting} />
            <span className="ca-cv-title">{t.title || 'Untitled'}</span>
          </div>
          <div className="ca-cv-l2">
            {data.labels.slice(0, 4).map((l) => (
              <LabelDot key={l.id} label={l} />
            ))}
            {due && (
              <span className={`ca-cv-due is-${due.tone}`} title={`Due ${due.long}`}>
                {due.text}
              </span>
            )}
            <span className="ca-cv-headline">
              {data.sessions.length > 0
                ? `${data.sessions.length} ${data.sessions.length === 1 ? 'session' : 'sessions'}${data.lane ? ` · ${data.lane.callsign}` : ''}`
                : data.lane
                  ? data.lane.callsign
                  : 'Todo'}
            </span>
          </div>
          {children}
        </div>
      );
    }
    case 'pr': {
      const p = data.pr;
      return (
        <div className={cls('pr', `pr-${p.state}`)} style={style} data-id={id}>
          <div className="ca-cv-l1">
            <span className={cx('ca-lane-pricon', `ca-pr--${p.state}`)}>
              <Icon name={PR_ICON[p.state]} size={13} stroke={1.75} />
            </span>
            <span className="ca-cv-title">
              <span className="ca-cv-num">#{p.number}</span> {p.title}
            </span>
          </div>
          <div className="ca-cv-l2">
            <span>{PR_WORD[p.state]}</span>
            {p.checks.failed > 0 && <span className="ca-cv-bad">{p.checks.failed} failing</span>}
            <span className="ca-cv-age">{ago(p.updatedAt, now)}</span>
          </div>
          {children}
        </div>
      );
    }
  }
}
