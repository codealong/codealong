// The Usage window's Your part and Sessions (UsageDialog.tsx shows them once `[experimental]
// analytics` is on): what came in to your sessions and who from, what your prompts did with
// what the agents had said, how sessions began, and how the ones you were done with stood
// (analytics.rs). The words and sums are analytics.ts's, the charts' pieces UsageChart.tsx's,
// and what the same days cost by who began the work comes with the token report.

import { useMemo, useState } from 'react';
import { BEGAN, byKey, cameIn, count, countOf, countTicks, HANDS, markLines, partByProject, percent, responses, secondHandCost, secondHandSessions, type Segment, segments, stanceStack, STANCES, stoodStack, sum, yours } from '../analytics';
import { agoPhrase } from '../format';
import { dayLabel, laneOf, money } from '../spend';
import type { Analytics, Board, PartDay, PartRow, Session, TokenReport } from '../types';
import { cx, Seg } from './primitives';
import { AXIS, bar, HitArea, PLOT, Tip, W, XAxis, xOf, YAxis } from './UsageChart';

interface Props {
  analytics: Analytics;
  report: TokenReport | null;
  lanes: Board['lanes'];
  sessions: Map<string, Session>;
  now: number;
  onShowSession: (key: string) => void;
}

type By = 'account' | 'project' | 'session';

const BYS: { id: By; label: string }[] = [
  { id: 'account', label: 'Account' },
  { id: 'project', label: 'Project' },
  { id: 'session', label: 'Session' },
];

/** How many of a list a window shows before it's asked for the rest. */
const SHOWN = 12;

/** A session's name in a list: its title as the ledger last had it, else on the board, else the
 *  start of its id. */
function titleOf(key: string, title: string | undefined, sessions: Map<string, Session>): string {
  return title || sessions.get(key)?.title || (key.split(':').pop()?.slice(0, 8) ?? key);
}

/** A session's name, as a button to it while the board lists it. */
function SessionName({ id, title, sessions, onShow }: { id: string; title?: string; sessions: Map<string, Session>; onShow: (key: string) => void }) {
  const name = titleOf(id, title, sessions);
  return sessions.has(id) ? (
    <button type="button" className="ca-linkbtn" title="Show the session" onClick={() => onShow(id)}>
      {name}
    </button>
  ) : (
    <span>{name}</span>
  );
}

function Stat({ label, value, note }: { label: string; value: string; note: string }) {
  return (
    <div className="ca-usage-stat">
      <span className="ca-usage-stat-label">{label}</span>
      <span className="ca-usage-stat-value">{value}</span>
      <span className="ca-usage-stat-note">{note}</span>
    </div>
  );
}

/** One bar cut into its series, each as wide as its share, with what each is under it. */
function Split({ parts, label, value }: { parts: Segment[]; label: string; value?: (s: Segment) => string }) {
  const all = sum(parts.map((p) => p.n));
  if (all <= 0) return null;
  return (
    <div className="ca-usage-split">
      <div className="ca-usage-split-bar" role="img" aria-label={`${label}: ${parts.map((p) => `${p.label} ${percent(p.n, all)}`).join(', ')}`}>
        {parts.map((p) => (
          <i key={p.id} style={{ flexGrow: p.n, background: p.color }} title={`${p.label}: ${value ? value(p) : count(p.n)} (${percent(p.n, all)})`} />
        ))}
      </div>
      <div className="ca-usage-legend">
        {parts.map((p) => (
          <span key={p.id} className="ca-usage-key">
            <i className="ca-usage-swatch" style={{ background: p.color }} />
            {p.label}
            <b>{value ? value(p) : count(p.n)}</b>
            <span className="ca-usage-none">{percent(p.n, all)}</span>
          </span>
        ))}
      </div>
    </div>
  );
}

const BARS_H = 120;

/** Each day's prompts of yours, stacked by what they did. */
function StanceDays({ days }: { days: PartDay[] }) {
  const [hover, setHover] = useState<number | null>(null);
  const n = days.length;
  const top = 8;
  const stacks = useMemo(() => days.map(stanceStack), [days]);
  const marks = countTicks(Math.max(1, ...stacks.map((s) => sum(s.map((x) => x.n)))));
  const scale = (BARS_H - top) / marks[marks.length - 1];
  const width = Math.min(24, (PLOT / Math.max(n, 1)) * 0.62);
  const shown = STANCES.filter((s) => stacks.some((stack) => stack.some((x) => x.id === s.id)));
  const h = hover !== null ? stacks[hover] : null;
  return (
    <figure className="ca-usage-chart" onMouseLeave={() => setHover(null)}>
      <figcaption className="ca-usage-legend">
        <span className="ca-usage-chart-title">What your prompts did, each day</span>
        {shown.map((s) => (
          <span key={s.id} className="ca-usage-key">
            <i className="ca-usage-swatch" style={{ background: s.color }} />
            {s.label}
          </span>
        ))}
      </figcaption>
      <div className="ca-usage-plot">
        <svg viewBox={`0 0 ${W} ${BARS_H + AXIS}`} role="img" aria-label="Each day's prompts of yours, by what they did with what the agent had said">
          <YAxis marks={marks} y={(v) => BARS_H - v * scale} top={top} format={count} />
          {stacks.map((stack, i) => {
            let at = BARS_H;
            const x = xOf(i, n) - width / 2;
            return (
              <g key={days[i].day} className={cx('ca-usage-bar', hover === i && 'is-hover')}>
                {stack.map((s, k) => {
                  const tall = s.n * scale;
                  // Each segment above the first leaves a two-pixel gap below it.
                  const gap = k > 0 ? Math.min(2, tall / 2) : 0;
                  const y = at - tall;
                  at = y;
                  const height = Math.max(0, tall - gap);
                  const r = k === stack.length - 1 ? Math.min(4, height / 2, width / 2) : 0;
                  return <path key={s.id} d={bar(x, y, width, height, r)} fill={s.color} />;
                })}
              </g>
            );
          })}
          <XAxis days={days} y={BARS_H + AXIS - 6} />
          <HitArea n={n} top={top} height={BARS_H - top} onHover={setHover} />
        </svg>
        {h && hover !== null && (
          <Tip x={xOf(hover, n)}>
            <span className="ca-usage-tip-day">{dayLabel(days[hover].day)}</span>
            {h.length === 0 && <span className="ca-usage-tip-row">No prompts of yours</span>}
            {[...h].reverse().map((s) => (
              <span key={s.id} className="ca-usage-tip-row">
                <i className="ca-usage-swatch" style={{ background: s.color }} />
                <b>{count(s.n)}</b> {s.label}
              </span>
            ))}
          </Tip>
        )}
      </div>
    </figure>
  );
}

/** A row's prompts of yours as one thin bar, cut by what they did, as long as its share of the
 *  most prompted row. */
function Mix({ row, top }: { row: PartRow; top: number }) {
  const parts = segments(STANCES, row.stances);
  const n = yours(row);
  if (n <= 0 || parts.length === 0) return null;
  return (
    <span className="ca-usage-mix" style={{ width: `${(n / top) * 100}%` }} title={parts.map((p) => `${p.label}: ${count(p.n)}`).join('\n')}>
      {parts.map((p) => (
        <i key={p.id} style={{ flexGrow: p.n, background: p.color }} />
      ))}
    </span>
  );
}

function PartTable({ analytics, by, lanes, sessions, onShowSession }: Props & { by: By }) {
  const part = analytics.part;
  const rows: PartRow[] = useMemo(() => {
    switch (by) {
      case 'account':
        return part.accounts;
      case 'project':
        return partByProject(part.folders, (f) => laneOf(f, lanes)?.project ?? '');
      case 'session':
        return part.sessions;
    }
  }, [part, by, lanes]);
  const top = Math.max(1, ...rows.map(yours));
  if (rows.length === 0) return <p className="ca-empty">Nothing came in to a session in these days.</p>;
  return (
    <table className="ca-usage-table">
      <thead>
        <tr>
          <th scope="col">{BYS.find((b) => b.id === by)?.label}</th>
          <th scope="col" className="is-num">
            Your prompts
          </th>
          <th scope="col" className="ca-usage-share-col">
            What they did
          </th>
          <th scope="col" className="is-num" title="Turns that count as yours: the ones you began, and what each session went on to do by itself after">
            Your turns
          </th>
          <th scope="col" className="is-num">
            Agents’
          </th>
          <th scope="col" className="is-num">
            Codealong’s
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.key}>
            <th scope="row">
              <span className="ca-usage-name">{by === 'session' ? <SessionName id={r.key} title={r.title} sessions={sessions} onShow={onShowSession} /> : <span>{r.key || 'Unknown'}</span>}</span>
              {by === 'session' && <span className="ca-usage-note">{[r.account, r.folder && r.folder.split('/').pop(), r.began && r.began !== 'you' && BEGAN.find((b) => b.id === r.began)?.label].filter(Boolean).join(' · ')}</span>}
            </th>
            <td className="is-num" title={`${count(cameIn(r))} turns in all`}>
              {count(yours(r))}
            </td>
            <td className="ca-usage-share-col">
              <Mix row={r} top={top} />
            </td>
            <td className="is-num">{r.counts.you ? count(r.counts.you) : <span className="ca-usage-none">—</span>}</td>
            <td className="is-num">{r.counts.agent ? count(r.counts.agent) : <span className="ca-usage-none">—</span>}</td>
            <td className="is-num">{r.counts.codealong ? count(r.counts.codealong) : <span className="ca-usage-none">—</span>}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** A list that shows its first `SHOWN` until it's asked for the rest. */
function More<T>({ items, children }: { items: T[]; children: (item: T) => React.ReactNode }) {
  const [all, setAll] = useState(false);
  const shown = all ? items : items.slice(0, SHOWN);
  return (
    <>
      <ul className="ca-usage-list">{shown.map(children)}</ul>
      {items.length > SHOWN && (
        <button type="button" className="ca-linkbtn ca-usage-more" onClick={() => setAll(!all)}>
          {all ? 'Show fewer' : `Show all ${count(items.length)}`}
        </button>
      )}
    </>
  );
}

/** Your part: what came in to your sessions and who from, and what your prompts did. */
export function YourPart(props: Props) {
  const { analytics, report, sessions, now, onShowSession } = props;
  const [by, setBy] = useState<By>('account');
  const part = analytics.part;
  const prompts = sum(part.stances.map((s) => s.n));
  const turns = sum(part.hands.map((h) => h.n));
  const went = responses(part.stances);
  const second = secondHandCost(report?.hands);
  const itself = countOf(part.sources, 'itself');
  const cost = useMemo(() => Object.fromEntries((report?.hands ?? []).map((h) => [h.key, h.api])), [report]);
  const lines = markLines(part);
  if (turns === 0) {
    return <p className="ca-empty">{analytics.filling || !analytics.asOf ? 'Reading what came in to your sessions…' : 'Nothing came in to a session in these days.'}</p>;
  }
  return (
    <>
      <div className="ca-usage-stats">
        <Stat label="Your prompts" value={count(prompts)} note={`about ${count(Math.round(prompts / Math.max(1, part.days.length)))} a day`} />
        <Stat label="Turns you began" value={percent(countOf(part.hands, 'you'), turns)} note={`${count(countOf(part.hands, 'you'))} of ${count(turns)}`} />
        <Stat label="Second hand" value={second ? percent(second.second, second.known) : '—'} note={second ? `${money(second.second)} of ${money(second.known)} at API prices` : 'of the cost, once it’s read'} />
        <Stat label="Went along" value={went.all > 0 ? percent(went.agreed, went.all) : '—'} note={went.all > 0 ? `of the ${count(went.all)} that answered a reply` : 'once your prompts are read'} />
      </div>
      <StanceDays days={part.days} />
      <div className="ca-usage-chart">
        <div className="ca-usage-legend">
          <span className="ca-usage-chart-title">Who began the work</span>
          <span>turns, then what they used at API prices</span>
        </div>
        <Split parts={segments(HANDS, byKey(part.hands))} label="Turns, by who began them" />
        <Split parts={segments(HANDS, cost)} label="Cost at API prices, by who began the turn" value={(s) => money(s.n)} />
        <p className="ca-usage-fine">
          {itself > 0 && `The session itself began ${count(itself)} of them (a background task reporting back, a wakeup it set): each counts with the turn it follows. `}
          {report?.subagents && report.subagents.api > 0 && `Subagents made ${money(report.subagents.api)} of the cost.`}
        </p>
      </div>
      {lines.length > 0 && (
        <div className="ca-usage-chart">
          <span className="ca-usage-chart-title">Of your prompts, for sure</span>
          <ul className="ca-usage-marks">
            {lines.map((l) => (
              <li key={l}>{l}</li>
            ))}
          </ul>
        </div>
      )}
      <div className="ca-usage-by">
        <span className="ca-usage-by-label" id="ca-usage-part-by">
          By
        </span>
        <Seg options={BYS} value={by} onPick={setBy} labelledBy="ca-usage-part-by" look="text" />
      </div>
      <PartTable {...props} by={by} />
      {part.prompts.length > 0 && (
        <div className="ca-usage-chart">
          <span className="ca-usage-chart-title">Where you steered or redirected</span>
          <More items={part.prompts}>
            {(p) => {
              const s = STANCES.find((x) => x.id === p.stance);
              return (
                <li key={p.id}>
                  <span className="ca-usage-name">
                    {s && <i className="ca-usage-swatch" style={{ background: s.color }} title={s.label} />}
                    <span>{p.opening || '(no words)'}</span>
                  </span>
                  <span className="ca-usage-note">
                    {[s?.label, p.afterStop ? 'after you stopped its turn' : p.mid ? 'while it worked' : '', agoPhrase(p.at, now)].filter(Boolean).join(' · ')} · <SessionName id={p.session} title={p.title} sessions={sessions} onShow={onShowSession} />
                  </span>
                </li>
              );
            }}
          </More>
        </div>
      )}
      <div className="ca-usage-notes">
        {part.unread > 0 &&
          (part.reading ? (
            <p>{count(part.unread)} of your prompts wait on a read; Jev reads a few each minute.</p>
          ) : (
            <p>
              {count(part.unread)} of your prompts haven’t been read for what they did. Settings › Experimental can have Jev read them, or an agent can: <code>codealong analytics prompts</code>.
            </p>
          ))}
        <p>What a prompt did is a read of it against the reply before it, by Jev or an agent; everything else here is from your harnesses’ own records.</p>
        {part.since && <p>Nothing is known of what came in before {new Date(part.since).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })}.</p>}
      </div>
    </>
  );
}

const WHAT: Record<string, string> = { done: 'Marked done', archive: 'Archived', 'close-idle': 'Closed while idle' };

/** Sessions: how they began, and how the ones you were done with stood. */
export function SessionsPart({ analytics, report, sessions, now, onShowSession }: Props) {
  const o = analytics.outcomes;
  const began = segments(BEGAN, byKey(o.began));
  const all = sum(began.map((b) => b.n));
  const second = secondHandSessions(o.began);
  const cost = useMemo(() => Object.fromEntries((report?.origins ?? []).map((s) => [s.key, s.api])), [report]);
  const top = Math.max(1, ...began.map((b) => b.n));
  return (
    <>
      <div className="ca-usage-stats">
        <Stat label="Sessions" value={count(all)} note="active in these days" />
        <Stat label="Second hand" value={percent(second.second, second.known)} note={`${count(second.second)} begun by agents or Codealong`} />
        <Stat label="Marked done" value={count(o.left)} note={o.left > 0 ? `${count(o.finished + o.lookedFinished)} of them finished` : 'none yet'} />
        <Stat label="Left unfinished" value={count(o.unfinished)} note={o.reopened > 0 ? `${count(o.reopened)} taken up again after` : 'of the ones you marked done'} />
      </div>
      {began.length === 0 ? (
        <p className="ca-empty">{analytics.filling || !analytics.asOf ? 'Reading where your sessions came from…' : 'No session was active in these days.'}</p>
      ) : (
        <table className="ca-usage-table">
          <thead>
            <tr>
              <th scope="col">Begun by</th>
              <th scope="col" className="is-num">
                Sessions
              </th>
              <th scope="col" className="ca-usage-share-col" aria-label="Against the most" />
              <th scope="col" className="is-num">
                At API prices
              </th>
            </tr>
          </thead>
          <tbody>
            {began.map((b) => (
              <tr key={b.id}>
                <th scope="row">
                  <span className="ca-usage-name">
                    <i className="ca-usage-swatch" style={{ background: b.color }} />
                    <span>{b.label}</span>
                  </span>
                </th>
                <td className="is-num">
                  {count(b.n)} <span className="ca-usage-none">{percent(b.n, all)}</span>
                </td>
                <td className="ca-usage-share-col">
                  <span className="ca-usage-share" style={{ width: `${(b.n / top) * 100}%`, background: b.color }} />
                </td>
                <td className="is-num">{cost[b.id] ? money(cost[b.id]) : <span className="ca-usage-none">—</span>}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="ca-usage-chart">
        <span className="ca-usage-chart-title">How the ones you were done with stood</span>
        {o.left === 0 ? (
          <p className="ca-usage-fine">Nothing marked done or archived in these days{o.since ? '' : ' since this was turned on'}.</p>
        ) : (
          <Split parts={stoodStack(o)} label="The sessions you marked done, by how they stood" />
        )}
        {o.byAgents > 0 && <p className="ca-usage-fine">Agents marked {count(o.byAgents)} more done from the command.</p>}
      </div>
      {o.list.length > 0 && (
        <div className="ca-usage-chart">
          <span className="ca-usage-chart-title">Left unfinished</span>
          <More items={o.list}>
            {(l) => (
              <li key={`${l.session}-${l.at}`}>
                <span className="ca-usage-name">
                  <SessionName id={l.session} title={l.title} sessions={sessions} onShow={onShowSession} />
                </span>
                <span className="ca-usage-note">{[l.said, l.unread && 'you hadn’t read its last reply', WHAT[l.what] ?? l.what, agoPhrase(l.at, now), l.reopened && 'taken up again'].filter(Boolean).join(' · ')}</span>
              </li>
            )}
          </More>
        </div>
      )}
      {o.hanging.length > 0 && (
        <div className="ca-usage-chart">
          <span className="ca-usage-chart-title">Still open, with work left undone</span>
          <More items={o.hanging}>
            {(h) => (
              <li key={h.session}>
                <span className="ca-usage-name">
                  <SessionName id={h.session} title={h.title} sessions={sessions} onShow={onShowSession} />
                </span>
                <span className="ca-usage-note">
                  {h.said} · untouched since {agoPhrase(h.lastActivity, now)}
                </span>
              </li>
            )}
          </More>
        </div>
      )}
      <div className="ca-usage-notes">
        <p>A session is finished when a PR it worked on merged, a todo linked to it is done or its own status line says its change landed, and looks finished when only the read of its last reply says so. How one stood is noted as you mark it done or archive it.</p>
        {o.since && <p>Nothing was noted before {new Date(o.since).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })}.</p>}
      </div>
    </>
  );
}

/** Which of the Usage window's views is up. */
export type UsageView = 'cost' | 'part' | 'sessions';

export const VIEWS: { id: UsageView; label: string }[] = [
  { id: 'cost', label: 'Cost' },
  { id: 'part', label: 'Your part' },
  { id: 'sessions', label: 'Sessions' },
];
