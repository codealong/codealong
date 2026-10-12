// Small pieces from the Codealong design system, typed for the live board.

import type { ButtonHTMLAttributes, CSSProperties, FormEvent, MouseEvent as ReactMouseEvent, ReactNode, TextareaHTMLAttributes } from 'react';
import { forwardRef, Fragment, useId, useRef } from 'react';
import { CLICK_WAIT_MS } from '../bar';
import { fieldAt, typingIn } from '../contextMenu';
import { deviceLabel } from '../devices';
import { middleTruncate, withTimes } from '../format';
import { HARNESS_WORD, sessionPlace } from '../harness';
import { branchLabel, laneWaits, waitsInBackground } from '../lanes';
import type { Bay, GitInfo, Lane, LogoStatus, Pr, PrState, Session, Status, Surface } from '../types';
import type { Waits } from '../queue';
import { LOGO_LABEL } from '../logo';
import { ICONS } from './icons';
import { MARK_PATHS, MARK_VIEW_BOX } from './brand';
import type { DragProps } from './useRowDrag';
import { useFocusTrap } from './focusTrap';
import { useEscape } from './useDismiss';

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

export function Icon({ name, size = 16, stroke = 1.5, title, className }: { name: string; size?: number; stroke?: number; title?: string; className?: string }) {
  const paths = ICONS[name];
  if (!paths) return null;
  return (
    <svg
      className={cx('ca-icon', className)}
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

/** What a kind of wait on you is drawn as wherever it's counted (the notch's pills, the strip over
 *  a session): a session by its status's glyph, a PR waiting on your review by the PR icon in the
 *  amber of what needs you. */
export function WaitsGlyph({ waits }: { waits: Waits }) {
  return waits === 'reviews' ? <Icon name="pr" size={12} stroke={1.75} className="ca-glyph-review" /> : <StatusGlyph status={waits} title="" />;
}

/** A status's glyph. `waiting` draws a working one as a clock that stands still: its turn is
 *  over and it waits on a command it left running in the background (`waitsInBackground`). */
export function StatusGlyph({ status, size = 12, title, waiting = false }: { status: Status; size?: number; title?: string; waiting?: boolean }) {
  if (waiting && status === 'working') {
    return (
      <svg className="ca-glyph ca-glyph--waiting" width={size} height={size} viewBox="0 0 12 12" role="img" aria-label={title ?? 'Waiting on a command in the background'}>
        <circle className="ca-g-clock" cx="6" cy="6" r="4.25" />
        <path className="ca-g-hands" d="M6 3.6V6l1.7 1.1" />
      </svg>
    );
  }
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

/** A session's status glyph, a clock while it waits on a command left running in the background. */
export function SessionGlyph({ s, size, title }: { s: Pick<Session, 'status' | 'background'>; size?: number; title?: string }) {
  return <StatusGlyph status={s.status} size={size} title={title} waiting={waitsInBackground(s)} />;
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
  return <StatusGlyph status={lane.glyph} waiting={laneWaits(lane)} />;
}

/** Keys that are a speck at a cap's size (⌃`'s backtick), which a cap sets larger so they read. */
const MARK_KEY = /^[`'.,]$/;

export function Kbd({ keys }: { keys: string[] }) {
  return (
    <span className="ca-kbds">
      {keys.map((k, i) => (
        <kbd key={i} className={cx('ca-kbd', MARK_KEY.test(k) && 'is-mark')}>
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

interface BoxTextProps extends Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, 'placeholder' | 'value' | 'rows'> {
  value: string;
  /** What the empty box says. */
  hint: string;
  /** The most lines it grows to before it scrolls. */
  lines: number;
}

/** The text of a box to write in (a session's reply, a PR's review). It grows with what's
 *  written, wrapped lines as well as new ones, up to `lines`, and then scrolls: a copy of what it
 *  shows, laid out in its place as the textarea lays out its text, gives it its height, with
 *  nothing measured. The copy is the hint while the box is empty, drawn over it in place of the
 *  textarea's own placeholder, and what's written after, unseen. WebKit lays a placeholder out as
 *  wide as its box times a card's CSS zoom (zoom.ts), so in a card drawn at a zoom the empty box
 *  scrolled sideways and a long hint was cut off, and no CSS on `::placeholder` takes it out of
 *  the layout (checked in a WKWebView on 2026-10-06). The hint is the textarea's
 *  `aria-placeholder`, for a screen reader. */
export const BoxText = forwardRef<HTMLTextAreaElement, BoxTextProps>(function BoxText({ value, hint, lines, ...rest }, ref) {
  return (
    <div className="ca-composer-text" style={{ '--ca-box-lines': lines } as CSSProperties}>
      <textarea ref={ref} value={value} rows={1} aria-placeholder={hint} {...rest} />
      {/* The space after the text gives a last line that's empty its height. */}
      <span className={cx('ca-composer-copy', value === '' && 'ca-composer-hint')} aria-hidden="true">
        {value === '' ? hint : `${value} `}
      </span>
    </div>
  );
});

interface DialogProps {
  title: string;
  /** A sentence or two under the title: what it's for. */
  lede?: ReactNode;
  /** While it works (saving, closing terminals): Escape, a press outside and its ✕ wait. */
  busy?: boolean;
  onClose: () => void;
  /** Makes it a form: Return in a field, or a `type="submit"` button in the foot, runs this. */
  onSubmit?: () => void;
  /** The buttons at the foot's end: the one that does it last. */
  foot: ReactNode;
  /** What the foot says or offers at its start: a count, a way to clear it. */
  aside?: ReactNode;
  /** The dialog's own class, for its width and what's in it. */
  className?: string;
  children?: ReactNode;
}

/** A dialog over the window: every one is this. It owns what they share: the scrim and a press
 *  on it, Escape (after any popover open inside it, `useEscape`), its name for a screen reader,
 *  its header and ✕, and the keyboard, which it takes as it opens, keeps (Tab goes round it,
 *  not out to the window behind, `useFocusTrap`) and hands back to what had it. */
export function Dialog({ title, lede, busy = false, onClose, onSubmit, foot, aside, className, children }: DialogProps) {
  const id = useId();
  const box = useRef<HTMLDivElement & HTMLFormElement>(null);
  const close = () => {
    if (!busy) onClose();
  };
  useEscape(close);
  useFocusTrap(box);
  const Box = onSubmit ? 'form' : 'div';
  return (
    <div
      className="ca-scrim"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <Box
        ref={box}
        className={cx('ca-dialog', className)}
        role="dialog"
        aria-modal="true"
        aria-labelledby={id}
        // It takes the keyboard itself when nothing in it does, only so Tab starts inside it:
        // it draws no ring for that.
        tabIndex={-1}
        style={{ outline: 'none' }}
        onSubmit={
          onSubmit &&
          ((e: FormEvent) => {
            e.preventDefault();
            onSubmit();
          })
        }
      >
        <header className="ca-dialog-head">
          <h2 id={id}>{title}</h2>
          <Button variant="ghost" size="sm" icon="x" aria-label="Close" onClick={onClose} disabled={busy} />
        </header>
        {lede && <p className="ca-dialog-lede">{lede}</p>}
        {children}
        <footer className="ca-dialog-foot">
          {aside ?? <span />}
          {/* The two names the styles have for a foot's buttons, until they have one. */}
          <span className="ca-previewdlg-actions ca-idle-actions">{foot}</span>
        </footer>
      </Box>
    </div>
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

/** One tab of a tab bar. */
export interface TabOption<T extends string> {
  id: T;
  label: string;
  /** How many it holds, quieter after the label; none says nothing. */
  count?: number | null;
  /** Something in it is under way (an agent reviewing): a dot after the label. */
  busy?: boolean;
  title?: string;
}

interface TabsProps<T extends string> {
  tabs: readonly TabOption<T>[];
  value: T;
  onPick: (id: T) => void;
  /** What the tabs are of, for a screen reader. */
  label: string;
  /** Ids the tabs' panels are named by: `${idBase}-${tab}`, a panel's tab `${idBase}-${tab}-tab`. */
  idBase: string;
  className?: string;
}

/** A tab bar: the parts of one thing, one shown at a time under it (a PR's description, changes
 *  and review). ← → go from tab to tab, as a tab bar's do. Its panels are the caller's, each
 *  `role="tabpanel"` with the id `${idBase}-${tab}`. */
export function Tabs<T extends string>({ tabs, value, onPick, label, idBase, className }: TabsProps<T>) {
  const bar = useRef<HTMLDivElement>(null);
  const step = (by: number) => {
    const at = tabs.findIndex((t) => t.id === value);
    const next = tabs[(at + by + tabs.length) % tabs.length];
    if (!next) return;
    onPick(next.id);
    bar.current?.querySelector<HTMLElement>(`#${CSS.escape(`${idBase}-${next.id}-tab`)}`)?.focus();
  };
  return (
    <div
      ref={bar}
      className={cx('ca-tabs', className)}
      role="tablist"
      aria-label={label}
      onKeyDown={(e) => {
        if (e.metaKey || e.ctrlKey || e.altKey || (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight')) return;
        e.preventDefault();
        step(e.key === 'ArrowRight' ? 1 : -1);
      }}
    >
      {tabs.map((t) => {
        const on = t.id === value;
        return (
          <button key={t.id} id={`${idBase}-${t.id}-tab`} type="button" role="tab" aria-selected={on} aria-controls={`${idBase}-${t.id}`} tabIndex={on ? 0 : -1} className={cx('ca-tab', on && 'is-on')} title={t.title} onClick={() => onPick(t.id)}>
            {t.label}
            {t.count != null && <span className="ca-tab-count">{t.count.toLocaleString('en-US')}</span>}
            {t.busy && <span className="ca-tab-busy" aria-hidden />}
          </button>
        );
      })}
    </div>
  );
}

/** A click waiting to see whether a second comes (`chipHandlers`' `twice`). */
export type ClickWait = { current: ReturnType<typeof setTimeout> | null };

/** What a chip that does something answers to, by where it is. On a list's row (`inRow`) a
 *  click is the row's, which it selects, so a double-click does the chip's thing, in place of
 *  the row's own. Anywhere else the chip is a button: a click does its thing and nothing of
 *  what it's on (a card that takes clicks, a row that selects), once, however many clicks came
 *  in a run, so a double-click that lands on it doesn't do it twice. A chip with `twice` is a
 *  button on a row too: a click waits `CLICK_WAIT_MS` for a second, as the bar's pills do, and
 *  then does its thing, or with the second does `twice` instead, without the first click's doing
 *  showing first (a PR's chip: one click shows it in Codealong, two open it on GitHub). */
export function chipHandlers(act: (() => void) | undefined, inRow: boolean, twice?: { run: () => void; wait: ClickWait }): { onMouseDown?: (e: Pick<ReactMouseEvent, 'stopPropagation'>) => void; onClick?: (e: Pick<ReactMouseEvent, 'stopPropagation' | 'detail'>) => void; onDoubleClick?: (e: Pick<ReactMouseEvent, 'stopPropagation'>) => void } {
  if (!act) return {};
  if (twice) {
    const { run, wait } = twice;
    return {
      onMouseDown: (e) => e.stopPropagation(),
      onDoubleClick: (e) => e.stopPropagation(),
      onClick: (e) => {
        e.stopPropagation();
        if (wait.current != null) clearTimeout(wait.current);
        wait.current = null;
        // The second click of a run is the double-click, and a third is neither. A key's click
        // (`detail` 0) has no second coming, so it acts at once.
        if (e.detail === 2) run();
        else if (e.detail === 0) act();
        else if (e.detail === 1)
          wait.current = setTimeout(() => {
            wait.current = null;
            act();
          }, CLICK_WAIT_MS);
      },
    };
  }
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
  /** What a double-click does in place of `act`, which a click then waits to do, and its words
   *  for the tooltip: `open it on GitHub`. See `chipHandlers`. */
  twice?: { run: () => void; does: string };
  /** A right-click on it shows its own menu, in place of the menu of the row it's on. */
  onMenu?: () => void;
  /** A small ✕ at its end, which takes it away: on a chip that doesn't act itself. */
  remove?: { label: string; title?: string; run: () => void };
  /** Its name for a screen reader, where it has no tooltip to say it (a list shows instead). */
  label?: string;
  /** A chip whose click shows a list: whether it's up. */
  expanded?: boolean;
  children?: ReactNode;
}

/** A chip: one shape and size wherever it shows, and one answer to clicks (`chipHandlers`). The
 *  named chips below and in TodoBits.tsx say what's in one; anything else a row labels goes
 *  through this too, with its own class for its fill. */
export function Chip({ kind = 'chip', className, title, does, act, inRow = false, twice, onMenu, remove, label, expanded, children }: ChipProps) {
  const base = CHIP_CLASS[kind];
  const wait = useRef<ReturnType<typeof setTimeout> | null>(null);
  const tip =
    title && act && does
      ? twice
        ? `${title}. Click to ${does}, double-click to ${twice.does}.`
        : `${title}. ${inRow ? 'Double-click' : 'Click'} to ${does}.`
      : title;
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
  if (!act || (inRow && !twice)) {
    return (
      <span className={cx(base, className)} title={tip} onContextMenu={onContextMenu} {...chipHandlers(act, inRow)}>
        {inner}
      </span>
    );
  }
  return (
    <button
      type="button"
      className={cx(base, `${base}--button`, className)}
      title={tip}
      aria-label={label}
      aria-haspopup={expanded === undefined ? undefined : 'menu'}
      aria-expanded={expanded}
      onContextMenu={onContextMenu}
      {...chipHandlers(act, false, twice && { run: twice.run, wait })}
    >
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
 *  `inRow` too, it's on a list's row, where a click selects the row, so a double-click opens it.
 *  A panel that lists it (PanelSession.tsx `PanelPlaces`) shows it in its place instead
 *  (`onShow`), with its menu on a right-click, and says more of it on hover (`detail`).
 *  `named` says its title instead, with the account on hover, where the chip stands for the
 *  session: on a PR's or a todo's row and in their panels, where nothing else names it. */
export function HarnessChip({ session, withStatus = true, named = false, onOpen, onShow, onMenu, inRow = false, className, detail, children }: { session: Session; withStatus?: boolean; named?: boolean; onOpen?: () => void; onShow?: () => void; onMenu?: () => void; inRow?: boolean; className?: string; detail?: string; children?: ReactNode }) {
  const said = named
    ? `${session.title}\n${session.account} (${HARNESS_WORD[session.harness]}) · ${STATUS_WORD[session.status]}`
    : `${session.account} · ${session.title} · ${STATUS_WORD[session.status]}`;
  return (
    <Chip
      className={cx(named && 'ca-sesschip', className)}
      title={`${said}${detail ? `\n${detail}` : ''}`}
      does={onShow ? 'show it here, right-click for its menu' : `open in ${sessionPlace(session, 'terminal')}`}
      act={onShow ?? onOpen}
      inRow={inRow}
      onMenu={onMenu}
    >
      {withStatus && <StatusGlyph status={session.status} size={10} />}
      <Icon name={SURFACE_ICON[session.surface]} size={12} />
      {named ? <span className="ca-sesschip-title">{session.title}</span> : <span>{session.account}</span>}
      {children}
    </Chip>
  );
}

/** A session's account where a line of text names it (its header), as its chip shows it: the icon
 *  for where it runs and the name as typed, with its harness on hover. */
export function AccountLabel({ session }: { session: Session }) {
  return (
    <span className="ca-account" title={`Its account: ${session.account} (${HARNESS_WORD[session.harness]})`}>
      <Icon name={SURFACE_ICON[session.surface]} size={12} />
      <span className="ca-account-name">{session.account}</span>
    </span>
  );
}

/** A worktree as a chip, wherever a row names one: its name, after its repo when it has a name of
 *  its own (the main worktree is its repo, then `main`), and after its device, in that device's
 *  color, when it's on another of yours. `short` leaves it at the one name that tells it apart,
 *  and a lane short of room does the same (see .ca-wtchip-more). A panel that lists it
 *  (PanelSession.tsx `PanelPlaces`) shows it on a click and gives its menu on a right-click, and
 *  adds what it says of it on hover (`note`: how it stands against the PR, say). */
export function WorktreeChip({ lane, short = false, note, onShow, onMenu }: { lane: Lane; short?: boolean; note?: string | null; onShow?: () => void; onMenu?: () => void }) {
  const branch = branchLabel(lane);
  const main = lane.origin === 'main';
  const own = !main && lane.callsign !== lane.repo;
  return (
    <Chip
      className="ca-wtchip"
      title={`${lane.device ? `On ${lane.device.name}: ` : ''}${lane.path}${branch ? `\nOn ${branch}` : ''}${note ? `\n${note}` : ''}`}
      does="show it in Worktrees, right-click for its menu"
      act={onShow}
      onMenu={onMenu}
    >
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

/** A row's answer to a right-click: its own menu in place of the webview's, and in place of
 *  the menu of a row it's on (an agent's line on its worktree's row). */
export function rowMenu(show: (() => void) | undefined): ((e: Pick<ReactMouseEvent, 'preventDefault' | 'stopPropagation'>) => void) | undefined {
  if (!show) return undefined;
  return (e) => {
    e.preventDefault();
    e.stopPropagation();
    show();
  };
}

/** A panel header's answer to a right-click anywhere on it: the menu of what the panel shows, as
 *  its row has it in the list. Anything inside with a menu of its own keeps it (it prevents the
 *  default), as does a text field you're typing in. One you weren't typing in, such as a todo's
 *  title, is part of the header, and lets go of any keyboard the right-click gave it, so the next
 *  key doesn't type over a word it selected. */
export function headerMenu(show: (() => void) | undefined): ((e: Pick<ReactMouseEvent, 'defaultPrevented' | 'preventDefault' | 'target'>) => void) | undefined {
  if (!show) return undefined;
  return (e) => {
    if (e.defaultPrevented || typingIn(e.target)) return;
    e.preventDefault();
    const field = fieldAt(e.target);
    if (field && document.activeElement === field) field.blur();
    show();
  };
}

interface RowProps {
  id?: string;
  /** An `option` (the default) is a row the list selects. A `group` holds rows of its own (a
   *  worktree's agents, each a line): it isn't selected itself, and `label` says what it is. */
  role?: 'option' | 'group';
  label?: string;
  selected?: boolean;
  className?: string;
  /** Dragging it (`useRowDrag`'s `rowProps`): where it starts, its place among the ordered rows,
   *  the marks of a drag under way, and with them the grip at its start. */
  drag?: DragProps;
  /** A press with the main button selects it; the event says which keys were held. */
  onSelect?: (e: ReactMouseEvent) => void;
  /** A double-click opens it. */
  onOpen?: (e: ReactMouseEvent) => void;
  /** A right-click (or ⌃-click) opens its menu and doesn't select it: the menu is for the row
   *  under the pointer, which contextMenu.ts rings while the menu is up. */
  onMenu?: () => void;
  children?: ReactNode;
}

/** A list's row: every list's rows are this, so a press, a double-click, a right-click and a
 *  drag mean the same in each. What's in the row, and its classes, are the list's own. */
export function Row({ id, role = 'option', label, selected = false, className, drag, onSelect, onOpen, onMenu, children }: RowProps) {
  return (
    <div
      id={id}
      role={role}
      aria-label={label}
      aria-selected={role === 'option' ? selected : undefined}
      className={cx(className, selected && 'is-selected', drag?.extraClass)}
      data-pin-index={drag?.pinIndex}
      onPointerDown={drag?.onPointerDown}
      onMouseDown={onSelect && ((e) => selects(e) && onSelect(e))}
      onDoubleClick={onOpen}
      onContextMenu={rowMenu(onMenu)}
    >
      {drag && (
        <span className="ca-grip" aria-hidden="true">
          <Icon name="grip-vertical" size={10} />
        </span>
      )}
      {children}
    </div>
  );
}

/** A PR's number, state and checks. With `onMenu`, a right-click on it shows the PR's own menu
 *  rather than the menu of the row it's on. With `onShow`, a click shows it in Codealong (its
 *  panel in PRs) and a double-click opens it on GitHub (`onOpen`), on a list's row too, where
 *  the click is then the chip's rather than the row's. With `onOpen` alone, a click opens it on
 *  GitHub, or with `inRow`, on a list's row, a double-click does. Without `titled`, it has no
 *  tooltip: a card shows instead (PrLink.tsx). */
export function PrChip({ pr, compact = false, titled = true, onMenu, onShow, onOpen, inRow = false }: { pr: Pr; compact?: boolean; titled?: boolean; onMenu?: () => void; onShow?: () => void; onOpen?: () => void; inRow?: boolean }) {
  const c = pr.checks;
  const open = pr.state !== 'merged' && pr.state !== 'closed';
  const title = `#${pr.number} ${PR_WORD[pr.state]}: ${pr.title}`;
  const twice = onShow && onOpen ? { run: onOpen, does: 'open it on GitHub' } : undefined;
  return (
    <Chip kind="pr" className={`ca-pr--${pr.state}`} title={titled ? title : undefined} does={onShow ? 'show it in Codealong' : 'open it on GitHub'} act={onShow ?? onOpen} twice={twice} inRow={inRow} onMenu={onMenu}>
      <Icon name={PR_ICON[pr.state]} size={12} stroke={1.75} />
      {/* One run of text, so the word sits on the number's baseline: as two pieces of the chip,
          each centered on its own, a zoomed page put the sans word a pixel below the mono. */}
      <span>
        #{pr.number}
        {!compact && <span className="ca-pr-word">{PR_WORD[pr.state]}</span>}
      </span>
      {/* What keeps it from merging that no state says, as the PRs view's pill says it. */}
      {open && pr.merge === 'conflicts' && (
        <span className="ca-checks is-attn" title="Conflicts with its base: resolve them before it can merge">
          <Icon name="alert" size={11} stroke={2} />
        </span>
      )}
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

/** A section header's hint for rows in your order (Pinned, a todo section): each chord on one
 *  cap, as the hotkey strip says its keys. */
export function ReorderHint() {
  return (
    <>
      Drag or <Kbd keys={['⌃⌘↑']} /> <Kbd keys={['⌃⌘↓']} /> to reorder
    </>
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
  /** The keys that go to it, shown in the count's place while the pointer is on it. */
  kbd?: string[];
  className?: string;
  title?: string;
  onClick?: () => void;
  onContextMenu?: () => void;
}

export function NavItem({ label, icon, glyph, count, active, child, tone, lead, badge, badgeTitle, kbd, className, title, onClick, onContextMenu }: NavItemProps) {
  return (
    <button
      type="button"
      className={cx('ca-nav', active && 'is-active', child && 'is-child', kbd && 'has-kbd', className)}
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
      {kbd && <Kbd keys={kbd} />}
    </button>
  );
}
