// The quick panel's list as it's drawn: a row of each kind, the heading each group of rows is
// under, and the notes under a group. Quick.tsx has the panel, what it lists and what a row does;
// quickRows.ts the rows that are the panel's own.

import { ago } from '../format';
import { dueWords } from '../due';
import { sessionWhy } from '../lanes';
import { scopeWords } from '../orchestrators';
import { keysIn } from '../platform';
import { prPills } from '../prStatus';
import { GROUP_WORD, type TodoRow } from '../quick';
import type { Row, RowSection, SuggestRow } from '../quickRows';
import { sureness, toSessionSuggestion, type Suggestion, type suggestionWords } from '../route';
import { skillWhere } from '../skills';
import { snippet, todoAgent } from '../todos';
import type { Lane, PrItem, Route } from '../types';
import { DeviceBadge } from './DeviceBadge';
import { SnoozeChip } from './LaneRow';
import { CheckBox, Chip, cx, Icon, Kbd, Marked, SaidText, STATUS_WORD, SessionEmblem, SessionGlyph } from './primitives';
import { ProjectPrefix } from './ProjectSaid';
import { PrGlyph } from './PrsView';
import { TodoGlyph } from './TodoBits';
import type { DragProps } from './useRowDrag';

/** A suggestion's row in words: what ⏎ does, why, and where (route.ts `suggestionWords`, as the
 *  panel's choice of agent and place has it). */
type Describe = (sg: Suggestion) => ReturnType<typeof suggestionWords>;

/** What every row of the list is drawn from and does, the same for all of them. */
export interface RowsShown {
  /** The row selected, by its key. */
  active: string;
  /** The rows ⌘1 to ⌘9 reach, in order. */
  numbered: Row[];
  now: number;
  /** Jev's answer for what's typed, and the one before it, which stands while the next is worked out. */
  route: Route | null;
  lastRoute: Route | null;
  /** Jev's suggestions as listed: with one that names a session, Send to's row says another. */
  suggested: SuggestRow[];
  /** Send to's rows keep a column for how likely Jev thinks each is. */
  sendLikely: boolean;
  describe: Describe;
  /** Whether the waiting window lists a worktree's sessions, for ⇥ to answer one there. */
  inWaiting: (lane: Lane) => boolean;
  /** A row's drag out of the panel (useRowDrag.tsx `rowProps`). */
  held: (key: string) => DragProps;
  /** A click on a row, and a right-click. */
  click: (key: string, e: React.MouseEvent) => void;
  menu: (r: Row) => void;
}

/** What a found todo's row says under its title: when it's due, then what it waits for, else the
 *  start of its description, else how many sessions it has. */
function todoWhy(r: TodoRow, now: number): string {
  const t = r.item.todo;
  const due = t.due && t.status !== 'done' ? dueWords(t.due, now)?.text : undefined;
  return [due, todoState(r)].filter(Boolean).join(' · ');
}

function todoState(r: TodoRow): string {
  const t = r.item.todo;
  if (t.waiting) return t.waiting.note ? `Waiting for ${t.waiting.note}` : 'Waiting';
  const live = r.item.sessions.find((x) => x.session.live);
  if (live) return `${live.session.account} · ${STATUS_WORD[live.session.status]}`;
  const n = r.item.sessions.length;
  return snippet(t.description, 120) || (n > 0 ? `${n} session${n === 1 ? '' : 's'}` : 'No session yet');
}

/** What a found PR's row says under its title: its number, who opened it, and where it stands.
 *  Checks read as words ("10 checks passed"): the row has no icons to carry them. */
function prWhy(pr: PrItem): string {
  return [`#${pr.number}`, pr.mine ? 'yours' : pr.author, ...prPills(pr).map((p) => (p.key === 'checks' ? p.title : p.text))].filter(Boolean).join(' · ');
}

/** Where a row's session, todo or PR is: its worktree or repo, after its project where the panel
 *  lists every project, the project shortening first; `lead` before them (a session's account). */
function RowPlace({ lead, project, place }: { lead?: string; project: string; place: string }) {
  return (
    <span className="ca-quick-row-where is-place">
      {lead && <span>{lead} ·</span>}
      <ProjectPrefix project={project} beside={[place]} />
      <span className="ca-quick-row-place">{place}</span>
    </span>
  );
}

/** What a suggestion's row shows, but its keys. */
export function SuggestionFace({ sg, describe }: { sg: Suggestion; describe: Describe }) {
  const said = describe(sg);
  return (
    <>
      <span className="ca-quick-plus is-suggest">
        <Icon name={said.icon} size={13} stroke={2} />
      </span>
      <span className="ca-quick-row-main">
        <span className="ca-quick-row-title">
          {/* A suggestion to send to a session or open one names it, so it shows its emblem too. */}
          {(sg.kind === 'send' || sg.kind === 'open') && <SessionEmblem emblem={sg.session.emblem} of={sg.session.key} />}
          {said.title}
        </span>
        <span className="ca-quick-row-why">{said.why}</span>
      </span>
      <span className="ca-quick-row-where">{said.where}</span>
      <span className={`ca-quick-sure is-${sureness(sg.p)}`}>{Math.round(sg.p * 100)}%</span>
    </>
  );
}

/** What stands in for Jev's suggestions, and how the list stands while it does. */
interface StandIn {
  /** The last answer's suggestions, while the next is worked out. */
  last: SuggestRow[];
  describe: Describe;
  routing: boolean;
  routeProblem: string | null;
  /** Jev suggests just what the new session's row starts. */
  startRowSuggested: boolean;
  /** Jev takes what's typed for a search. */
  asSearch: boolean;
  /** ⏎ goes to the new session's row. */
  startLeads: boolean;
}

/** What Suggested shows while it has no suggestion for what's typed: the last answer's, dimmed
 *  and doing nothing, while Jev works out the next; else a line in a suggestion's shape, so the
 *  rows under it don't move when suggestions come. */
export function SuggestedStandIn({ last: lastSuggested, describe, routing, routeProblem, startRowSuggested, asSearch, startLeads }: StandIn) {
  return lastSuggested.length > 0 ? (
    lastSuggested.map((r) => (
      <div key={r.key} role="option" aria-selected={false} aria-disabled="true" data-row={r.key} className="ca-quick-row is-suggest is-stale" onMouseDown={(e) => e.preventDefault()}>
        <SuggestionFace sg={r.s} describe={describe} />
        {/* No key while it does nothing; the column stays, so nothing beside it moves. */}
        <span className="ca-kbds" aria-hidden="true" />
      </div>
    ))
  ) : (
    <div className={cx('ca-quick-row', 'is-suggest', 'is-note', routing && 'is-routing')}>
      <span className="ca-quick-plus is-suggest">
        <Icon name={routeProblem ? 'alert' : 'sparkles'} size={13} stroke={2} />
      </span>
      <span className="ca-quick-row-main">
        <span className="ca-quick-row-title">{routing ? 'Working out where this goes…' : routeProblem ? 'Couldn’t suggest where this goes' : startRowSuggested ? 'Jev suggests the start below' : asSearch ? 'This looks like a search' : 'No suggestion for this'}</span>
        <span className="ca-quick-row-why">{routing ? 'Jev is reading what you’ve typed.' : (routeProblem ?? (startRowSuggested ? keysIn(`It’s just what the new session’s row starts.${startLeads ? ' ⏎ starts it.' : ''}`) : asSearch ? 'None of the sessions Jev read fits it. What the search finds is below.' : 'Start it below, or keep typing.'))}</span>
      </span>
    </div>
  );
}

/** A group's heading: its name, how many there are of it, and for sessions the box that lists
 *  archived ones too. `working`: Jev is working out the next answer while the last one's
 *  suggestions stand in. */
export function GroupLabel({ s, count, working, withArchived, archivedFound, onArchived }: { s: RowSection; count: number; working: boolean; withArchived: boolean; archivedFound: number; onArchived: () => void }) {
  return (
    <div className={`ca-quick-glabel is-${s.group}`}>
      {GROUP_WORD[s.group]}
      {s.group !== 'suggested' ? (
        <span>{count}</span>
      ) : s.rows.some((r) => r.kind === 'suggest') ? (
        <span>{s.rows.filter((r) => r.kind === 'suggest').length}</span>
      ) : (
        working && (
          <span className="ca-quick-glabel-routing" title="Working out where this goes…">
            <Icon name="sparkles" size={11} />
          </span>
        )
      )}
      {(s.group === 'found' || s.group === 'sendto') && (
        <label
          className="ca-check ca-quick-archived"
          title={keysIn(withArchived ? 'Searches list archived sessions too, after the others that match as closely. ⇧⌘A leaves them out' : `Searches leave archived sessions out${archivedFound > 0 ? `; ${archivedFound} ${archivedFound === 1 ? 'matches' : 'match'} this one` : ''}. ⇧⌘A lists them`)}
          // The keyboard stays in the box.
          onMouseDown={(e) => e.preventDefault()}
        >
          <CheckBox checked={withArchived} onChange={onArchived} />
          Archived
          {archivedFound > 0 && <span className="ca-quick-archived-count">{archivedFound}</span>}
        </label>
      )}
    </div>
  );
}

/** One row of the list, as its kind is drawn. */
export function QuickRow({ r, list }: { r: Row; list: RowsShown }) {
  const { active, numbered, now, route, lastRoute, suggested, sendLikely, describe, inWaiting } = list;
  const n = numbered.indexOf(r) + 1;
  // What every row does with the pointer: hovering selects (the list's pointerAt, by the row's
  // data-row), a click does it, a right-click has the rest, and dragging it out of the panel pins
  // it to the desktop.
  const held = list.held(r.key);
  const handlers = {
    'data-row': r.key,
    onPointerDown: held.onPointerDown,
    onMouseDown: (e: React.MouseEvent) => e.preventDefault(),
    onClick: (e: React.MouseEvent) => list.click(r.key, e),
    onContextMenu: (e: React.MouseEvent) => {
      e.preventDefault();
      list.menu(r);
    },
  };
  if (r.kind === 'suggest') {
    const said = describe(r.s);
    return (
      <div
        role="option"
        aria-selected={active === r.key}
        className={cx('ca-quick-row', 'is-suggest', active === r.key && 'is-active', held.extraClass)}
        title={keysIn(`${said.title}\n${said.why}\n${Math.round(r.s.p * 100)}% likely, from ${route?.model ?? 'Jev'} in ${route?.ms ?? 0} ms · ⏎ does it${r.s.kind === 'send' ? ' · ⌥⏎ shows the session in Codealong' : r.s.kind === 'open' ? ' · ⌥⏎ opens the session where it lives' : ''}`)}
        {...handlers}
      >
        <SuggestionFace sg={r.s} describe={describe} />
        {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
      </div>
    );
  }
  if (r.kind === 'skill') {
    return (
      <div
        role="option"
        aria-selected={active === r.key}
        className={cx('ca-quick-row', 'is-skill', active === r.key && 'is-active')}
        title={keysIn(`/${r.skill.name}${r.skill.hint ? ` ${r.skill.hint}` : ''}\n${r.skill.description}\n${r.lane ? `In ${r.lane.repo}` : `${r.accounts.join(', ')}’s own`}${r.skill.uses > 0 ? ` · run ${r.skill.uses} ${r.skill.uses === 1 ? 'time' : 'times'}` : ''}\n⏎ or ⇥ finishes the name; ${r.lane ? `a session on it starts in ${r.lane.repo}` : 'a session on it starts where it’s set to below'}`)}
        data-row={r.key}
        onMouseDown={handlers.onMouseDown}
        onClick={handlers.onClick}
      >
        <span className="ca-quick-plus is-suggest">
          <Icon name="slash" size={13} stroke={2} />
        </span>
        <span className="ca-quick-row-main">
          <span className="ca-quick-row-title">
            <Marked text={`/${r.skill.name}`} marks={r.marks} />
            {r.skill.hint && <span className="ca-quick-row-hint"> {r.skill.hint}</span>}
          </span>
          <span className="ca-quick-row-why">{r.skill.description}</span>
        </span>
        <span className="ca-quick-row-where">{skillWhere(r)}</span>
        {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
      </div>
    );
  }
  if (r.kind === 'todo') {
    const t = r.item.todo;
    const opens = todoAgent(r.item) ? 'shows its session in Codealong' : 'gets a session ready to start for it';
    return (
      <div
        role="option"
        aria-selected={active === r.key}
        className={cx('ca-quick-row', 'is-todo', active === r.key && 'is-active', held.extraClass)}
        title={keysIn(`${t.title || 'Untitled'}\n⏎ ${opens} · ⌥⏎ shows the todo in Codealong`)}
        {...handlers}
      >
        <TodoGlyph status={t.status} waiting={r.waiting} />
        <span className="ca-quick-row-main">
          <span className="ca-quick-row-title">
            <Marked text={t.title || 'Untitled'} marks={r.marks} />
          </span>
          <span className="ca-quick-row-why">{todoWhy(r, now)}</span>
        </span>
        {r.item.lane ? <RowPlace project={r.item.lane.project} place={r.item.lane.callsign} /> : <span className="ca-quick-row-where">{t.project || 'No worktree'}</span>}
        <span className="ca-quick-row-age">{ago(r.item.lastActivity, now)}</span>
        {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
      </div>
    );
  }
  if (r.kind === 'more') {
    return (
      <div
        role="option"
        aria-selected={active === r.key}
        className={cx('ca-quick-row', 'is-more', active === r.key && 'is-active', held.extraClass)}
        {...handlers}
      >
        <span className="ca-quick-plus">
          <Icon name="chevron-down" size={13} stroke={2} />
        </span>
        <span className="ca-quick-row-main">
          <span className="ca-quick-row-title">
            {r.count} more {r.count === 1 ? 'session' : 'sessions'}
          </span>
        </span>
        {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
      </div>
    );
  }
  if (r.kind === 'sendto') {
    const another = suggested.some((x) => x.s.kind === 'send' || x.s.kind === 'open');
    return (
      <div
        role="option"
        aria-selected={active === r.key}
        className={cx('ca-quick-row', 'is-more', active === r.key && 'is-active', held.extraClass)}
        title={keysIn('Lists every session this prompt could go to, Jev’s picks first, and finds one by its title, its worktree or what was said in it (⌘F)')}
        {...handlers}
      >
        <span className="ca-quick-plus">
          <Icon name="send" size={13} stroke={2} />
        </span>
        <span className="ca-quick-row-main">
          <span className="ca-quick-row-title">Send to {another ? 'another' : 'a'} session…</span>
          <span className="ca-quick-row-why">Find it by its title, its worktree or what was said in it.</span>
        </span>
        <span className="ca-quick-row-where">
          {r.count} {r.count === 1 ? 'session' : 'sessions'}
        </span>
        <Kbd keys={['⌘', 'F']} />
      </div>
    );
  }
  if (r.kind === 'pr' || r.kind === 'review') {
    const pr = r.pr;
    return (
      <div
        role="option"
        aria-selected={active === r.key}
        className={cx('ca-quick-row', 'is-pr', active === r.key && 'is-active', held.extraClass)}
        title={keysIn(`${pr.title}\n${pr.repoSlug}#${pr.number} · ${pr.head}\n${r.kind === 'review' ? '⏎ reviews it here · ⌥⏎ shows it in Codealong' : '⏎ shows it in Codealong · ⌥⏎ opens it on GitHub'}`)}
        {...handlers}
      >
        <PrGlyph pr={pr} />
        <span className="ca-quick-row-main">
          <span className="ca-quick-row-title">
            <Marked text={pr.title} marks={r.marks} />
          </span>
          <span className="ca-quick-row-why">{prWhy(pr)}</span>
        </span>
        <RowPlace project={pr.project} place={pr.repo} />
        <span className="ca-quick-row-age">{ago(pr.updatedAt, now)}</span>
        {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
      </div>
    );
  }
  const s2 = r.session;
  // In Send to, ⏎ does what a suggestion of it would (route.ts): it sends what's typed, or shows
  // a session that waits on you to answer.
  const sendsTo = r.group === 'sendto' ? toSessionSuggestion(r.lane, s2, r.p ?? 0) : null;
  const sendWords = sendsTo && describe(sendsTo);
  // One on another machine opens there, not from here, and is answered in Waiting on you only
  // while that lists it (Quick.tsx `answerSession`).
  const there = r.lane.device;
  const answers = inWaiting(r.lane);
  const tab = sendsTo ? (sendsTo.kind === 'send' ? '⏎ Send' : '⏎ Answer') : answers ? '⇥ Reply' : null;
  return (
    <div
      role="option"
      aria-selected={active === r.key}
      className={cx('ca-quick-row', active === r.key && 'is-active', held.extraClass, `is-${s2.status}`)}
      title={keysIn(
        sendsTo && sendWords
          ? `${sendWords.title}\n${sendWords.why}\n${sendWords.where}${r.p != null ? ` · ${Math.round(r.p * 100)}% likely, from ${(route ?? lastRoute)?.model ?? 'Jev'}` : ''}\n${sendsTo.kind === 'send' ? '⏎ sends it · ⌥⏎ shows the session in Codealong' : '⏎ shows it in Codealong · ⌥⏎ opens it where it lives'} · ⇥ answers it in Waiting on you`
          : there
            ? `${s2.title}\n${s2.account} · ${r.lane.callsign} · on ${there.name}\n⏎ shows it in Codealong${answers ? ' · ⇥ answers it in Waiting on you' : ''}`
            : `${s2.title}\n${s2.account} · ${r.lane.callsign}\n⏎ shows it in Codealong · ⇥ answers it in Waiting on you · ⌥⏎ opens it where it lives`,
      )}
      {...handlers}
    >
      <SessionGlyph s={s2} />
      <span className="ca-quick-row-main">
        <span className="ca-quick-row-title">
          <SessionEmblem emblem={s2.emblem} of={s2.key} />
          <Marked text={s2.title} marks={r.marks} />
        </span>
        {r.said ? (
          <SaidText said={r.said} className="ca-quick-row-why is-said" />
        ) : (
          <span className="ca-quick-row-why">{sessionWhy(s2.detail, s2.headline, now)}</span>
        )}
      </span>
      {s2.archived && (
        <Chip title="Archived: listed because searches list archived sessions too">
          <Icon name="archive" size={11} />
          Archived
        </Chip>
      )}
      <SnoozeChip until={s2.snoozedUntil} now={now} away="out of what waits on you" />
      {there && <DeviceBadge device={there} now={now} />}
      {/* Every orchestrator is called Orchestrator: what it keeps watch over tells them apart. */}
      {s2.orchestrator ? <RowPlace lead={s2.account} project="" place={scopeWords(s2.orchestrator.project)} /> : <RowPlace lead={s2.account} project={r.lane.project} place={r.lane.callsign} />}
      {sendLikely && sendsTo && <span className={cx('ca-quick-sure', r.p != null && `is-${sureness(r.p)}`)}>{r.p != null ? `${Math.round(r.p * 100)}%` : ''}</span>}
      <span className="ca-quick-row-age">{ago(s2.statusSince ?? s2.lastActivity, now)}</span>
      {active === r.key && tab && <span className="ca-quick-row-tab ca-keyhint">{keysIn(tab)}</span>}
      {n <= 9 && <Kbd keys={['⌘', String(n)]} />}
    </div>
  );
}

/** The notes under a group's rows. `sendList`: Send to's sessions, with how many there are in all. */
export function GroupNotes({ s, foundNothing, sendToIn, archivedFound, withArchived, sendList, building }: { s: RowSection; foundNothing: boolean; sendToIn: 'suggested' | 'found' | null; archivedFound: number; withArchived: boolean; sendList: { rows: unknown[]; count: number } | null; building: boolean }) {
  return (
    <>
      {/* Under Sessions' heading rather than over the list, so they don't move Suggested as they
          come and go. */}
      {s.group === 'found' && foundNothing && (sendToIn !== 'found' || (archivedFound > 0 && !withArchived)) && (
        <p className="ca-quick-note">
          {archivedFound > 0 && !withArchived
            ? keysIn(`No sessions, todos or PRs match, but ${archivedFound} archived ${archivedFound === 1 ? 'session does' : 'sessions do'}: ⇧⌘A lists ${archivedFound === 1 ? 'it' : 'them'}. Or start a session with this prompt below.`)
            : 'No sessions, todos or PRs match. Start a session with this prompt below.'}
        </p>
      )}
      {s.group === 'sendto' && s.rows.length === 0 && (
        <p className="ca-quick-note">
          {archivedFound > 0 && !withArchived
            ? keysIn(`No session matches, but ${archivedFound} archived ${archivedFound === 1 ? 'one does' : 'ones do'}: ⇧⌘A lists ${archivedFound === 1 ? 'it' : 'them'}. Esc goes back to your prompt.`)
            : 'No session matches. Esc goes back to your prompt.'}
        </p>
      )}
      {s.group === 'sendto' && sendList && sendList.count > sendList.rows.length && (
        <p className="ca-quick-note">{sendList.count - sendList.rows.length} more: type more of what you’re looking for to find them.</p>
      )}
      {(s.group === 'found' || s.group === 'sendto') && building && <p className="ca-quick-note">Still reading what recent sessions said, so more may match in a moment.</p>}
    </>
  );
}
