// The usage bar under the title bar: one chip per account, in config order. Each limit shows
// how full it is and, after the clock, how long until it resets. Per-model limits, like
// Fable's weekly limit, are marked with sparkles. Claude numbers are read every 2 minutes;
// Codex numbers arrive with each Codex turn.

import { agoPhrase, resetsPhrase, untilShort } from '../format';
import type { Meter, Usage } from '../types';
import { Button, Icon } from './primitives';

export function meterTitle(m: Meter, now: number): string {
  const reset = m.resetsAt ? `, resets ${resetsPhrase(m.resetsAt, now)}` : '';
  return `${m.window}: ${Math.round(m.percent)}% used${reset}`;
}

/** Compact inside a chip; full, with its reset time underneath, in the Accounts dialog. */
export function UsageMeter({ meter: m, now, compact = true }: { meter: Meter; now: number; compact?: boolean }) {
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
      <span className={cls} title={title}>
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
      <div className="ca-meter-foot">{m.resetsAt ? `Resets ${resetsPhrase(m.resetsAt, now)}` : m.percent > 0 ? 'No reset pending' : 'Not started'}</div>
    </div>
  );
}

/** Where an account's numbers came from, for tooltips. */
export function asOfPhrase(u: Usage, now: number): string | null {
  if (!u.asOf) return null;
  return u.harness === 'codex' ? `As of your last Codex turn, ${agoPhrase(u.asOf, now)}` : `Read ${agoPhrase(u.asOf, now)}`;
}

function AccountChip({ usage: u, now, onOpen, onSignIn }: { usage: Usage; now: number; onOpen: () => void; onSignIn: () => void }) {
  const plain = u.error?.replace(/`/g, '') ?? '';
  if (u.needsSignIn) {
    return (
      <button type="button" className="ca-uchip is-attn" title={`${plain}. Click to sign in`} onClick={onSignIn}>
        <span className="ca-uchip-name">{u.account}</span>
        <span className="ca-uchip-signin">
          <Icon name="alert" size={12} stroke={2} />
          Sign in
        </span>
      </button>
    );
  }
  // A failed refresh keeps the last numbers; they're dimmed and the tooltip says why.
  const stale = !!u.error && u.meters.length > 0;
  const about = [u.account, u.plan, asOfPhrase(u, now), stale ? plain : null].filter(Boolean).join(' · ');
  const summary = u.meters.map((m) => meterTitle(m, now)).join('; ');
  return (
    <button
      type="button"
      className={['ca-uchip', stale && 'is-stale'].filter(Boolean).join(' ')}
      title={`${about}. Open in Accounts`}
      aria-label={`${u.account}${u.plan ? `, ${u.plan}` : ''}${summary ? `. ${summary}` : ''}${stale ? `. ${plain}` : ''}`}
      onClick={onOpen}
    >
      <span className="ca-uchip-name">{u.account}</span>
      {u.meters.length > 0 ? (
        u.meters.map((m) => <UsageMeter key={m.key} meter={m} now={now} />)
      ) : (
        <span className="ca-uchip-note" title={plain || undefined}>
          {u.loading ? 'Reading…' : u.harness === 'codex' ? 'No turns yet' : 'Usage unavailable'}
        </span>
      )}
      {stale && <Icon name="alert" size={12} />}
    </button>
  );
}

interface UsageBarProps {
  usage: Usage[];
  now: number;
  onRefresh: () => void;
  /** Opens the Accounts dialog, at one account when given. */
  onAccounts: (account?: string) => void;
  onSignIn: (account: string) => void;
}

export function UsageBar({ usage, now, onRefresh, onAccounts, onSignIn }: UsageBarProps) {
  const reads = usage.filter((u) => u.harness === 'claude' && u.asOf).map((u) => u.asOf as number);
  const lastRead = reads.length ? Math.max(...reads) : null;
  return (
    <div className="ca-usagebar" role="region" aria-label="Plan usage">
      <div className="ca-usagebar-accts">
        {usage.map((u) => (
          <AccountChip key={u.account} usage={u} now={now} onOpen={() => onAccounts(u.account)} onSignIn={() => onSignIn(u.account)} />
        ))}
        {usage.length === 0 && <span className="ca-uchip-note">No accounts yet.</span>}
      </div>
      <div className="ca-usagebar-end">
        <Button
          variant="ghost"
          size="sm"
          icon="refresh"
          aria-label="Refresh usage"
          title={lastRead ? `Claude usage read ${agoPhrase(lastRead, now)}; it refreshes every 2 minutes. Refresh now` : 'Refresh usage'}
          onClick={onRefresh}
        />
        <Button variant="ghost" size="sm" icon="settings" title="Accounts (⌘,)" onClick={() => onAccounts()}>
          Accounts
        </Button>
      </div>
    </div>
  );
}
