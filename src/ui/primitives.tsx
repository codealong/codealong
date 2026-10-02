// Small pieces from the Codealong design system, typed for the live board.

import type { ButtonHTMLAttributes, MouseEvent as ReactMouseEvent, ReactNode } from 'react';
import { Fragment } from 'react';
import { deviceLabel } from '../devices';
import { middleTruncate, withTimes } from '../format';
import { sessionPlace } from '../harness';
import type { Bay, GitInfo, Lane, LogoStatus, Pr, PrState, Session, Status, Surface } from '../types';
import { LOGO_LABEL } from '../logo';
import { ICONS } from './icons';
import { MARK_PATHS, MARK_VIEW_BOX } from './brand';

export const cx = (...parts: (string | false | null | undefined)[]) => parts.filter(Boolean).join(' ');

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

/** The rounded C surrounding a commit node, shared with the native app icon. */
export function Mark({ size = 16, status = 'idle' }: { size?: number; status?: LogoStatus }) {
  return (
    <svg className="ca-mark" data-status={status} width={size} height={size} viewBox={MARK_VIEW_BOX} role="img" aria-label={`Codealong · ${LOGO_LABEL[status]}`} focusable="false">
      <path d={MARK_PATHS[0]} />
      <path className="ca-mark-node" d={MARK_PATHS[1]} />
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

/** One choice of a segmented control. */
export interface SegOption<T> {
  id: T;
  label: string;
  title?: string;
  /** Drawn before the label, or in its place (`look`). */
  icon?: string;
  disabled?: boolean;
}

interface SegProps<T> {
  options: readonly SegOption<T>[];
  /** The choice that's on. Null is a choice of its own where an option has it for its id (the
   *  account's own effort), and otherwise none of them. */
  value: T | null;
  onPick: (id: T) => void;
  /** What it chooses, for a screen reader: in words, or the id of the element that says it. */
  label?: string;
  labelledBy?: string;
  className?: string;
  title?: string;
  disabled?: boolean;
  /** What a choice draws: its icon then its label, its label alone (`text`), or its icon alone
   *  (`icon`), where the label is its name and, with no title of its own, its tooltip. */
  look?: 'both' | 'text' | 'icon';
  iconSize?: number;
  /** Choices that turn on and off, none of which need be on (where the bar goes): buttons that
   *  say whether they're pressed, where the others are radios. */
  toggles?: boolean;
  /** A press leaves the keyboard where it is: under a menu whose filter is being typed in. */
  keepsFocus?: boolean;
}

/** A segmented control: a few choices side by side, one of them on. Every one in the app is
 *  this, so they look, read and say what's on the same way. Picking the choice that's on calls
 *  `onPick` too; a caller for whom that would do something says so itself. */
export function Seg<T extends string | number | null>({ options, value, onPick, label, labelledBy, className, title, disabled, look = 'both', iconSize = 14, toggles = false, keepsFocus = false }: SegProps<T>) {
  return (
    <div className={cx('ca-segctl', className)} role={toggles ? 'group' : 'radiogroup'} aria-label={label} aria-labelledby={labelledBy} title={title}>
      {options.map((o) => {
        const on = value === o.id;
        return (
          <button
            key={String(o.id)}
            type="button"
            role={toggles ? undefined : 'radio'}
            aria-checked={toggles ? undefined : on}
            aria-pressed={toggles ? on : undefined}
            aria-label={look === 'icon' ? o.label : undefined}
            className={on ? 'is-on' : undefined}
            title={o.title ?? (look === 'icon' ? o.label : undefined)}
            disabled={disabled || o.disabled}
            onMouseDown={keepsFocus ? (e) => e.preventDefault() : undefined}
            onClick={() => onPick(o.id)}
          >
            {o.icon && look !== 'text' && <Icon name={o.icon} size={iconSize} />}
            {look !== 'icon' && o.label}
          </button>
        );
      })}
    </div>
  );
}

/** What a chip that does something answers to, by where it is. On a list's row (`inRow`) a
 *  click is the row's, which it selects, so a double-click does the chip's thing, in place of
 *  the row's own. Anywhere else the chip is a button: a click does its thing and nothing of
 *  what it's on (a card that takes clicks, a row that selects), once, however many clicks came
 *  in a run, so a double-click that lands on it doesn't do it twice. */
export function chipHandlers(act: (() => void) | undefined, inRow: boolean): { onMouseDown?: (e: Pick<ReactMouseEvent, 'stopPropagation'>) => void; onClick?: (e: Pick<ReactMouseEvent, 'stopPropagation' | 'detail'>) => void; onDoubleClick?: (e: Pick<ReactMouseEvent, 'stopPropagation'>) => void } {
  if (!act) return {};
  if (inRow) {
    return {
      onDoubleClick: (e) => {
        e.stopPropagation();
        act();
      },
    };
  }
  return {
    onMouseDown: (e) => e.stopPropagation(),
    onDoubleClick: (e) => e.stopPropagation(),
    // WebKit counts a run's clicks in `detail`: the second of a double-click is 2. A key's is 0.
    onClick: (e) => {
      e.stopPropagation();
      if (e.detail < 2) act();
    },
  };
}

/** The classes of the three chips that are drawn alike but named apart in the styles. */
const CHIP_CLASS = { chip: 'ca-chip', label: 'ca-labelchip', pr: 'ca-pr' } as const;

interface ChipProps {
  /** Which styles it takes: a chip's (the default), a todo label's, or a PR's. */
  kind?: keyof typeof CHIP_CLASS;
  className?: string;
  /** Its tooltip. With `does`, a chip that acts ends it with how: "… Click to open it on GitHub." */
  title?: string;
  /** What `act` does, in the tooltip's words: `open it on GitHub`. */
  does?: string;
  /** What a click on it does, or on a list's row (`inRow`) a double-click: see `chipHandlers`. */
  act?: () => void;
  inRow?: boolean;
  /** A right-click on it shows its own menu, in place of the menu of the row it's on. */
  onMenu?: () => void;
  /** A small ✕ at its end, which takes it away: on a chip that doesn't act itself. */
  remove?: { label: string; title?: string; run: () => void };
  children?: ReactNode;
}

/** A chip: one shape and size wherever it shows, and one answer to clicks (`chipHandlers`). The
 *  named chips below and in TodoBits.tsx say what's in one; anything else a row labels goes
 *  through this too, with its own class for its fill. */
export function Chip({ kind = 'chip', className, title, does, act, inRow = false, onMenu, remove, children }: ChipProps) {
  const base = CHIP_CLASS[kind];
  const tip = title && act && does ? `${title}. ${inRow ? 'Double-click' : 'Click'} to ${does}.` : title;
  const onContextMenu = onMenu
    ? (e: ReactMouseEvent) => {
        e.preventDefault();
        e.stopPropagation();
        onMenu();
      }
    : undefined;
  const inner = (
    <>
      {children}
      {remove && (
        <button type="button" aria-label={remove.label} title={remove.title} onClick={remove.run}>
          <Icon name="x" size={10} stroke={2.25} />
        </button>
      )}
    </>
  );
  if (!act || inRow) {
    return (
      <span className={cx(base, className)} title={tip} onContextMenu={onContextMenu} {...chipHandlers(act, inRow)}>
        {inner}
      </span>
    );
  }
  return (
    <button type="button" className={cx(base, `${base}--button`, className)} title={tip} onContextMenu={onContextMenu} {...chipHandlers(act, false)}>
      {inner}
    </button>
  );
}

/** How many more there are than the chips a row shows, with their names on hover: `+3`. */
export function MoreChip({ names, className }: { names: string[]; className?: string }) {
  if (names.length === 0) return null;
  return (
    <span className={cx('ca-chip', 'ca-chip--more', className)} title={names.join('\n')}>
      +{names.length}
    </span>
  );
}

/** A session's account and where it runs. With `onOpen`, a click opens it where it runs; with
 *  `inRow` too, it's on a list's row, where a click selects the row, so a double-click opens it. */
export function HarnessChip({ session, withStatus = true, onOpen, inRow = false, className }: { session: Session; withStatus?: boolean; onOpen?: () => void; inRow?: boolean; className?: string }) {
  return (
    <Chip className={className} title={`${session.account} · ${session.title} · ${STATUS_WORD[session.status]}`} does={`open in ${sessionPlace(session, 'terminal')}`} act={onOpen} inRow={inRow}>
      {withStatus && <StatusGlyph status={session.status} size={10} />}
      <Icon name={SURFACE_ICON[session.surface]} size={12} />
      <span>{session.account}</span>
    </Chip>
  );
}

/** A worktree as a chip, wherever a row names one: its name, after its repo when it has a name of
 *  its own (the main worktree is its repo, then `main`), and after its device, in that device's
 *  color, when it's on another of yours. `short` leaves it at the one name that tells it apart,
 *  and a lane short of room does the same (see .ca-wtchip-more). */
export function WorktreeChip({ lane, short = false }: { lane: Lane; short?: boolean }) {
  const branch = lane.branch ?? (lane.detached ? `detached @ ${lane.detached}` : null);
  const main = lane.origin === 'main';
  const own = !main && lane.callsign !== lane.repo;
  return (
    <Chip className="ca-wtchip" title={`${lane.device ? `On ${lane.device.name}: ` : ''}${lane.path}${branch ? `\nOn ${branch}` : ''}`}>
      <Icon name="folder" size={12} />
      {lane.device && (
        <span className="ca-wtchip-device">
          <i className={`ca-labeldot is-${lane.device.color}`} aria-hidden="true" />
          {deviceLabel(lane.device.name)}
        </span>
      )}
      {own && !short && <span className="ca-wtchip-more">{lane.repo} /</span>}
      <span className="ca-wtchip-name">{main ? lane.repo : lane.callsign}</span>
      {main && !short && <span className="ca-wtchip-more">/ main</span>}
    </Chip>
  );
}

/** Whether a press on a list's row selects it: the primary button, without ⌃, which macOS takes
 *  as a right-click. A right-click opens the row's menu and leaves the selection where it was. */
export function selects(e: { button: number; ctrlKey: boolean }): boolean {
  return e.button === 0 && !e.ctrlKey;
}

/** A PR's number, state and checks. With `onMenu`, a right-click on it shows the PR's own menu
 *  rather than the menu of the row it's on; with `onOpen`, a click opens it (on GitHub) rather
 *  than selecting the row, or with `inRow`, on a list's row, a double-click does. Without
 *  `titled`, it has no tooltip: a card shows instead (PrLink.tsx). */
export function PrChip({ pr, compact = false, titled = true, onMenu, onOpen, inRow = false }: { pr: Pr; compact?: boolean; titled?: boolean; onMenu?: () => void; onOpen?: () => void; inRow?: boolean }) {
  const c = pr.checks;
  const open = pr.state !== 'merged' && pr.state !== 'closed';
  const title = `#${pr.number} ${PR_WORD[pr.state]}: ${pr.title}`;
  return (
    <Chip kind="pr" className={`ca-pr--${pr.state}`} title={titled ? title : undefined} does="open it on GitHub" act={onOpen} inRow={inRow} onMenu={onMenu}>
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
    </Chip>
  );
}

/** How far a worktree has moved from the default branch on the remote, in words: `3 commits
 *  ahead of origin/main, 42 behind`. Null when it's even with it or git hasn't said. */
export function driftWords(g: GitInfo | null): string | null {
  const ahead = g?.baseAhead ?? 0;
  const behind = g?.baseBehind ?? 0;
  if (!g || (ahead === 0 && behind === 0)) return null;
  const base = g.base ?? 'the default branch';
  const commits = (n: number) => `${n.toLocaleString('en-US')} ${n === 1 ? 'commit' : 'commits'}`;
  if (behind === 0) return `${commits(ahead)} ahead of ${base}`;
  if (ahead === 0) return `${commits(behind)} behind ${base}`;
  return `${commits(ahead)} ahead of ${base}, ${behind.toLocaleString('en-US')} behind`;
}

/** Commits ahead of and behind the default branch on the remote: `↑3 ↓42`. Ahead is work the
 *  remote doesn't have yet, so it reads stronger than behind. */
export function Drift({ git: g }: { git: GitInfo | null }) {
  const words = driftWords(g);
  if (!g || !words) return null;
  const ahead = g.baseAhead ?? 0;
  const behind = g.baseBehind ?? 0;
  return (
    <span className="ca-drift" title={words}>
      {ahead > 0 && <span className="ca-drift-ahead">↑{ahead.toLocaleString('en-US')}</span>}
      {behind > 0 && <span className="ca-drift-behind">↓{behind.toLocaleString('en-US')}</span>}
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

/** A section that isn't a bay: Pinned, the days in Sessions, Archived. */
export function SectionHeader({ icon, label, count, hint }: { icon?: string; label: string; count: number; hint?: ReactNode }) {
  return (
    <div className="ca-bay ca-bay--section" role="heading" aria-level={3}>
      {icon && <Icon name={icon} size={12} />}
      <span>{label}</span>
      <span className="ca-bay-count">{count}</span>
      {hint && <span className="ca-bay-action ca-bay-hint">{hint}</span>}
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
  /** Drawn where the glyph or icon goes, for a mark of its own (a device's color). */
  lead?: ReactNode;
  /** A count that asks for you, in a solid badge before the plain count. */
  badge?: number;
  badgeTitle?: string;
  className?: string;
  title?: string;
  onClick?: () => void;
  onContextMenu?: () => void;
}

export function NavItem({ label, icon, glyph, count, active, child, tone, lead, badge, badgeTitle, className, title, onClick, onContextMenu }: NavItemProps) {
  return (
    <button
      type="button"
      className={cx('ca-nav', active && 'is-active', child && 'is-child', className)}
      aria-pressed={active}
      title={title}
      onClick={onClick}
      onContextMenu={
        onContextMenu &&
        ((e) => {
          e.preventDefault();
          onContextMenu();
        })
      }
    >
      {lead ?? (glyph ? <StatusGlyph status={glyph} size={10} title="" /> : icon ? <Icon name={icon} size={14} /> : null)}
      <span className="ca-nav-label">{label}</span>
      {badge ? (
        <span className="ca-nav-badge" title={badgeTitle}>
          {badge}
        </span>
      ) : null}
      {count != null && <span className={cx('ca-nav-count', tone && count > 0 && `is-${tone}`)}>{count}</span>}
    </button>
  );
}
