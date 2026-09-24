// Small pieces from the Codealong design system, typed for the live board.

import type { ButtonHTMLAttributes, ReactNode } from 'react';
import { Fragment } from 'react';
import { middleTruncate, withTimes } from '../format';
import type { Bay, Lane, Pr, PrState, Session, Status, Surface } from '../types';
import { ICONS } from './icons';

const cx = (...parts: (string | false | null | undefined)[]) => parts.filter(Boolean).join(' ');

export const STATUS_WORD: Record<Status, string> = {
  needs: 'Needs you',
  error: 'Error',
  working: 'Working',
  unread: 'Your turn',
  idle: 'Idle',
  stopped: 'Stopped',
};

export const BAY_WORD: Record<Bay, string> = {
  needs: 'Needs you',
  working: 'Working',
  unread: 'Your turn',
  review: 'In review',
  idle: 'Idle',
  merged: 'Merged',
};

export const PR_WORD: Record<PrState, string> = {
  draft: 'Draft',
  open: 'Open',
  approved: 'Approved',
  changes: 'Changes requested',
  merged: 'Merged',
  closed: 'Closed',
};

export const PR_ICON: Record<PrState, string> = {
  draft: 'pr-draft',
  open: 'pr',
  approved: 'pr',
  changes: 'pr',
  merged: 'merge',
  closed: 'pr-closed',
};

const SURFACE_ICON: Record<Surface, string> = { cli: 'terminal', app: 'app' };

/** Text with `backtick` spans as inline code and {ago:}/{for:} tokens as live times. */
export function Rich({ text, now }: { text: string; now: number }) {
  const parts = text.split('`');
  return (
    <>
      {parts.map((p, i) =>
        i % 2 ? (
          <code key={i} className="ca-ic">
            {p}
          </code>
        ) : (
          <Fragment key={i}>{withTimes(p, now)}</Fragment>
        ),
      )}
    </>
  );
}

export function Icon({ name, size = 16, stroke = 1.5, title }: { name: string; size?: number; stroke?: number; title?: string }) {
  const paths = ICONS[name];
  if (!paths) return null;
  return (
    <svg
      className="ca-icon"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={stroke}
      strokeLinecap="round"
      strokeLinejoin="round"
      role={title ? 'img' : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
      focusable="false"
    >
      {paths.map((d, i) => (
        <path key={i} d={d} />
      ))}
    </svg>
  );
}

/** Three staggered lanes and the dot riding along the last one. */
export function Mark({ size = 16 }: { size?: number }) {
  return (
    <svg className="ca-mark" width={size} height={size} viewBox="0 0 24 24" role="img" aria-label="Codealong">
      <rect className="ca-mark-lane" x="2" y="4.5" width="15" height="3.5" rx="1.75" />
      <rect className="ca-mark-lane" x="6.5" y="10.25" width="15.5" height="3.5" rx="1.75" />
      <rect className="ca-mark-lane" x="2" y="16" width="10" height="3.5" rx="1.75" />
      <circle className="ca-mark-dot" cx="17.75" cy="17.75" r="3" />
    </svg>
  );
}

export function StatusGlyph({ status, size = 12, title }: { status: Status; size?: number; title?: string }) {
  return (
    <svg className={cx('ca-glyph', `ca-glyph--${status}`)} width={size} height={size} viewBox="0 0 12 12" role="img" aria-label={title ?? STATUS_WORD[status]}>
      {status === 'needs' && <rect className="ca-g-attn" x="2.45" y="2.45" width="7.1" height="7.1" rx="1.1" transform="rotate(45 6 6)" />}
      {status === 'working' && (
        <>
          <circle className="ca-g-track" cx="6" cy="6" r="4.25" />
          <path className="ca-g-arc" d="M6 1.75a4.25 4.25 0 0 1 4.25 4.25" />
          <path className="ca-g-half" d="M6 1.75a4.25 4.25 0 0 1 0 8.5z" />
        </>
      )}
      {status === 'unread' && <circle className="ca-g-unread" cx="6" cy="6" r="4.25" />}
      {status === 'idle' && <circle className="ca-g-idle" cx="6" cy="6" r="4" />}
      {status === 'stopped' && <circle className="ca-g-stopped" cx="6" cy="6" r="4" />}
      {status === 'error' && (
        <>
          <circle className="ca-g-alert" cx="6" cy="6" r="5" />
          <path className="ca-g-x" d="M4.25 4.25l3.5 3.5M7.75 4.25l-3.5 3.5" />
        </>
      )}
    </svg>
  );
}

/** A lane's status box: its status glyph, or its PR icon in review and merged bays. */
export function LaneGlyph({ lane }: { lane: Lane }) {
  if ((lane.bay === 'review' || lane.bay === 'merged') && lane.pr) {
    return (
      <span className={cx('ca-lane-pricon', `ca-pr--${lane.pr.state}`)}>
        <Icon name={PR_ICON[lane.pr.state]} size={14} stroke={1.75} title={`#${lane.pr.number} ${PR_WORD[lane.pr.state]}`} />
      </span>
    );
  }
  return <StatusGlyph status={lane.glyph} />;
}

export function Kbd({ keys }: { keys: string[] }) {
  return (
    <span className="ca-kbds">
      {keys.map((k, i) => (
        <kbd key={i} className="ca-kbd">
          {k}
        </kbd>
      ))}
    </span>
  );
}

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost';
  size?: 'md' | 'sm';
  icon?: string;
  kbd?: string[];
  children?: ReactNode;
}

export function Button({ variant = 'secondary', size = 'md', icon, kbd, children, className, ...rest }: ButtonProps) {
  return (
    <button type="button" {...rest} className={cx('ca-btn', `ca-btn--${variant}`, `ca-btn--${size}`, !children && 'is-icon', className)}>
      {icon && <Icon name={icon} size={size === 'sm' ? 14 : 16} />}
      {children && <span>{children}</span>}
      {kbd && <Kbd keys={kbd} />}
    </button>
  );
}

export function HarnessChip({ session, withStatus = true, onOpen }: { session: Session; withStatus?: boolean; onOpen?: () => void }) {
  const where = session.surface === 'cli' ? 'terminal' : session.harness === 'claude' ? 'Claude app' : 'Codex';
  const title = `${session.account} · ${session.title} · ${STATUS_WORD[session.status]}`;
  const inner = (
    <>
      {withStatus && <StatusGlyph status={session.status} size={10} />}
      <Icon name={SURFACE_ICON[session.surface]} size={12} />
      <span>{session.account}</span>
    </>
  );
  if (!onOpen) {
    return (
      <span className="ca-chip" title={title}>
        {inner}
      </span>
    );
  }
  return (
    <button
      type="button"
      className="ca-chip ca-chip--button"
      title={`${title}. Click to open in ${where}.`}
      onMouseDown={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onClick={onOpen}
    >
      {inner}
    </button>
  );
}

export function PrChip({ pr, compact = false }: { pr: Pr; compact?: boolean }) {
  const c = pr.checks;
  const open = pr.state !== 'merged' && pr.state !== 'closed';
  return (
    <span className={cx('ca-pr', `ca-pr--${pr.state}`)} title={`#${pr.number} ${PR_WORD[pr.state]}: ${pr.title}`}>
      <Icon name={PR_ICON[pr.state]} size={12} stroke={1.75} />
      <span>#{pr.number}</span>
      {!compact && <span className="ca-pr-word">{PR_WORD[pr.state]}</span>}
      {open && c.failed > 0 && (
        <span className="ca-checks is-fail" title={`${c.failed} failing`}>
          <Icon name="x" size={11} stroke={2.25} />
          {c.failed}
        </span>
      )}
      {open && c.failed === 0 && c.pending > 0 && (
        <span className="ca-checks is-pending" title={`${c.pending} running`}>
          <Icon name="circle-dashed" size={11} stroke={2} />
          {c.pending}
        </span>
      )}
      {open && c.failed === 0 && c.pending === 0 && c.passed > 0 && (
        <span className="ca-checks is-pass" title={`${c.passed} passed`}>
          <Icon name="check" size={11} stroke={2.25} />
          {c.passed}
        </span>
      )}
    </span>
  );
}

export function BranchLabel({ branch, ahead, behind, dirty }: { branch: string; ahead?: number; behind?: number; dirty?: number }) {
  return (
    <span className="ca-branch" title={branch}>
      <Icon name="branch" size={12} />
      <span className="ca-branch-name">{middleTruncate(branch, 34)}</span>
      {ahead || behind ? (
        <span className="ca-branch-ab">
          {ahead ? `↑${ahead}` : ''}
          {ahead && behind ? ' ' : ''}
          {behind ? `↓${behind}` : ''}
        </span>
      ) : null}
      {dirty ? (
        <span className="ca-branch-dirty" title={`${dirty} uncommitted`}>
          ● {dirty}
        </span>
      ) : null}
    </span>
  );
}

export function BayHeader({ bay, count, action }: { bay: Bay; count: number; action?: ReactNode }) {
  const glyph: Partial<Record<Bay, Status>> = { needs: 'needs', working: 'working', unread: 'unread', idle: 'idle' };
  const g = glyph[bay];
  return (
    <div className={cx('ca-bay', `ca-bay--${bay}`)} role="heading" aria-level={3}>
      {g ? <StatusGlyph status={g} size={10} title="" /> : <Icon name={bay === 'merged' ? 'merge' : 'pr'} size={12} />}
      <span>{BAY_WORD[bay]}</span>
      <span className="ca-bay-count">{count}</span>
      {action && <span className="ca-bay-action">{action}</span>}
    </div>
  );
}

interface NavItemProps {
  label: string;
  icon?: string;
  glyph?: Status;
  count?: number;
  active?: boolean;
  child?: boolean;
  tone?: 'attn' | 'alert';
  onClick?: () => void;
}

export function NavItem({ label, icon, glyph, count, active, child, tone, onClick }: NavItemProps) {
  return (
    <button type="button" className={cx('ca-nav', active && 'is-active', child && 'is-child')} aria-pressed={active} onClick={onClick}>
      {glyph ? <StatusGlyph status={glyph} size={10} title="" /> : icon ? <Icon name={icon} size={14} /> : null}
      <span className="ca-nav-label">{label}</span>
      {count != null && <span className={cx('ca-nav-count', tone && count > 0 && `is-${tone}`)}>{count}</span>}
    </button>
  );
}
