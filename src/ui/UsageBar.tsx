// The usage bar under the title bar: one chip per account, in config order. Each limit shows
// how full it is and, after the clock, how long until it resets. Per-model limits, like
// Fable's weekly limit, are marked with sparkles. Claude numbers are read every 2 minutes;
// Codex numbers arrive with each Codex turn. The star before each name marks, and toggles, the
// accounts Auto prefers.
//
// When the chips don't all fit, each shows only the limit it's closest to running out of, and
// hovering it shows all of them in a card. When even those don't fit, the row scrolls sideways
// (the mouse wheel scrolls it too).

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { agoPhrase, resetsPhrase, untilShort } from '../format';
import { usageNote } from '../harness';
import type { Meter, Severity, Usage } from '../types';
import { Button, Icon } from './primitives';

/** How long the pointer rests on a chip before its card shows; moving between chips while a card
 *  is up, or just after, switches straight away. */
const SHOW_MS = 150;
const HIDE_MS = 120;
const WARM_MS = 400;
/** A collapsed row opens up again only with this much room to spare, so a reset time growing
 *  by a character near the edge doesn't flip it back and forth. */
const SLACK = 16;

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
  const cls = ['ca-meter', compact && 'is-compact', `is-${m.severity}`, m.model && 'is-model'].filter(Boolean).join(' ');
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

interface ChipProps {
  usage: Usage;
  now: number;
  /** Only the limit closest to running out; the card shows the rest. */
  collapsed?: boolean;
  /** A hidden copy that measures how wide the chip is with every limit showing. */
  measure?: boolean;
  onOpen?: () => void;
  onSignIn?: () => void;
  /** The pointer or keyboard focus arrived on a collapsed chip, or left it. */
  onPeek?: (el: HTMLElement) => void;
  onUnpeek?: () => void;
}

/** A chip with the star before its name. The star is its own button beside the chip's, laid
 *  over the chip's left end, since a button can't hold another. The measuring copy has no star
 *  but keeps its room. */
function StarredChip(props: ChipProps & { onStar?: () => void }) {
  const { usage: u, measure, onStar } = props;
  return (
    <span className="ca-uchip-wrap">
      <AccountChip {...props} />
      {!measure && (
        <button
          type="button"
          className={['ca-uchip-star', u.preferred && 'is-on'].filter(Boolean).join(' ')}
          aria-pressed={!!u.preferred}
          aria-label={`Preferred: ${u.account}`}
          title={u.preferred ? `Preferred: Auto starts ${u.account} while it has room. Click to stop preferring it.` : `Prefer ${u.account}: Auto starts preferred accounts while one has room, and others only when they're all near a limit.`}
          onClick={onStar}
        >
          <Icon name="star" size={13} />
        </button>
      )}
    </span>
  );
}

function AccountChip({ usage: u, now, collapsed = false, measure = false, onOpen, onSignIn, onPeek, onUnpeek }: ChipProps) {
  const plain = u.error?.replace(/`/g, '') ?? '';
  if (u.needsSignIn) {
    return (
      <button type="button" className="ca-uchip is-attn" title={measure ? undefined : `${plain}. Click to sign in`} onClick={onSignIn}>
        <span className="ca-uchip-name">{u.account}</span>
        <span className="ca-uchip-signin">
          <Icon name="alert" size={12} stroke={2} />
          Sign in
        </span>
      </button>
    );
  }
  const stale = isStale(u);
  const about = [u.account, u.plan, asOfPhrase(u, now), stale ? plain : null].filter(Boolean).join(' · ');
  const summary = u.meters.map((m) => meterTitle(m, now)).join('; ');
  const headline = collapsed ? headlineMeter(u.meters) : null;
  const shown = headline ? [headline] : u.meters;
  // A collapsed chip's card says everything its tooltip would, so it has none.
  const peeks = !!headline && !measure;
  return (
    <button
      type="button"
      className={['ca-uchip', stale && 'is-stale'].filter(Boolean).join(' ')}
      title={measure || peeks ? undefined : `${about}. Open in Accounts`}
      aria-label={`${u.account}${u.plan ? `, ${u.plan}` : ''}${summary ? `. ${summary}` : ''}${stale ? `. ${plain}` : ''}`}
      onClick={onOpen}
      onMouseEnter={peeks ? (e) => onPeek?.(e.currentTarget) : undefined}
      onMouseLeave={peeks ? onUnpeek : undefined}
      onFocus={peeks ? (e) => e.currentTarget.matches(':focus-visible') && onPeek?.(e.currentTarget) : undefined}
      onBlur={peeks ? onUnpeek : undefined}
    >
      <span className="ca-uchip-name">{u.account}</span>
      {shown.length > 0 ? (
        shown.map((m) => <UsageMeter key={m.key} meter={m} now={now} titled={!measure && !peeks} />)
      ) : (
        <span className="ca-uchip-note" title={measure ? undefined : plain || undefined}>
          {usageNote(u)}
        </span>
      )}
      {stale && <Icon name="alert" size={12} />}
    </button>
  );
}

/** A collapsed chip's limits, all of them, in a card under it. It's only a picture: the chip's
 *  label already reads them out. */
function UsageCard({ usage: u, now, anchor }: { usage: Usage; now: number; anchor: HTMLElement }) {
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
  return createPortal(
    <div ref={ref} className={['ca-ucard', 'ca-usage-popover', stale && 'is-stale'].filter(Boolean).join(' ')} aria-hidden="true">
      <div className="ca-ucard-head">
        <span className="ca-uchip-name">{u.account}</span>
        {u.plan && <span className="ca-ucard-plan">{u.plan}</span>}
      </div>
      <div className="ca-ucard-meters">
        {u.meters.map((m) => (
          <div key={m.key} className={['ca-meter', 'is-compact', `is-${m.severity}`, m.model && 'is-model'].filter(Boolean).join(' ')}>
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
      {stale && (
        <div className="ca-ucard-error">
          <Icon name="alert" size={12} />
          <span>{u.error?.replace(/`/g, '')}</span>
        </div>
      )}
      {asOf && <div className="ca-ucard-foot">{asOf}</div>}
    </div>,
    document.body,
  );
}

/** Which collapsed chip's card is up, with the rest period before it shows and after it goes. */
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
  const hide = (now = false) => {
    window.clearTimeout(timer.current);
    const go = () => {
      if (warmUntil.current === Infinity) warmUntil.current = performance.now() + WARM_MS;
      setPeek(null);
    };
    if (now) go();
    else timer.current = window.setTimeout(go, HIDE_MS);
  };
  return { peek, show, hide };
}

interface UsageBarProps {
  usage: Usage[];
  now: number;
  onRefresh: () => void;
  /** Opens Settings at Accounts, at one account when given. */
  onAccounts: (account?: string) => void;
  onSignIn: (account: string) => void;
  /** Stars an account, or takes its star away. */
  onStar: (account: string, preferred: boolean) => void;
}

export function UsageBar({ usage, now, onRefresh, onAccounts, onSignIn, onStar }: UsageBarProps) {
  const reads = usage.filter((u) => u.harness === 'claude' && u.asOf).map((u) => u.asOf as number);
  const lastRead = reads.length ? Math.max(...reads) : null;
  const row = useRef<HTMLDivElement>(null);
  const measure = useRef<HTMLDivElement>(null);
  const [collapsed, setCollapsed] = useState(false);
  // Which ends have chips scrolled past them, for the fades there.
  const [clipped, setClipped] = useState({ start: false, end: false });
  const { peek, show, hide } = usePeek();
  const accounts = usage.map((u) => u.account).join('\n');

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
      if (e.ctrlKey || el.scrollWidth <= el.clientWidth || Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
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
  }, [collapsed]);
  useEffect(() => {
    const el = row.current;
    if (!el || !peek) return;
    const away = () => hide(true);
    el.addEventListener('scroll', away, { passive: true });
    window.addEventListener('resize', away);
    window.addEventListener('pointerdown', away, true);
    return () => {
      el.removeEventListener('scroll', away);
      window.removeEventListener('resize', away);
      window.removeEventListener('pointerdown', away, true);
    };
  }, [peek]);

  const peeked = collapsed && peek ? usage.find((u) => u.account === peek.account) : undefined;
  return (
    <div className="ca-usagebar" role="region" aria-label="Plan usage">
      <div
        ref={row}
        className={['ca-usagebar-accts', clipped.start && 'is-clip-start', clipped.end && 'is-clip-end'].filter(Boolean).join(' ')}
      >
        {usage.map((u) => (
          <StarredChip
            key={u.account}
            usage={u}
            now={now}
            collapsed={collapsed}
            onStar={() => onStar(u.account, !u.preferred)}
            onOpen={() => onAccounts(u.account)}
            onSignIn={() => onSignIn(u.account)}
            onPeek={(el) => show(u.account, el)}
            onUnpeek={() => hide()}
          />
        ))}
        {usage.length === 0 && <span className="ca-uchip-note">No accounts yet.</span>}
      </div>
      <div ref={measure} className="ca-usagebar-measure" aria-hidden="true">
        {usage.map((u) => (
          <StarredChip key={u.account} usage={u} now={now} measure />
        ))}
      </div>
      {peeked && peek && peeked.meters.length > 0 && <UsageCard usage={peeked} now={now} anchor={peek.anchor} />}
      <div className="ca-usagebar-end">
        <Button
          variant="ghost"
          size="sm"
          icon="refresh"
          aria-label="Refresh usage"
          title={lastRead ? `Claude usage read ${agoPhrase(lastRead, now)}; it refreshes every 2 minutes. Refresh now` : 'Refresh usage'}
          onClick={onRefresh}
        />
        <Button variant="ghost" size="sm" icon="settings" title="Accounts, in Settings (⌘,)" onClick={() => onAccounts()}>
          Accounts
        </Button>
      </div>
    </div>
  );
}
