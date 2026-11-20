// The Schedules view's list (App.tsx): a row for each schedule, laid out as todo and PR rows are
// (rows.css .is-item): what watches for its time, the emblem its sessions show, its name and when it
// runs, what it does, when it runs next, and how long ago it last ran. Under it, set in as a session's helpers are in
// Sessions, the sessions its runs started or sent to (boardLists.ts `scheduleSessions`): the
// latest run's and the ones at work, the rest a click away. The box at the top makes one
// (ScheduleDialog.tsx); a row's menu is menus.ts `scheduleMenu`, and its panel (SchedulePanel.tsx)
// has the rest; a session's is its own. The ones that run once and haven't yet are listed apart,
// under One-off, after the ones that go on running. After them, the schedules the harnesses keep
// themselves (a Claude Code session's loop, a Codex app automation), read only, each with the
// session it wakes; and last, folded, the finished ones (`scheduleSections`). The View menu over the
// list sorts the ones Codealong keeps (`SCHEDULE_SORTS`), puts each on two lines or one
// (`SCHEDULE_ROWS`), and groups them under the worktrees or repos they run in (groups.ts,
// Groups.tsx; schedules.ts `scheduleListing`), the one-offs and the finished among themselves.

import { Fragment, useRef, type KeyboardEvent, type ReactNode } from 'react';
import { ago, absolute } from '../format';
import { groupOptions, type ListGroup } from '../groups';
import { HARNESS_WORD } from '../harness';
import type { Placed } from '../lanes';
import { doesWords, nextWords, OUTCOME_WORD, scheduleName, whenIcon, type ScheduleListing, type ScheduleSection, type ScheduleSort } from '../schedules';
import type { HarnessSchedule, ScheduleItem } from '../types';
import { GroupBlock, PinnedGroups, type GroupActs } from './Groups';
import { AgentRow } from './LaneRow';
import { rowsOptions, type ListOption } from './ListBar';
import { Chip, cx, Icon, Row, SectionHeader, SessionEmblem } from './primitives';

/** Sort's choices over Schedules (schedules.ts `ScheduleSort`): the finished keep the latest to
 *  finish first, and the harnesses' their own order. */
export const SCHEDULE_SORTS: ListOption<ScheduleSort>[] = [
  { id: 'next', label: 'Next run', icon: 'clock', title: 'Sort by next run: the soonest first, then the ones that watch, wait for an event or are paused' },
  { id: 'last', label: 'Last run', icon: 'history', title: 'Sort by last run: the one that ran latest first, then the ones that haven’t run' },
  { id: 'added', label: 'Added', icon: 'sort-descending-numbers', title: 'Sort by when added: the newest schedule first' },
  { id: 'name', label: 'Name', icon: 'sort-ascending-letters', title: 'Sort by name, from A to Z' },
];

/** Group by's choices over Schedules (groups.ts). */
export const SCHEDULE_GROUPS = groupOptions('schedules');

/** What the heading over schedules in no worktree says: the ones that start a chat of their own,
 *  or whose worktree is gone. */
export const SCHEDULE_NONE = 'No worktree';

export const SCHEDULE_ROWS = rowsOptions('under each schedule, what it does and when it runs next', 'each schedule with what it does and when it runs next at the end');

export interface SchedulesViewProps {
  /** The schedules Codealong keeps, as the view lists them: sorted, and grouped when it's grouped. */
  listing: ScheduleListing;
  /** What a group's heading does, when the list is grouped. */
  groupActs: GroupActs;
  /** Sorted by when added, a row's age says when it was added rather than when it last ran. */
  sort: ScheduleSort;
  /** Each row on one line, what it does and when next at its end. */
  slim: boolean;
  /** The schedules the harnesses keep themselves, as far as the project shows them. */
  harnessItems: HarnessSchedule[];
  /** Why the schedules' files don't read, while they don't. */
  problem?: string;
  now: number;
  /** The schedule selected, by its id, or a harness's, by its key. */
  selectedId: string | null;
  /** The session selected under its schedule, by key; the schedule's row isn't, meanwhile. */
  selectedSession: string | null;
  /** A session's title by its key, for what a schedule sends to. */
  sessionTitle: (key: string) => string | null;
  /** The sessions listed under a schedule (boardLists.ts `scheduleSessions`), and how many more
   *  its fold row lists, or hides while they're listed (`allShown`). */
  sessionsOf: (item: ScheduleItem) => { rows: Placed[]; more: number };
  /** A session on the board by key, for the one a harness's schedule wakes. */
  placed: (key: string) => Placed | null;
  allShown: ReadonlySet<string>;
  /** The finished schedules are listed, not folded away. */
  showFinished: boolean;
  onToggleFinished: () => void;
  onToggleAll: (id: string) => void;
  onSelect: (id: string) => void;
  onEdit: (item: ScheduleItem) => void;
  onMenu: (item: ScheduleItem) => void;
  /** A session under a schedule, or under a harness's (by its key), picked. */
  onSelectSession: (id: string, p: Placed) => void;
  onOpenSession: (p: Placed) => void;
  onSessionMenu: (p: Placed) => void;
  /** Opens a harness's schedule where it lives: its link, else its session. */
  onOpenHarness: (h: HarnessSchedule) => void;
  onNew: () => void;
}

/** A row's element id, from its schedule's. */
export const scheduleRowId = (id: string) => `schedule-${id}`;

/** A session's row's element id, under schedule `id`. */
const sessionRowId = (id: string, key: string) => `schedule-${id}-${key}`;

/** One of the list's rows, in order: a schedule, or a session under it; a harness's schedule, or
 *  its session. */
type Entry = { id: string; item?: ScheduleItem; harness?: HarnessSchedule; p: Placed | null };

export function SchedulesView(props: SchedulesViewProps) {
  const { listing, groupActs, sort, slim, harnessItems, problem, now, selectedId, selectedSession, sessionTitle, sessionsOf, placed, allShown, showFinished, onToggleFinished, onToggleAll, onSelect, onEdit, onMenu, onSelectSession, onOpenSession, onSessionMenu, onOpenHarness, onNew } = props;
  const list = useRef<HTMLDivElement>(null);
  // Each schedule's sessions, worked out once a render for both its rows and the arrows.
  const sessionsKept = new Map<string, ReturnType<typeof sessionsOf>>();
  const sessionsFor = (item: ScheduleItem) => {
    const had = sessionsKept.get(item.id);
    if (had) return had;
    const got = sessionsOf(item);
    sessionsKept.set(item.id, got);
    return got;
  };
  const { grouped } = listing;
  const section = (id: ScheduleSection['id']) => listing.sections.find((x) => x.id === id)?.rows ?? [];
  // A section's groups, grouped; none for one with no row.
  const groupsIn = (id: ScheduleSection['id']) => grouped?.sections.find((x) => x.id === id)?.groups ?? [];
  // A section's rows as it draws them: in its groups' order, the ones folded away left out.
  const drawn = (id: ScheduleSection['id']) => (grouped ? groupsIn(id).flatMap((g) => g.rows) : section(id));
  const finished = section('finished');
  const harnessRows = harnessItems.map((h) => ({ h, p: h.session ? placed(h.session) : null }));
  const scheduleEntries = (xs: ScheduleItem[]): Entry[] => xs.flatMap((item) => [{ id: item.id, item, p: null }, ...sessionsFor(item).rows.map((p) => ({ id: item.id, item, p }))]);
  const entries: Entry[] = [
    ...scheduleEntries(grouped ? grouped.pinned.flatMap((g) => g.rows) : []),
    ...scheduleEntries(drawn('active')),
    ...scheduleEntries(drawn('oneOff')),
    ...harnessRows.flatMap(({ h, p }) => [{ id: h.key, harness: h, p: null }, ...(p ? [{ id: h.key, harness: h, p }] : [])]),
    ...scheduleEntries(showFinished ? drawn('finished') : []),
  ];
  const isSelected = (x: Entry) => x.id === selectedId && (x.p ? x.p.session.key === selectedSession : !selectedSession);
  const select = (x: Entry) => (x.p ? onSelectSession(x.id, x.p) : onSelect(x.id));
  const selectedEntry = entries.find(isSelected);
  // ↑ ↓ through the schedules and the sessions under them; Return changes a schedule, or opens a
  // harness's where it lives. A session's Return, and its menu's keys, are the window's
  // (mainKeys.ts), as in any list, and a button (List N more) takes its own.
  const keys = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.target instanceof HTMLButtonElement) return;
    const i = entries.findIndex(isSelected);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const next = entries[Math.min(entries.length - 1, Math.max(0, i + (e.key === 'ArrowDown' ? 1 : -1)))];
      if (next) select(next);
    } else if (e.key === 'Enter' && i >= 0 && !entries[i].p) {
      e.preventDefault();
      const x = entries[i];
      if (x.item) onEdit(x.item);
      else if (x.harness) onOpenHarness(x.harness);
    }
  };
  const sessionRow = (id: string, p: Placed) => (
    <AgentRow
      key={p.session.key}
      lane={p.lane}
      session={p.session}
      domId={sessionRowId(id, p.session.key)}
      now={now}
      selected={id === selectedId && p.session.key === selectedSession}
      onSelect={() => onSelectSession(id, p)}
      onOpen={() => onOpenSession(p)}
      onContextMenu={() => onSessionMenu(p)}
      nested
      // Its schedule's sessions can be in any worktree: each row says its own.
      elsewhere
    />
  );
  const scheduleRow = (s: ScheduleItem) => {
    const { rows, more } = sessionsFor(s);
    const last = s.runs[0];
    const next = nextWords(s, now);
    const all = allShown.has(s.id);
    const done = s.finished != null;
    const chips = (
      <>
        <Chip className="ca-schedrow-does" title={s.run.prompt}>
          {doesWords(s, sessionTitle)}
        </Chip>
        {!done && s.problem && (
          <Chip className="ca-schedrow-problem" title={s.problem}>
            Can’t run
          </Chip>
        )}
        {last?.outcome === 'failed' && (
          <Chip className="ca-schedrow-problem" title={last.said}>
            Last run failed
          </Chip>
        )}
        {s.queued && (
          <Chip className="ca-schedrow-queued" title={`Waiting for its session’s turn to end: ${s.queued.why.join('; ')}`}>
            <Icon name="hourglass" size={11} />
            Waiting
          </Chip>
        )}
      </>
    );
    const added = sort === 'added';
    return (
      <Fragment key={s.id}>
        <Row
          id={scheduleRowId(s.id)}
          label={s.orchestrates ? `${scheduleName(s)}, for ${s.orchestrates.project ?? 'All projects'}` : s.name}
          selected={s.id === selectedId && !selectedSession}
          className={cx('ca-lane', 'is-item', 'ca-schedrow', slim && 'is-slim', (s.paused || done) && 'is-paused', !done && !!s.problem && 'is-problem')}
          onSelect={() => onSelect(s.id)}
          onOpen={() => onEdit(s)}
          onMenu={() => onMenu(s)}
        >
          <div className="ca-seg ca-seg-glyph">
            <Icon name={done ? 'check' : s.problem ? 'alert' : s.paused ? 'player-pause' : s.orchestrates ? 'compass' : whenIcon(s.when)} size={14} className="ca-schedrow-glyph" />
          </div>
          <div className="ca-seg ca-seg-main">
            <div className="ca-lane-line1">
              <SessionEmblem emblem={s.emblem} of={s.id} />
              <span className="ca-lane-headline ca-schedrow-name">{scheduleName(s)}</span>
              <span className="ca-lane-detail">{s.whenWords}</span>
              {slim && (
                <>
                  <span className="ca-chips ca-lane-tags">{chips}</span>
                  {next && <span className="ca-schedrow-next">{next}</span>}
                </>
              )}
            </div>
            {!slim && (
              <div className="ca-lane-line2">
                <span className="ca-chips ca-lane-lead">{chips}</span>
                {next && <span className="ca-lane-right ca-schedrow-next">{next}</span>}
              </div>
            )}
          </div>
          <div className="ca-seg ca-seg-age" title={added ? `Added ${absolute(s.createdAt)}${last ? `; ${OUTCOME_WORD[last.outcome].toLowerCase()} ${absolute(last.at)}` : ''}` : last ? `${OUTCOME_WORD[last.outcome]} ${absolute(last.at)}: ${last.why}` : 'Hasn’t run yet'}>
            {added ? ago(s.createdAt, now) : last ? ago(last.at, now) : ''}
          </div>
        </Row>
        {rows.map((p) => sessionRow(s.id, p))}
        {more > 0 && (
          <div className="ca-lane is-item is-agent is-slim is-nested is-fold" role="presentation">
            <div className="ca-seg ca-seg-glyph" aria-hidden="true">
              <Icon name={all ? 'chevron-up' : 'chevron-down'} size={12} />
            </div>
            <div className="ca-seg ca-seg-main">
              <button type="button" className="ca-linkbtn ca-foldrow-btn" onClick={() => onToggleAll(s.id)}>
                {all ? 'List only the latest run’s and those at work' : `List ${more} more ${more === 1 ? 'session' : 'sessions'}`}
              </button>
            </div>
          </div>
        )}
      </Fragment>
    );
  };
  const harnessRow = ({ h, p }: (typeof harnessRows)[number]) => {
    const next = h.paused ? 'Paused' : nextWords({ paused: false, nextAt: h.nextAt, when: { kind: 'time', rule: '' } }, now);
    const chips = (
      <>
        <Chip className="ca-schedrow-does" title={`${h.lasts}\n\n${h.prompt}`}>
          {HARNESS_WORD[h.harness]} {h.kind.toLowerCase()}
        </Chip>
        {h.problem && (
          <Chip className="ca-schedrow-problem" title={h.problem}>
            Can’t run
          </Chip>
        )}
        {/* Its account last, as a session's is. */}
        <Chip title={`On ${h.account}`}>{h.account}</Chip>
      </>
    );
    return (
      <Fragment key={h.key}>
        <Row
          id={scheduleRowId(h.key)}
          label={h.name}
          selected={h.key === selectedId && !selectedSession}
          className={cx('ca-lane', 'is-item', 'ca-schedrow', 'is-harness', slim && 'is-slim', h.paused && 'is-paused', !!h.problem && 'is-problem')}
          onSelect={() => onSelect(h.key)}
          onOpen={() => onOpenHarness(h)}
        >
          <div className="ca-seg ca-seg-glyph">
            <Icon name={h.problem ? 'alert' : h.paused ? 'player-pause' : 'clock'} size={14} className="ca-schedrow-glyph" />
          </div>
          <div className="ca-seg ca-seg-main">
            <div className="ca-lane-line1">
              <span className="ca-lane-headline ca-schedrow-name">{h.name}</span>
              <span className="ca-lane-detail">{h.whenWords}</span>
              {slim && (
                <>
                  <span className="ca-chips ca-lane-tags">{chips}</span>
                  {next && <span className="ca-schedrow-next">{next}</span>}
                </>
              )}
            </div>
            {!slim && (
              <div className="ca-lane-line2">
                <span className="ca-chips ca-lane-lead">{chips}</span>
                {next && <span className="ca-lane-right ca-schedrow-next">{next}</span>}
              </div>
            )}
          </div>
          <div className="ca-seg ca-seg-age" />
        </Row>
        {p && sessionRow(h.key, p)}
      </Fragment>
    );
  };
  // A group of schedules under its worktree's or repo's heading, which shows the worktree in place
  // of the schedule's panel (mainGroups.ts `groupActsOf`).
  const groupBlock = (g: ListGroup<ScheduleItem>, pinned: boolean) => (
    <GroupBlock key={`group:${g.key}`} g={g} acts={groupActs} now={now} rows={false} pinned={pinned} none={SCHEDULE_NONE}>
      {g.rows.map(scheduleRow)}
    </GroupBlock>
  );
  const rowsOf = (id: ScheduleSection['id']): ReactNode => (grouped ? groupsIn(id).map((g) => groupBlock(g, false)) : section(id).map(scheduleRow));
  return (
    <>
      <button type="button" className="ca-schednew" onClick={onNew}>
        <Icon name="plus" size={14} />
        <span>New schedule…</span>
      </button>
      {problem && (
        <div className="ca-schedproblem" role="alert">
          <Icon name="alert" size={14} />
          <span>{problem}</span>
        </div>
      )}
      {listing.sections.every((x) => x.rows.length === 0) && harnessItems.length === 0 ? (
        <div className="ca-board-empty">
          No schedules yet. A schedule starts a session, or sends one a message, at the times you give it, when a command it runs prints something new, when a PR merges or a todo is ready, or when a script fires it.
        </div>
      ) : (
        <div
          role="listbox"
          aria-label="Schedules"
          tabIndex={0}
          ref={list}
          className="ca-board-list ca-schedlist"
          aria-activedescendant={selectedEntry ? (selectedEntry.p ? sessionRowId(selectedEntry.id, selectedEntry.p.session.key) : scheduleRowId(selectedEntry.id)) : undefined}
          onKeyDown={keys}
        >
          {grouped && grouped.pinned.length > 0 && (
            <PinnedGroups by={grouped.by} count={grouped.pinned.reduce((n, g) => n + g.all.length, 0)} reorder={false}>
              {grouped.pinned.map((g) => groupBlock(g, true))}
            </PinnedGroups>
          )}
          {rowsOf('active')}
          {section('oneOff').length > 0 && (
            <section className="ca-bayblock">
              <SectionHeader icon="repeat-once" label="One-off" count={section('oneOff').length} hint="Each stops after its first run" />
              <div className="ca-bay-lanes">{rowsOf('oneOff')}</div>
            </section>
          )}
          {harnessRows.length > 0 && (
            <section className="ca-bayblock">
              <SectionHeader icon="clock" label="Kept by their harnesses" count={harnessRows.length} hint="Read only: change them where they live" />
              <div className="ca-bay-lanes">{harnessRows.map(harnessRow)}</div>
            </section>
          )}
          {finished.length > 0 && (
            <section className="ca-bayblock">
              <SectionHeader
                icon="check"
                label="Finished"
                count={finished.length}
                hint={
                  <button type="button" className="ca-linkbtn" aria-expanded={showFinished} onClick={onToggleFinished}>
                    {showFinished ? 'Hide' : 'Show'}
                  </button>
                }
              />
              {showFinished && <div className="ca-bay-lanes">{rowsOf('finished')}</div>}
            </section>
          )}
        </div>
      )}
    </>
  );
}
