// The Canvas view's cards: a worktree, session, todo or PR in two lines, small enough that a
// few dozen fit on screen and the shape of the work reads at a glance. Hovering one opens a
// popover with the rest: the whole headline, the last message, what it's linked to.

import { memo, type ReactNode } from 'react';
import { absolute, agoPhrase, ago, duration, tildePath } from '../format';
import { HARNESS_WORD, sessionPlace } from '../harness';
import { blockedBy, sectionOf, SECTION_WORD, snippet } from '../todos';
import type { Label, Lane, PrItem, Session, Todo } from '../types';
import { CARD_H, CARD_W, type Entities, type RefItem } from '../canvas/model';
import { BranchLabel, HarnessChip, Icon, LaneGlyph, PR_ICON, PR_WORD, PrChip, Rich, STATUS_WORD, StatusGlyph } from './primitives';
import { LabelChip, LabelDot, TodoGlyph } from './TodoBits';
import { DeviceBadge } from './DeviceBadge';

const cx = (...parts: (string | false | null | undefined)[]) => parts.filter(Boolean).join(' ');

/** What a card shows, looked up from the board; null when the board no longer has it. */
export type CardData =
  | { ref: 'worktree'; lane: Lane }
  | { ref: 'session'; lane: Lane; session: Session }
  | { ref: 'todo'; todo: Todo; labels: Label[]; sessions: { lane: Lane; session: Session }[]; lane: Lane | null }
  | { ref: 'pr'; pr: PrItem };

export function cardData(e: Entities, it: RefItem, labels: Map<string, Label>): CardData | null {
  switch (it.ref) {
    case 'worktree': {
      const lane = e.lanes.get(it.key);
      return lane ? { ref: 'worktree', lane } : null;
    }
    case 'session': {
      const p = e.sessions.get(it.key);
      return p ? { ref: 'session', lane: p.lane, session: p.session } : null;
    }
    case 'todo': {
      const todo = e.todos.get(it.key);
      if (!todo) return null;
      const sessions = todo.sessions.flatMap((k) => e.sessions.get(k) ?? []).sort((a, b) => b.session.lastActivity - a.session.lastActivity);
      const lane = todo.worktrees.map((id) => e.lanes.get(id)).find(Boolean) ?? sessions[0]?.lane ?? null;
      return { ref: 'todo', todo, labels: todo.labels.flatMap((id) => labels.get(id) ?? []), sessions, lane };
    }
    case 'pr': {
      const pr = e.prs.get(it.key);
      return pr ? { ref: 'pr', pr } : null;
    }
  }
}

/** What a session is up to, in a line: what it runs or asks while it's busy, else the start of
 *  its last message, which agents open with where the work stands. */
export function sessionLine(s: Session): string {
  const busy = s.status === 'working' || s.status === 'needs' || s.status === 'error';
  const detail = (s.detail ?? '').replace(/^\{(?:for|ago):-?\d+\}\s*·\s*/, '');
  const said = (s.excerpt ?? '').split('\n')[0];
  return (busy ? detail || said : said || detail) || s.headline;
}

/** Everything a card draws, as one string: a card redraws only when this changes. */
export function cardSig(d: CardData | null, now: number): string {
  if (!d) return 'gone';
  const minute = Math.floor(now / 60_000);
  switch (d.ref) {
    case 'worktree': {
      const l = d.lane;
      return `w|${l.glyph}|${l.bay}|${l.callsign}|${l.branch}|${l.headline}|${l.repo}|${l.pr?.number}|${l.pr?.state}|${l.pr?.checks.failed}|${l.pr?.checks.pending}|${l.lastActivity}|${l.device?.color}|${l.device?.online}|${minute}`;
    }
    case 'session': {
      const s = d.session;
      return `s|${s.status}|${s.title}|${sessionLine(s)}|${s.account}|${s.surface}|${s.lastActivity}|${s.statusSince}|${d.lane.callsign}|${d.lane.device?.color}|${d.lane.device?.online}|${minute}`;
    }
    case 'todo': {
      const t = d.todo;
      return `t|${t.status}|${t.title}|${!!t.waiting}|${t.labels.join()}|${d.labels.map((l) => l.color + l.name).join()}|${d.sessions.map((s) => s.session.key + s.session.status).join()}|${t.updatedAt}`;
    }
    case 'pr': {
      const p = d.pr;
      return `p|${p.state}|${p.title}|${p.checks.failed}|${p.checks.pending}|${p.checks.passed}|${p.merge}|${p.updatedAt}|${minute}`;
    }
  }
}

interface CardProps {
  item: RefItem;
  data: CardData | null;
  /** cardSig of data; the card redraws only when it, the position or the selection change. */
  sig: string;
  now: number;
  selected: boolean;
  /** Ties the card to the others it belongs with while one of them is hovered or selected. */
  related: boolean;
  /** Doesn't match the window's filter. */
  faded: boolean;
  /** Under the pointer. */
  hovered: boolean;
}

function Card({ item, data, now, selected, related, faded, hovered }: CardProps) {
  const style = { transform: `translate(${item.x}px, ${item.y}px)`, width: CARD_W, height: CARD_H[item.ref] };
  const cls = (kind: string, extra?: string | false) => cx('ca-cv-card', `is-${kind}`, selected && 'is-selected', related && 'is-related', hovered && 'is-hovered', faded && 'is-faded', extra);
  const id = `${item.ref}:${item.key}`;
  if (!data) {
    return (
      <div className={cls(item.ref, 'is-gone')} style={style} data-id={id}>
        <div className="ca-cv-l1">
          <Icon name={item.ref === 'worktree' ? 'folder' : item.ref === 'pr' ? 'pr' : item.ref === 'todo' ? 'list-check' : 'terminal'} size={12} />
          <span className="ca-cv-title">{item.title ?? item.key}</span>
        </div>
        <div className="ca-cv-l2">No longer on the board</div>
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
            {l.pr && <PrChip pr={l.pr} compact />}
          </div>
          <div className="ca-cv-l2">
            {l.branch && l.branch !== l.callsign ? (
              <BranchLabel branch={l.branch} />
            ) : (
              <span className="ca-cv-headline">
                <Rich text={l.headline} now={now} />
              </span>
            )}
            <span className="ca-cv-age">{ago(l.lastActivity, now)}</span>
          </div>
        </div>
      );
    }
    case 'session': {
      const s = data.session;
      return (
        <div className={cls('session', cx(`st-${s.status}`, data.lane.device && !data.lane.device.online && 'is-away'))} style={style} data-id={id}>
          <div className="ca-cv-l1">
            <StatusGlyph status={s.status} />
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
          </div>
        </div>
      );
    }
    case 'todo': {
      const t = data.todo;
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
            <span className="ca-cv-headline">
              {data.sessions.length > 0
                ? `${data.sessions.length} ${data.sessions.length === 1 ? 'session' : 'sessions'}${data.lane ? ` · ${data.lane.callsign}` : ''}`
                : data.lane
                  ? data.lane.callsign
                  : 'Todo'}
            </span>
          </div>
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
        </div>
      );
    }
  }
}

export const CanvasCard = memo(
  Card,
  (a, b) => a.sig === b.sig && a.item.x === b.item.x && a.item.y === b.item.y && a.selected === b.selected && a.related === b.related && a.faded === b.faded && a.hovered === b.hovered && a.item.title === b.item.title,
);

/* ---- The popover ---- */

function Row({ icon, children }: { icon: string; children: ReactNode }) {
  return (
    <div className="ca-cvp-row">
      <Icon name={icon} size={12} />
      <span>{children}</span>
    </div>
  );
}

function SessionLine({ s, now }: { s: Session; now: number }) {
  return (
    <div className="ca-cvp-session">
      <StatusGlyph status={s.status} size={10} />
      <span className="ca-cvp-session-title">{s.title}</span>
      <span className="ca-cvp-dim">{ago(s.lastActivity, now)}</span>
    </div>
  );
}

/** The popover's body for a card: everything the card leaves out. */
export function CardDetail({ item, data, now, terminal, todos }: { item: RefItem; data: CardData | null; now: number; terminal: string; todos: Todo[] }) {
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
                <PrChip pr={l.pr} />
                <span className="ca-cvp-clip">{l.pr.title}</span>
              </div>
            )}
            <Row icon="folder">{tildePath(l.path)}</Row>
            {l.git?.lastCommit && <Row icon="commit">{l.git.lastCommit}{l.git.lastCommitAt ? ` · ${agoPhrase(l.git.lastCommitAt, now)}` : ''}</Row>}
          </div>
          {own.length > 0 && (
            <div className="ca-cvp-list">
              {own.slice(0, 6).map((s) => (
                <SessionLine key={s.key} s={s} now={now} />
              ))}
              {own.length > 6 && <div className="ca-cvp-dim">and {own.length - 6} more</div>}
            </div>
          )}
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
          <div className="ca-cvp-hint">Double-click to open in {sessionPlace(s, terminal)}</div>
        </>
      );
    }
    case 'todo': {
      const t = data.todo;
      const section = sectionOf(t, todos);
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
              {data.sessions.slice(0, 5).map(({ session }) => (
                <SessionLine key={session.key} s={session} now={now} />
              ))}
            </div>
          )}
          <div className="ca-cvp-hint">{data.sessions.length > 0 ? 'Double-click to open its session' : 'Double-click to start a session for it'}</div>
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
              <PrChip pr={{ number: p.number, title: p.title, url: p.url, state: p.state, checks: p.checks, merge: p.merge, updatedAt: p.updatedAt, mergedAt: p.mergedAt, closedAt: p.closedAt }} />
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
