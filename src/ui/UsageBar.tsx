// The usage bar under the title bar: one chip per account, in config order. Each limit shows
// how full it is and, after the clock, how long until it resets. Per-model limits, like
// Fable's weekly limit, are marked with sparkles. Claude numbers are read every 2 minutes;
// Codex numbers arrive with each Codex turn.
//
// A chip dragged sideways puts its account in another place: the others make way as it goes, and
// letting go writes the new order into config.toml (config.rs `with_account_order`), which
// Settings lists the accounts in too. The chips show it at once, before the board comes back.
//
// When the chips don't all fit, each shows only the limit it's closest to running out of, and
// hovering it shows all of them in a card. When even those don't fit, the row scrolls sideways
// (the mouse wheel scrolls it too).
//
// An account whose provider may train on what it sends (training.ts) has its chip in the
// attention colors with a shield on it, and its hover card says why, turns training off where
// Codealong can, and opens the provider's settings page; that card stays up while the pointer is
// on it.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent, type ReactNode, type RefObject } from 'react';
import { createPortal } from 'react-dom';
import { agoPhrase, resetsPhrase, untilShort } from '../format';
import { usageNote } from '../harness';
import { useLatest } from '../latest';
import { cardTitle, flagged, readNote, trainingOf } from '../training';
import type { Meter, Severity, Training, Usage } from '../types';
import { Button, cx, Icon } from './primitives';
import { useDismiss } from './useDismiss';
import { bare, keysIn, pinch } from '../platform';

/** How long the pointer rests on a chip before its card shows; moving between chips while a card
 *  is up, or just after, switches straight away. */
const SHOW_MS = 150;
const HIDE_MS = 120;
const WARM_MS = 400;
/** A collapsed row opens up again only with this much room to spare, so a reset time growing
 *  by a character near the edge doesn't flip it back and forth. */
const SLACK = 16;
/** How far the pointer moves with the button down before it drags a chip rather than clicks it. */
const DRAG_AFTER = 4;

export function meterTitle(m: Meter, now: number): string {
  const reset = m.resetsAt ? `, resets ${resetsPhrase(m.resetsAt, now)}` : '';
  return `${m.window}: ${Math.round(m.percent)}% used${reset}`;
}

/** When a limit resets, in words, for the Accounts dialog and the chips' cards. */
export function resetNote(m: Meter, now: number): string {
  return m.resetsAt ? `Resets ${resetsPhrase(m.resetsAt, now)}` : m.percent > 0 ? 'No reset pending' : 'Not started';
}

const RANK: Record<Severity, number> = { normal: 0, warning: 1, critical: 2 };

/** The limit an account is closest to running out of: the one its harness warns about most,
 *  then the fullest. The first listed wins a tie. */
export function headlineMeter(meters: Meter[]): Meter | null {
  let best: Meter | null = null;
  for (const m of meters) {
    const rank = RANK[m.severity];
    if (!best || rank > RANK[best.severity] || (rank === RANK[best.severity] && m.percent > best.percent)) best = m;
  }
  return best;
}

/** Whether the row should show one limit per account: when every limit takes more room than
 *  there is, and, once collapsed, until they fit with some to spare. */
export function shouldCollapse(collapsed: boolean, needed: number, room: number): boolean {
  return needed > (collapsed ? room - SLACK : room);
}

/** Compact inside a chip; full, with its reset time underneath, in the Accounts dialog. */
export function UsageMeter({ meter: m, now, compact = true, titled = true }: { meter: Meter; now: number; compact?: boolean; titled?: boolean }) {
  const pct = Math.round(m.percent);
  const title = meterTitle(m, now);
  const fill = <span className="ca-meter-fill" style={{ width: `${Math.min(100, Math.max(0, m.percent))}%` }} />;
  const label = (
    <span className="ca-meter-label">
      {m.model && <Icon name="sparkles" size={compact ? 11 : 12} />}
      <span>{compact ? m.label : m.window}</span>
    </span>
  );
  const cls = cx('ca-meter', compact && 'is-compact', `is-${m.severity}`, m.model && 'is-model');
  if (compact) {
    // Phrasing content only: the chip around it is a button, and names it for screen readers.
    return (
      <span className={cls} title={titled ? title : undefined}>
        {label}
        <span className="ca-meter-track">{fill}</span>
        <span className="ca-meter-value">{pct}%</span>
        {m.resetsAt ? (
          <span className="ca-meter-reset">
            <Icon name="clock" size={10} stroke={2} />
            {untilShort(m.resetsAt, now)}
          </span>
        ) : (
          <span />
        )}
      </span>
    );
  }
  return (
    <div className={cls} role="meter" aria-label={title} aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}>
      <div className="ca-meter-head">
        {label}
        <span className="ca-meter-value">{pct}%</span>
      </div>
      <div className="ca-meter-track">{fill}</div>
      <div className="ca-meter-foot">{resetNote(m, now)}</div>
    </div>
  );
}

/** Where an account's numbers came from, for tooltips. */
export function asOfPhrase(u: Usage, now: number): string | null {
  if (!u.asOf) return null;
  return u.harness === 'codex' ? `As of your last Codex turn, ${agoPhrase(u.asOf, now)}` : `Read ${agoPhrase(u.asOf, now)}`;
}

/** A failed refresh keeps the last numbers; they're dimmed and the tooltip or card says why. */
function isStale(u: Usage): boolean {
  return !!u.error && u.meters.length > 0;
}

/** What a chip in the row gets for being dragged to another place (`useSort`). */
interface SortProps {
  onPointerDown: (e: ReactPointerEvent<HTMLElement>) => void;
  className?: string;
  style?: CSSProperties;
}

interface ChipProps {
  usage: Usage;
  now: number;
  /** Only the limit closest to running out; the card shows the rest. */
  collapsed?: boolean;
  /** A hidden copy that measures how wide the chip is with every limit showing. */
  measure?: boolean;
  onOpen?: () => void;
  onSignIn?: () => void;
  /** The pointer or keyboard focus arrived on a chip with a card (collapsed, or flagged for
   *  training), or left it. */
  onPeek?: (el: HTMLElement) => void;
  onUnpeek?: () => void;
  /** Its provider may train on it (`flagged`): the chip says so, and its card says why. */
  training?: Training;
  sort?: SortProps;
}

function AccountChip({ usage: u, now, collapsed = false, measure = false, onOpen, onSignIn, onPeek, onUnpeek, training, sort }: ChipProps) {
  const plain = u.error?.replace(/`/g, '') ?? '';
  // Signed out, the last reading of whether it's trained on still stands, and its card with it.
  const trainingPeeks = !!training && !measure;
  if (u.needsSignIn) {
    return (
      <button
        type="button"
        className={cx('ca-uchip', 'is-attn', training && 'is-training', sort?.className)}
        style={sort?.style}
        title={measure ? undefined : `${plain}. Click to sign in`}
        onClick={onSignIn}
        onPointerDown={sort?.onPointerDown}
        onMouseEnter={trainingPeeks ? (e) => onPeek?.(e.currentTarget) : undefined}
        onMouseLeave={trainingPeeks ? onUnpeek : undefined}
      >
        <span className="ca-uchip-name">{u.account}</span>
        <span className="ca-uchip-signin">
          <Icon name="alert" size={12} stroke={2} />
          Sign in
        </span>
        {training && <Icon name="shield-off" size={12} stroke={2} />}
      </button>
    );
  }
  const stale = isStale(u);
  const about = [u.account, u.plan, asOfPhrase(u, now), stale ? plain : null].filter(Boolean).join(' · ');
  // The label stands in for everything the chip shows, the note in place of meters included.
  const summary = u.meters.length ? u.meters.map((m) => meterTitle(m, now)).join('; ') : usageNote(u);
  const headline = collapsed ? headlineMeter(u.meters) : null;
  const shown = headline ? [headline] : u.meters;
  // A chip with a card (collapsed, or flagged for training) says there everything its tooltip
  // would, so it has none.
  const peeks = (!!headline && !measure) || trainingPeeks;
  return (
    <button
      type="button"
      className={cx('ca-uchip', stale && 'is-stale', training && 'is-training', sort?.className)}
      style={sort?.style}
      title={measure || peeks ? undefined : `${about}. Show what it used and cost`}
      aria-label={`${u.account}${u.plan ? `, ${u.plan}` : ''}${summary ? `. ${summary}` : ''}${stale ? `. ${plain}` : ''}${training ? `. ${cardTitle(training)}: ${training.note}` : ''}`}
      onClick={onOpen}
      onPointerDown={sort?.onPointerDown}
      onMouseEnter={peeks ? (e) => onPeek?.(e.currentTarget) : undefined}
      onMouseLeave={peeks ? onUnpeek : undefined}
      onFocus={peeks ? (e) => e.currentTarget.matches(':focus-visible') && onPeek?.(e.currentTarget) : undefined}
      onBlur={peeks ? onUnpeek : undefined}
    >
      <span className="ca-uchip-name">{u.account}</span>
      {shown.length > 0 ? (
        shown.map((m) => <UsageMeter key={m.key} meter={m} now={now} titled={!measure && !peeks} />)
      ) : u.noLimits ? (
        // Nothing will ever show here, so it takes no room to say so; the words, and why, are on
        // hover.
        <span className="ca-uchip-none" title={measure ? undefined : [usageNote(u), plain].filter(Boolean).join('. ')}>
          <Icon name="gauge-off" size={10} stroke={2} />
        </span>
      ) : (
        <span className="ca-uchip-note" title={measure ? undefined : plain || undefined}>
          {usageNote(u)}
        </span>
      )}
      {stale && <Icon name="alert" size={12} />}
      {training && <Icon name="shield-off" size={12} stroke={2} />}
    </button>
  );
}

interface CardProps {
  usage: Usage;
  now: number;
  anchor: HTMLElement;
  /** Its limits are shown: the chip shows only the one closest to running out. */
  meters: boolean;
  /** Its provider may train on it: why, and what to do about it. */
  training?: Training;
  onStopTraining?: (account: string) => void;
  onOpenUrl?: (url: string) => void;
  /** The pointer came onto the card, or left it: one with buttons stays up while it's there. */
  onEnter?: () => void;
  onLeave?: () => void;
}

/** A chip's card under it: a collapsed chip's limits, all of them, and for an account its
 *  provider may train on, why, with Turn off training and Open settings. Its limits are only a
 *  picture: the chip's label already reads them out. */
function UsageCard({ usage: u, now, anchor, meters, training: t, onStopTraining, onOpenUrl, onEnter, onLeave }: CardProps) {
  const ref = useRef<HTMLDivElement>(null);
  // Under the chip, kept inside the window.
  useLayoutEffect(() => {
    const card = ref.current;
    if (!card) return;
    const r = anchor.getBoundingClientRect();
    card.style.left = `${Math.max(8, Math.min(r.left, window.innerWidth - card.offsetWidth - 8))}px`;
    card.style.top = `${r.bottom + 6}px`;
  });
  const stale = isStale(u);
  const asOf = asOfPhrase(u, now);
  const read = t && readNote(t, now);
  return createPortal(
    <div
      ref={ref}
      className={cx('ca-ucard', 'ca-usage-popover', stale && 'is-stale', t && 'is-training')}
      role={t ? 'group' : undefined}
      aria-label={t ? `${u.account}: ${cardTitle(t)}` : undefined}
      aria-hidden={t ? undefined : true}
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
    >
      <div className="ca-ucard-head">
        <span className="ca-uchip-name">{u.account}</span>
        {u.plan && <span className="ca-ucard-plan">{u.plan}</span>}
      </div>
      {t && (
        <div className="ca-training">
          <div className="ca-training-title">
            <Icon name="shield-off" size={14} stroke={2} />
            <span>{cardTitle(t)}</span>
          </div>
          <div className="ca-training-text">{t.note.replace(/`/g, '')}</div>
          {read && <div className="ca-training-asof">{read}</div>}
          {(t.url || t.canStop) && (
            <div className="ca-training-foot">
              {t.url && (
                <Button size="sm" variant={t.canStop ? 'secondary' : 'primary'} icon="external" onClick={() => onOpenUrl?.(t.url as string)}>
                  Open settings
                </Button>
              )}
              {t.canStop && (
                <Button size="sm" variant="primary" onClick={() => onStopTraining?.(t.account)}>
                  Turn off training
                </Button>
              )}
            </div>
          )}
        </div>
      )}
      {meters && (
        <div className="ca-ucard-meters">
          {u.meters.map((m) => (
            <div key={m.key} className={cx('ca-meter', 'is-compact', `is-${m.severity}`, m.model && 'is-model')}>
              <span className="ca-meter-label">
                {m.model && <Icon name="sparkles" size={11} />}
                <span>{m.label}</span>
              </span>
              <span className="ca-meter-track">
                <span className="ca-meter-fill" style={{ width: `${Math.min(100, Math.max(0, m.percent))}%` }} />
              </span>
              <span className="ca-meter-value">{Math.round(m.percent)}%</span>
              <span className="ca-ucard-reset">{resetNote(m, now)}</span>
            </div>
          ))}
        </div>
      )}
      {/* The chip has no tooltip once it has a card, so a failed read is said here either way. */}
      {stale && (
        <div className="ca-ucard-error">
          <Icon name="alert" size={12} />
          <span>{u.error?.replace(/`/g, '')}</span>
        </div>
      )}
      {/* Without its limits, the card says whose time this is: the training setting has its own. */}
      {asOf && <div className="ca-ucard-foot">{meters ? asOf : `Usage ${asOf.charAt(0).toLowerCase()}${asOf.slice(1)}`}</div>}
    </div>,
    document.body,
  );
}

/** Which chip's card is up, with the rest period before it shows and after it goes. */
function usePeek() {
  const [peek, setPeek] = useState<{ account: string; anchor: HTMLElement } | null>(null);
  const timer = useRef(0);
  // Until when a new chip's card shows at once: always while one is up, briefly after.
  const warmUntil = useRef(0);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const show = (account: string, anchor: HTMLElement) => {
    window.clearTimeout(timer.current);
    const go = () => {
      warmUntil.current = Infinity;
      setPeek({ account, anchor });
    };
    if (performance.now() < warmUntil.current) go();
    else timer.current = window.setTimeout(go, SHOW_MS);
  };
  // One function for good (it reads only the refs), so the effects that put the card away can
  // name it without running again.
  const hide = useCallback((now = false) => {
    window.clearTimeout(timer.current);
    const go = () => {
      if (warmUntil.current === Infinity) warmUntil.current = performance.now() + WARM_MS;
      setPeek(null);
    };
    if (now) go();
    else timer.current = window.setTimeout(go, HIDE_MS);
  }, []);
  // The pointer came onto the card on its way from the chip: it stays.
  const keep = useCallback(() => window.clearTimeout(timer.current), []);
  return { peek, show, hide, keep };
}

/** A chip's left and right edges along the row. */
export interface Edges {
  left: number;
  right: number;
}

/** Where a chip dragged from `from` lands with its middle at `middle`: after every other chip
 *  whose middle it has passed. */
export function dropPlace(edges: Edges[], from: number, middle: number): number {
  return edges.filter((e, i) => i !== from && middle > (e.left + e.right) / 2).length;
}

/** How far the chip at `i` moves aside while the one from `from` would land at `to`: one place,
 *  `by` (the dragged chip's width and a gap), toward where the dragged one came from. */
export function shiftOf(i: number, from: number, to: number, by: number): number {
  if (from < i && i <= to) return -by;
  if (to <= i && i < from) return by;
  return 0;
}

/** `names` with the one at `from` moved to `to`. */
export function moveName(names: string[], from: number, to: number): string[] {
  const next = names.filter((_, i) => i !== from);
  next.splice(to, 0, names[from]);
  return next;
}

/** The accounts in `order`; any it doesn't name come after, as they were. */
export function inOrder(usage: Usage[], order: string[]): Usage[] {
  const at = (u: Usage) => {
    const i = order.indexOf(u.account);
    return i < 0 ? order.length : i;
  };
  return [...usage].sort((a, b) => at(a) - at(b));
}

/** Whether the board lists the accounts `order` names in that order. */
export function listsInOrder(usage: Usage[], order: string[]): boolean {
  const listed = usage.map((u) => u.account).filter((a) => order.includes(a));
  return listed.join('\n') === order.filter((a) => listed.includes(a)).join('\n');
}

/** A chip being dragged to another place: where it came from and where it would land. */
interface Sorting {
  from: number;
  to: number;
  /** How far the chips it passes move aside: its width and the gap beside it. */
  by: number;
}

/** Dragging the row's chips into another order. A pointer drag, as `useRowDrag` is: clicks are
 *  left alone until the pointer has moved a few pixels, and Escape puts the chip back. */
function useSort(row: RefObject<HTMLDivElement | null>, onStart: () => void, onPlace: (names: string[]) => void) {
  const [sorting, setSorting] = useState<Sorting | null>(null);
  const stopNow = useRef<(() => void) | null>(null);
  const on = useLatest({ onStart, onPlace });
  useEffect(() => () => stopNow.current?.(), []);

  const start = (e: ReactPointerEvent<HTMLElement>, from: number, names: string[]) => {
    const el = row.current;
    // ⌘-, ⌃-, ⌥- and ⇧-clicks stay clicks.
    if (!el || e.button !== 0 || !bare(e)) return;
    stopNow.current?.();
    const chip = e.currentTarget;
    const x0 = e.clientX;
    const y0 = e.clientY;
    const scroll0 = el.scrollLeft;
    let x = x0;
    // Measured along the row once it's a drag, before anything moves aside.
    let edges: Edges[] | null = null;
    let to = from;
    let by = 0;

    const follow = () => {
      if (!edges) return;
      const dx = x - x0 + el.scrollLeft - scroll0;
      // It's drawn inside the row; where it lands goes by the pointer, so a chip wider than the
      // last one can still go after it.
      const lo = edges[0].left - edges[from].left;
      const hi = edges[edges.length - 1].right - edges[from].right;
      chip.style.transform = `translateX(${Math.min(hi, Math.max(lo, dx))}px)`;
      const next = dropPlace(edges, from, (edges[from].left + edges[from].right) / 2 + dx);
      if (next === to) return;
      to = next;
      setSorting({ from, to, by });
    };
    const move = (ev: PointerEvent) => {
      x = ev.clientX;
      if (!edges) {
        if (Math.abs(x - x0) + Math.abs(ev.clientY - y0) < DRAG_AFTER) return;
        edges = Array.from(el.querySelectorAll(':scope > .ca-uchip'), (c) => {
          const r = c.getBoundingClientRect();
          return { left: r.left + scroll0, right: r.right + scroll0 };
        });
        // One chip has nowhere to go.
        if (edges.length < 2) return stop();
        by = from < edges.length - 1 ? edges[from + 1].left - edges[from].left : edges[from].right - edges[from - 1].right;
        document.documentElement.classList.add('ca-dragging');
        on.current.onStart();
        setSorting({ from, to, by });
      }
      follow();
    };
    // The click a drag's release makes, over the chip it started on, isn't a click on the chip.
    const swallowClick = () => {
      const swallow = (c: MouseEvent) => {
        c.stopPropagation();
        c.preventDefault();
      };
      window.addEventListener('click', swallow, true);
      window.setTimeout(() => window.removeEventListener('click', swallow, true), 0);
    };
    const key = (ev: KeyboardEvent) => {
      if (ev.key !== 'Escape' || !edges) return;
      ev.preventDefault();
      ev.stopPropagation();
      stop();
      // The button is still down; letting go of it after is no click either.
      window.addEventListener('pointerup', swallowClick, { once: true });
    };
    const stop = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', end);
      window.removeEventListener('pointercancel', stop);
      window.removeEventListener('keydown', key, true);
      el.removeEventListener('scroll', follow);
      stopNow.current = null;
      chip.style.transform = '';
      document.documentElement.classList.remove('ca-dragging');
      setSorting(null);
    };
    const end = () => {
      const dragged = !!edges;
      stop();
      if (!dragged) return;
      swallowClick();
      if (to !== from) on.current.onPlace(moveName(names, from, to));
    };

    stopNow.current = stop;
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', end);
    window.addEventListener('pointercancel', stop);
    window.addEventListener('keydown', key, true);
    // The mouse wheel scrolls a row too wide for the window while a chip is held.
    el.addEventListener('scroll', follow, { passive: true });
  };

  /** What the chip at `i` of `names` gets for being dragged, and for making way. */
  const sortProps = (i: number, names: string[]): SortProps => ({
    onPointerDown: (e) => start(e, i, names),
    className: sorting?.from === i ? 'is-lifted' : undefined,
    style: sorting && sorting.from !== i ? { transform: `translateX(${shiftOf(i, sorting.from, sorting.to, sorting.by)}px)` } : undefined,
  });
  return { sorting: !!sorting, sortProps };
}

interface UsageBarProps {
  usage: Usage[];
  now: number;
  onRefresh: () => void;
  /** Opens Settings at Accounts, at one account when given. */
  onAccounts: (account?: string) => void;
  /** Opens the Usage window (UsageDialog.tsx), at one account when given: what sessions used and
   *  cost. */
  onUsage: (account?: string) => void;
  onSignIn: (account: string) => void;
  /** Whether each account's provider may train on it, for the flags (training.ts). */
  training?: Training[];
  /** Turns training off for an account; how it went is the caller's to say. */
  onStopTraining?: (account: string) => void;
  /** Opens a provider's settings page. */
  onOpenUrl?: (url: string) => void;
  /** The board hasn't been read yet, so it lists no accounts, though config.toml may have some. */
  loading?: boolean;
  /** What isn't working in the background (`ui/Notices.tsx`), at the bar's end. */
  notices?: ReactNode;
  /** Puts the accounts in this order in config.toml; a failure is the caller's to say, and the
   *  chips go back. */
  onReorder?: (accounts: string[]) => Promise<void>;
}

export function UsageBar({ usage: board, now, onRefresh, onAccounts, onUsage, onSignIn, training, onStopTraining, onOpenUrl, loading, notices, onReorder }: UsageBarProps) {
  // The order a drag left the chips in, until the board lists them so.
  const [order, setOrder] = useState<string[] | null>(null);
  const usage = order ? inOrder(board, order) : board;
  useEffect(() => {
    if (order && listsInOrder(board, order)) setOrder(null);
  }, [board, order]);
  const reads = usage.filter((u) => u.harness === 'claude' && u.asOf).map((u) => u.asOf as number);
  const lastRead = reads.length ? Math.max(...reads) : null;
  const row = useRef<HTMLDivElement>(null);
  const measure = useRef<HTMLDivElement>(null);
  const [collapsed, setCollapsed] = useState(false);
  // Which ends have chips scrolled past them, for the fades there.
  const [clipped, setClipped] = useState({ start: false, end: false });
  const { peek, show, hide, keep } = usePeek();
  const names = usage.map((u) => u.account);
  const accounts = names.join('\n');
  const sort = useSort(
    row,
    () => hide(true),
    (next) => {
      setOrder(next);
      // The caller says why it failed.
      onReorder?.(next).catch(() => setOrder(null));
    },
  );

  // Every chip is measured with all its limits, in a hidden copy, against the room the row has.
  useLayoutEffect(() => {
    const el = row.current;
    const full = measure.current;
    if (!el || !full) return;
    const check = () => {
      const style = getComputedStyle(el);
      const room = el.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      setCollapsed((c) => shouldCollapse(c, full.getBoundingClientRect().width, room));
      const end = el.scrollWidth - el.clientWidth;
      setClipped((c) => {
        const next = { start: el.scrollLeft > 0.5, end: el.scrollLeft < end - 0.5 };
        return next.start === c.start && next.end === c.end ? c : next;
      });
    };
    check();
    // The chips change size as their numbers and reset times do, which moves the row's end.
    const observer = new ResizeObserver(check);
    observer.observe(el);
    observer.observe(full);
    for (const chip of Array.from(el.children)) observer.observe(chip);
    el.addEventListener('scroll', check, { passive: true });
    return () => {
      observer.disconnect();
      el.removeEventListener('scroll', check);
    };
  }, [accounts, collapsed]);

  // A mouse wheel only scrolls up and down; over this row it scrolls sideways. Trackpads swipe
  // sideways on their own.
  useEffect(() => {
    const el = row.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (pinch(e) || el.scrollWidth <= el.clientWidth || Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
      e.preventDefault();
      el.scrollLeft += e.deltaY;
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, []);

  // The card belongs to where the chip was: scrolling, a click, or the row opening up again puts
  // it away.
  useEffect(() => {
    if (!collapsed) hide(true);
  }, [collapsed, hide]);
  // A press anywhere but on a card's buttons. Any key, which goes on to do what it does, puts away
  // a card that only shows; one with buttons goes on Escape, so Return and Space work them.
  const peekHasButtons = !!peek && flagged(trainingOf(training, peek.account));
  useDismiss(null, () => hide(true), { when: !!peek, keys: peekHasButtons ? 'escape' : 'any', inside: '.ca-usage-popover.is-training' });
  useEffect(() => {
    const el = row.current;
    if (!el || !peek) return;
    const away = () => hide(true);
    el.addEventListener('scroll', away, { passive: true });
    window.addEventListener('resize', away);
    return () => {
      el.removeEventListener('scroll', away);
      window.removeEventListener('resize', away);
    };
  }, [peek, hide]);

  const peeked = peek ? usage.find((u) => u.account === peek.account) : undefined;
  const peekedTraining = peeked && trainingOf(training, peeked.account);
  const flaggedPeek = flagged(peekedTraining) ? peekedTraining : undefined;
  const peekMeters = collapsed && !!peeked && peeked.meters.length > 0;
  return (
    <div className="ca-usagebar" role="region" aria-label="Plan usage">
      <div
        ref={row}
        className={cx('ca-usagebar-accts', clipped.start && 'is-clip-start', clipped.end && 'is-clip-end', sort.sorting && 'is-sorting')}
      >
        {usage.map((u, i) => {
          const t = trainingOf(training, u.account);
          return (
            <AccountChip
              key={u.account}
              usage={u}
              now={now}
              collapsed={collapsed}
              training={flagged(t) ? t : undefined}
              onOpen={() => onUsage(u.account)}
              onSignIn={() => onSignIn(u.account)}
              onPeek={(el) => {
                if (!sort.sorting) show(u.account, el);
              }}
              onUnpeek={() => hide()}
              sort={onReorder ? sort.sortProps(i, names) : undefined}
            />
          );
        })}
        {usage.length === 0 && !loading && <span className="ca-uchip-note">No accounts yet.</span>}
      </div>
      <div ref={measure} className="ca-usagebar-measure" aria-hidden="true">
        {usage.map((u) => {
          const t = trainingOf(training, u.account);
          return <AccountChip key={u.account} usage={u} now={now} measure training={flagged(t) ? t : undefined} />;
        })}
      </div>
      {peeked && peek && (peekMeters || flaggedPeek) && (
        <UsageCard
          usage={peeked}
          now={now}
          anchor={peek.anchor}
          meters={peekMeters}
          training={flaggedPeek}
          onOpenUrl={onOpenUrl}
          onStopTraining={(account) => {
            hide(true);
            onStopTraining?.(account);
          }}
          onEnter={flaggedPeek ? keep : undefined}
          onLeave={flaggedPeek ? () => hide() : undefined}
        />
      )}
      <div className="ca-usagebar-end">
        {notices}
        <Button
          variant="ghost"
          size="sm"
          icon="refresh"
          aria-label="Refresh usage"
          title={lastRead ? `Claude usage read ${agoPhrase(lastRead, now)}; it refreshes every 2 minutes. Refresh now` : 'Refresh usage'}
          onClick={onRefresh}
        />
        <Button variant="ghost" size="sm" icon="chart-bar" title={keysIn('What sessions used and cost (⇧⌘U)')} onClick={() => onUsage()}>
          Usage
        </Button>
        <Button variant="ghost" size="sm" icon="settings" title={keysIn('Accounts, in Settings (⌘,)')} onClick={() => onAccounts()}>
          Accounts
        </Button>
      </div>
    </div>
  );
}
