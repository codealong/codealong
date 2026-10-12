// View › Usage… (⇧⌘U), and a click on the usage bar: what sessions used and cost over the last
// days, from the token ledger (ledger.rs). It leads with what the work would have cost at API
// prices against what was paid (plans a day's share at a time, Claude's extra usage, accounts that
// pay by the token), then two charts over the same days, the running totals and each day's cost
// by account, and a table of it all by account, model, project, session or day. Prices and plans
// are config.toml's (`[prices]`, an account's `monthly` and `per_token`); the words for each
// number are spend.ts's.

import { useEffect, useMemo, useState } from 'react';
import { tokenReport } from '../api';
import { agoPhrase } from '../format';
import { byProject, cacheShare, cumulative, dayLabel, leverage, money, moneyTick, readNote, seriesColor, sessionLabel, stackOf, ticks, tokenCount, totalTokens } from '../spend';
import type { Board, Session, Spent, SpendDay, TokenReport } from '../types';
import { Button, cx, Dialog, Icon, Seg } from './primitives';

type By = 'account' | 'model' | 'project' | 'session' | 'day';

const SPANS: { id: number; label: string }[] = [
  { id: 7, label: '7 days' },
  { id: 30, label: '30 days' },
  { id: 90, label: '90 days' },
  { id: 365, label: 'Year' },
];

const BYS: { id: By; label: string }[] = [
  { id: 'account', label: 'Account' },
  { id: 'model', label: 'Model' },
  { id: 'project', label: 'Project' },
  { id: 'session', label: 'Session' },
  { id: 'day', label: 'Day' },
];

interface Props {
  board: Board;
  /** The account to show at first, from a click on its chip; null shows them all. */
  account: string | null;
  now: number;
  onClose: () => void;
  /** Shows a session of this Mac's on the board. */
  onShowSession: (key: string) => void;
  /** Settings › Accounts, where each account's plan price is set. */
  onAccounts: (account?: string) => void;
}

export function UsageDialog({ board, account: first, now, onClose, onShowSession, onAccounts }: Props) {
  const [days, setDays] = useState(30);
  const [account, setAccount] = useState<string | null>(first);
  const [by, setBy] = useState<By>('account');
  const [report, setReport] = useState<TokenReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setLoading(true);
    const ask = () =>
      tokenReport(days, account)
        .then((r) => {
          if (!live) return;
          setReport(r);
          setError(null);
        })
        .catch((e: unknown) => live && setError(String(e)))
        .finally(() => live && setLoading(false));
    void ask();
    // While the ledger reads records for the first time, and as sessions work, the numbers move.
    const t = setInterval(() => void ask(), 30_000);
    return () => {
      live = false;
      clearInterval(t);
    };
  }, [days, account]);

  const order = useMemo(() => board.usage.map((u) => u.account), [board.usage]);
  const sessions = useMemo(() => new Map<string, Session>(board.lanes.filter((l) => !l.device).flatMap((l) => l.sessions.map((s) => [s.key, s] as const))), [board.lanes]);

  return (
    <Dialog
      className="ca-usage"
      title="Usage"
      onClose={onClose}
      aside={<span className="ca-usage-asof">{report ? (readNote(report) ?? (report.asOf ? `Read ${agoPhrase(report.asOf, now)}` : 'Reading…')) : ''}</span>}
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={() => onAccounts(account ?? undefined)}>
            Plan prices…
          </Button>
          <Button variant="primary" size="sm" onClick={onClose}>
            Done
          </Button>
        </>
      }
    >
      <div className="ca-dialog-body ca-usage-body">
        <div className="ca-usage-filters">
          <Seg options={SPANS} value={days} onPick={setDays} label="Days" look="text" />
          <div className="ca-start-select ca-usage-account">
            <select aria-label="Account" value={account ?? ''} onChange={(e) => setAccount(e.target.value || null)}>
              <option value="">All accounts</option>
              {order.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </select>
            <Icon name="chevron-down" size={12} />
          </div>
        </div>
        {error && !report ? (
          <p className="ca-form-error" role="alert">
            {error}
          </p>
        ) : !report ? (
          <p className="ca-empty">Reading what your sessions used…</p>
        ) : (
          <div className={cx('ca-usage-report', loading && 'is-loading')}>
            <Stats report={report} onAccounts={() => onAccounts(account ?? undefined)} />
            <Charts days={report.days} order={order} />
            <div className="ca-usage-by">
              <span className="ca-usage-by-label" id="ca-usage-by">
                By
              </span>
              <Seg options={BYS} value={by} onPick={setBy} labelledBy="ca-usage-by" look="text" />
            </div>
            <Table report={report} by={by} order={order} sessions={sessions} lanes={board.lanes} onAccount={(a) => setAccount(a)} onShowSession={onShowSession} />
            <Notes report={report} />
          </div>
        )}
      </div>
    </Dialog>
  );
}

function Stats({ report: r, onAccounts }: { report: TokenReport; onAccounts: () => void }) {
  const plans = r.paying.reduce((s, p) => s + p.plan, 0);
  const extra = r.paying.reduce((s, p) => s + p.extra, 0);
  const byToken = r.paying.reduce((s, p) => s + p.tokens, 0);
  const priced = r.paying.some((p) => (p.monthly ?? 0) > 0 || p.perToken);
  const parts = [plans > 0 && `plans ${money(plans)}`, extra > 0 && `extra usage ${money(extra)}`, byToken > 0 && `by the token ${money(byToken)}`].filter(Boolean);
  const x = leverage(r.total.api, r.paid);
  const total = totalTokens(r.total.tokens);
  return (
    <div className="ca-usage-stats">
      <div className="ca-usage-stat">
        <span className="ca-usage-stat-label">At API prices</span>
        <span className="ca-usage-stat-value">{money(r.total.api)}</span>
        <span className="ca-usage-stat-note">what the work would have cost</span>
      </div>
      <div className="ca-usage-stat">
        <span className="ca-usage-stat-label">Paid</span>
        <span className="ca-usage-stat-value">{r.paid > 0 || priced ? money(r.paid) : '—'}</span>
        {parts.length > 0 ? (
          <span className="ca-usage-stat-note">{parts.join(' · ')}</span>
        ) : (
          <button type="button" className="ca-linkbtn ca-usage-stat-note" onClick={onAccounts}>
            Set what each plan costs
          </button>
        )}
      </div>
      <div className="ca-usage-stat">
        <span className="ca-usage-stat-label">Leverage</span>
        <span className="ca-usage-stat-value">{x ?? '—'}</span>
        <span className="ca-usage-stat-note">{x ? 'API prices for each dollar paid' : 'once something is paid'}</span>
      </div>
      <div className="ca-usage-stat">
        <span className="ca-usage-stat-label">Tokens</span>
        <span className="ca-usage-stat-value">{tokenCount(total)}</span>
        <span className="ca-usage-stat-note">{total > 0 ? `${Math.round(cacheShare(r.total.tokens) * 100)}% read from cache` : 'none yet'}</span>
      </div>
    </div>
  );
}

/** The charts' width in their own units: they scale to the dialog, both the same, so their days
 *  line up. */
const W = 800;
const LEFT = 48;
const RIGHT = 12;
const PLOT = W - LEFT - RIGHT;
const AXIS = 22;

function xOf(i: number, n: number): number {
  return LEFT + ((i + 0.5) * PLOT) / Math.max(n, 1);
}

/** About six of the days, the last always among them, for the x axis. */
function labelled(n: number): Set<number> {
  const step = Math.max(1, Math.ceil(n / 6));
  const out = new Set<number>();
  for (let i = n - 1; i >= 0; i -= step) out.add(i);
  return out;
}

function Charts({ days, order }: { days: SpendDay[]; order: string[] }) {
  const [hover, setHover] = useState<{ i: number; chart: 'total' | 'daily' } | null>(null);
  const run = useMemo(() => cumulative(days), [days]);
  const accounts = useMemo(() => order.filter((a) => days.some((d) => (d.api[a] ?? 0) > 0)), [order, days]);
  const other = days.some((d) => Object.keys(d.api).some((a) => !order.slice(0, 6).includes(a) && d.api[a] > 0));
  return (
    <div className="ca-usage-charts" onMouseLeave={() => setHover(null)}>
      <figure className="ca-usage-chart">
        <figcaption className="ca-usage-legend">
          <span className="ca-usage-chart-title">Running total</span>
          <span className="ca-usage-key">
            <i className="ca-usage-linekey is-api" />
            At API prices
          </span>
          <span className="ca-usage-key">
            <i className="ca-usage-linekey is-paid" />
            Paid
          </span>
        </figcaption>
        <Running run={run} hover={hover?.i ?? null} tip={hover?.chart === 'total'} onHover={(i) => setHover(i === null ? null : { i, chart: 'total' })} />
      </figure>
      <figure className="ca-usage-chart">
        <figcaption className="ca-usage-legend">
          <span className="ca-usage-chart-title">Each day at API prices</span>
          {accounts.slice(0, 6).map((a) => (
            <span key={a} className="ca-usage-key">
              <i className="ca-usage-swatch" style={{ background: seriesColor(a, order) }} />
              {a}
            </span>
          ))}
          {other && (
            <span className="ca-usage-key">
              <i className="ca-usage-swatch" style={{ background: 'var(--line-strong)' }} />
              Other
            </span>
          )}
        </figcaption>
        <Daily days={days} order={order} hover={hover?.i ?? null} tip={hover?.chart === 'daily'} onHover={(i) => setHover(i === null ? null : { i, chart: 'daily' })} />
      </figure>
    </div>
  );
}

/** The hit area over a chart: the day under the pointer, nearest by its x. */
function HitArea({ n, top, height, onHover }: { n: number; top: number; height: number; onHover: (i: number | null) => void }) {
  return (
    <rect
      x={LEFT}
      y={top}
      width={PLOT}
      height={height}
      fill="transparent"
      onPointerMove={(e) => {
        const box = e.currentTarget.getBoundingClientRect();
        const at = ((e.clientX - box.left) / box.width) * n;
        onHover(Math.min(n - 1, Math.max(0, Math.floor(at))));
      }}
      onPointerLeave={() => onHover(null)}
    />
  );
}

function YAxis({ marks, y, top }: { marks: number[]; y: (v: number) => number; top: number }) {
  return (
    <g className="ca-usage-axis">
      {marks.map((m) => (
        <g key={m}>
          <line x1={LEFT} x2={W - RIGHT} y1={y(m)} y2={y(m)} className="ca-usage-grid" />
          <text x={LEFT - 8} y={Math.max(top + 8, y(m) + 4)} textAnchor="end">
            {moneyTick(m)}
          </text>
        </g>
      ))}
    </g>
  );
}

function XAxis({ days, y }: { days: { day: string }[]; y: number }) {
  const shown = labelled(days.length);
  return (
    <g className="ca-usage-axis">
      {days.map((d, i) =>
        shown.has(i) ? (
          <text key={d.day} x={xOf(i, days.length)} y={y} textAnchor={i === days.length - 1 ? 'end' : 'middle'} dx={i === days.length - 1 ? 6 : 0}>
            {dayLabel(d.day)}
          </text>
        ) : null,
      )}
    </g>
  );
}

function Tip({ x, children }: { x: number; children: React.ReactNode }) {
  // Kept inside the chart: past its middle it opens to the left of the day.
  const left = x / W > 0.5;
  return (
    <div className={cx('ca-usage-tip', left && 'is-left')} style={{ left: `${(x / W) * 100}%` }} role="status">
      {children}
    </div>
  );
}

const RUN_H = 150;

function Running({ run, hover, tip, onHover }: { run: { day: string; api: number; paid: number }[]; hover: number | null; tip: boolean; onHover: (i: number | null) => void }) {
  const n = run.length;
  const top = 8;
  const base = RUN_H;
  const max = Math.max(1, ...run.map((r) => Math.max(r.api, r.paid)));
  const marks = ticks(max);
  const y = (v: number) => base - ((base - top) * v) / marks[marks.length - 1];
  const path = (pick: (r: { api: number; paid: number }) => number) => run.map((r, i) => `${i === 0 ? 'M' : 'L'}${xOf(i, n).toFixed(1)},${y(pick(r)).toFixed(1)}`).join(' ');
  const last = run[n - 1];
  const h = hover !== null ? run[hover] : null;
  return (
    <div className="ca-usage-plot">
      <svg viewBox={`0 0 ${W} ${RUN_H + AXIS}`} role="img" aria-label={last ? `Running total: ${money(last.api)} at API prices, ${money(last.paid)} paid` : 'Running total'}>
        <YAxis marks={marks} y={y} top={top} />
        {n > 0 && (
          <>
            <path d={`${path((r) => r.api)} L${xOf(n - 1, n).toFixed(1)},${base} L${xOf(0, n).toFixed(1)},${base} Z`} className="ca-usage-area" />
            <path d={path((r) => r.paid)} className="ca-usage-line is-paid" />
            <path d={path((r) => r.api)} className="ca-usage-line is-api" />
            <circle cx={xOf(n - 1, n)} cy={y(last.paid)} r={4} className="ca-usage-dot is-paid" />
            <circle cx={xOf(n - 1, n)} cy={y(last.api)} r={4} className="ca-usage-dot is-api" />
          </>
        )}
        {h && hover !== null && (
          <g className="ca-usage-cross">
            <line x1={xOf(hover, n)} x2={xOf(hover, n)} y1={top} y2={base} />
            <circle cx={xOf(hover, n)} cy={y(h.paid)} r={4} className="ca-usage-dot is-paid" />
            <circle cx={xOf(hover, n)} cy={y(h.api)} r={4} className="ca-usage-dot is-api" />
          </g>
        )}
        <XAxis days={run} y={RUN_H + AXIS - 6} />
        <HitArea n={n} top={top} height={base - top} onHover={onHover} />
      </svg>
      {tip && h && hover !== null && (
        <Tip x={xOf(hover, n)}>
          <span className="ca-usage-tip-day">By {dayLabel(h.day)}</span>
          <span className="ca-usage-tip-row">
            <i className="ca-usage-linekey is-api" />
            <b>{money(h.api)}</b> at API prices
          </span>
          <span className="ca-usage-tip-row">
            <i className="ca-usage-linekey is-paid" />
            <b>{money(h.paid)}</b> paid
          </span>
        </Tip>
      )}
    </div>
  );
}

const DAY_H = 110;

function Daily({ days, order, hover, tip, onHover }: { days: SpendDay[]; order: string[]; hover: number | null; tip: boolean; onHover: (i: number | null) => void }) {
  const n = days.length;
  const top = 8;
  const base = DAY_H;
  const stacks = days.map((d) => stackOf(d, order));
  const max = Math.max(1, ...stacks.map((s) => s.reduce((a, b) => a + b.api, 0)));
  const marks = ticks(max, 3);
  const scale = (base - top) / marks[marks.length - 1];
  const width = Math.min(24, (PLOT / Math.max(n, 1)) * 0.62);
  const h = hover !== null ? stacks[hover] : null;
  return (
    <div className="ca-usage-plot">
      <svg viewBox={`0 0 ${W} ${DAY_H + AXIS}`} role="img" aria-label="Each day's cost at API prices, by account">
        <YAxis marks={marks} y={(v) => base - v * scale} top={top} />
        {stacks.map((stack, i) => {
          let at = base;
          const x = xOf(i, n) - width / 2;
          return (
            <g key={days[i].day} className={cx('ca-usage-bar', hover === i && 'is-hover')}>
              {stack.map((s, k) => {
                const tall = s.api * scale;
                // Each segment above the first leaves a two-pixel gap below it.
                const gap = k > 0 ? Math.min(2, tall / 2) : 0;
                const y = at - tall;
                at = y;
                const height = Math.max(0, tall - gap);
                const topmost = k === stack.length - 1;
                const r = topmost ? Math.min(4, height / 2, width / 2) : 0;
                return <path key={s.account} d={bar(x, y, width, height, r)} fill={s.account === 'Other' ? 'var(--line-strong)' : seriesColor(s.account, order)} />;
              })}
            </g>
          );
        })}
        <XAxis days={days} y={DAY_H + AXIS - 6} />
        <HitArea n={n} top={top} height={base - top} onHover={onHover} />
      </svg>
      {tip && h && hover !== null && (
        <Tip x={xOf(hover, n)}>
          <span className="ca-usage-tip-day">{dayLabel(days[hover].day)}</span>
          {h.length === 0 && <span className="ca-usage-tip-row">Nothing used</span>}
          {[...h].reverse().map((s) => (
            <span key={s.account} className="ca-usage-tip-row">
              <i className="ca-usage-swatch" style={{ background: s.account === 'Other' ? 'var(--line-strong)' : seriesColor(s.account, order) }} />
              <b>{money(s.api)}</b> {s.account}
            </span>
          ))}
          {days[hover].paid > 0 && (
            <span className="ca-usage-tip-row">
              <i className="ca-usage-linekey is-paid" />
              <b>{money(days[hover].paid)}</b> paid
            </span>
          )}
        </Tip>
      )}
    </div>
  );
}

/** A bar's outline: square at the baseline, its top corners rounded by `r`. */
function bar(x: number, y: number, w: number, h: number, r: number): string {
  if (h <= 0) return '';
  return `M${x},${y + h} V${y + r} Q${x},${y} ${x + r},${y} H${x + w - r} Q${x + w},${y} ${x + w},${y + r} V${y + h} Z`;
}

interface TableProps {
  report: TokenReport;
  by: By;
  order: string[];
  sessions: Map<string, Session>;
  lanes: Board['lanes'];
  onAccount: (account: string) => void;
  onShowSession: (key: string) => void;
}

interface Line {
  key: string;
  name: string;
  note?: string;
  swatch?: string;
  spent: Spent;
  act?: () => void;
  actTitle?: string;
}

function Table({ report: r, by, order, sessions, lanes, onAccount, onShowSession }: TableProps) {
  const lines: Line[] = useMemo(() => {
    switch (by) {
      case 'account':
        return r.accounts.map((s) => ({ key: s.key, name: s.key, swatch: seriesColor(s.key, order), spent: s, act: () => onAccount(s.key), actTitle: `Show ${s.key} alone` }));
      case 'model':
        return r.models.map((s) => ({ key: s.key, name: s.key || 'Unknown model', spent: s }));
      case 'project':
        return byProject(r.folders, lanes).map((s) => ({ key: s.key, name: s.key, spent: s }));
      case 'session':
        return r.sessions.map((s) => {
          const on = sessions.has(s.key);
          return { key: s.key, name: sessionLabel(s, sessions), note: [s.account, s.folder && s.folder.split('/').pop()].filter(Boolean).join(' · '), spent: s, act: on ? () => onShowSession(s.key) : undefined, actTitle: 'Show the session' };
        });
      case 'day':
        return [...r.days].reverse().map((d) => {
          const api = Object.values(d.api).reduce((a, b) => a + b, 0);
          const spent: Spent = { key: d.day, api, paid: d.paid, tokens: { input: 0, cacheRead: 0, cacheWrite: 0, output: 0, reasoning: 0 }, calls: 0, unpriced: 0, lastAt: 0 };
          return { key: d.day, name: dayLabel(d.day), spent: { ...spent, tokens: { ...spent.tokens, input: d.tokens } } };
        });
    }
  }, [r, by, order, sessions, lanes, onAccount, onShowSession]);
  const top = Math.max(1e-9, ...lines.map((l) => l.spent.api));
  if (lines.length === 0) return <p className="ca-empty">No session used anything in these days.</p>;
  return (
    <table className="ca-usage-table">
      <thead>
        <tr>
          <th scope="col">{BYS.find((b) => b.id === by)?.label}</th>
          <th scope="col" className="is-num">
            Tokens
          </th>
          <th scope="col" className="ca-usage-share-col" aria-label="Against the costliest" />
          <th scope="col" className="is-num">
            At API prices
          </th>
          <th scope="col" className="is-num">
            Paid
          </th>
        </tr>
      </thead>
      <tbody>
        {lines.map((l) => (
          <tr key={l.key}>
            <th scope="row">
              <span className="ca-usage-name">
                {l.swatch && <i className="ca-usage-swatch" style={{ background: l.swatch }} />}
                {l.act ? (
                  <button type="button" className="ca-linkbtn" title={l.actTitle} onClick={l.act}>
                    {l.name}
                  </button>
                ) : (
                  <span>{l.name}</span>
                )}
              </span>
              {l.note && <span className="ca-usage-note">{l.note}</span>}
            </th>
            <td className="is-num" title={tokensTitle(l.spent)}>
              {tokenCount(totalTokens(l.spent.tokens))}
            </td>
            <td className="ca-usage-share-col">
              {l.spent.api > 0 && <span className="ca-usage-share" style={{ width: `${(l.spent.api / top) * 100}%` }} />}
            </td>
            <td className="is-num" title={l.spent.unpriced > 0 ? `${tokenCount(l.spent.unpriced)} tokens of models with no price aren't counted` : undefined}>
              {money(l.spent.api)}
              {l.spent.unpriced > 0 && <span className="ca-usage-flag">+ unpriced</span>}
            </td>
            <td className="is-num">{l.spent.paid > 0 ? money(l.spent.paid) : <span className="ca-usage-none">—</span>}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function tokensTitle(s: Spent): string | undefined {
  const t = s.tokens;
  if (t.cacheRead + t.cacheWrite + t.output === 0) return undefined;
  return [`${tokenCount(t.input)} input`, `${tokenCount(t.cacheRead)} read from cache`, `${tokenCount(t.cacheWrite)} written to cache`, `${tokenCount(t.output)} output${t.reasoning ? `, ${tokenCount(t.reasoning)} of it thinking` : ''}`, `${s.calls.toLocaleString('en-US')} calls`].join('\n');
}

function Notes({ report: r }: { report: TokenReport }) {
  const extras = r.paying.filter((p) => p.extraNow);
  return (
    <div className="ca-usage-notes">
      {extras.map((p) => (
        <p key={p.account}>
          {p.account}’s extra usage this month: {money(p.extraNow![0])}
          {p.extraNow![1] != null ? ` of ${money(p.extraNow![1])}` : ''}, as Claude reports it.
        </p>
      ))}
      {r.unpriced.length > 0 && (
        <p>
          No price for {r.unpriced.join(', ')}: their tokens are counted and not priced. A price for one goes in config.toml as <code>[prices."{r.unpriced[0]}"]</code>, dollars per million tokens.
        </p>
      )}
      {r.untracked.length > 0 && <p>{r.untracked.join(', ')} keep no record of tokens Codealong can read, so they aren’t here.</p>}
      <p>API prices are Anthropic’s and OpenAI’s list prices per token. What a plan costs is counted a day’s share at a time.</p>
    </div>
  );
}
