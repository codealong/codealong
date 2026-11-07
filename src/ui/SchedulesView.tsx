// The Schedules view's list (App.tsx): a row for each schedule, laid out as todo and PR rows are
// (rows.css .is-item): what watches for its time, its name and when it runs, what it does, when it
// runs next, and how long ago it last ran. Under it, set in as a session's helpers are in
// Sessions, the sessions its runs started or sent to (boardLists.ts `scheduleSessions`): the
// latest run's and the ones at work, the rest a click away. The box at the top makes one
// (ScheduleDialog.tsx); a row's menu is menus.ts `scheduleMenu`, and its panel (SchedulePanel.tsx)
// has the rest; a session's is its own. After them, the schedules the harnesses keep themselves (a
// Claude Code session's loop, a Codex app automation), read only, each with the session it wakes;
// and last, folded, the finished ones (`scheduleSections`).

import { Fragment, useRef, type KeyboardEvent } from 'react';
import { ago, absolute } from '../format';
import { HARNESS_WORD } from '../harness';
import type { Placed } from '../lanes';
import { doesWords, nextWords, OUTCOME_WORD, scheduleSections, whenIcon } from '../schedules';
import type { HarnessSchedule, ScheduleItem } from '../types';
import { AgentRow } from './LaneRow';
import { Chip, cx, Icon, Row, SectionHeader } from './primitives';

export interface SchedulesViewProps {
  items: ScheduleItem[];
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
  const { items, harnessItems, problem, now, selectedId, selectedSession, sessionTitle, sessionsOf, placed, allShown, showFinished, onToggleFinished, onToggleAll, onSelect, onEdit, onMenu, onSelectSession, onOpenSession, onSessionMenu, onOpenHarness, onNew } = props;
  const list = useRef<HTMLDivElement>(null);
  const { active, finished } = scheduleSections(items);
  const listed = (xs: ScheduleItem[]) => xs.map((item) => ({ item, ...sessionsOf(item) }));
  const activeRows = listed(active);
  const finishedRows = showFinished ? listed(finished) : [];
  const harnessRows = harnessItems.map((h) => ({ h, p: h.session ? placed(h.session) : null }));
  const scheduleEntries = (xs: typeof activeRows): Entry[] => xs.flatMap(({ item, rows }) => [{ id: item.id, item, p: null }, ...rows.map((p) => ({ id: item.id, item, p }))]);
  const entries: Entry[] = [
    ...scheduleEntries(activeRows),
    ...harnessRows.flatMap(({ h, p }) => [{ id: h.key, harness: h, p: null }, ...(p ? [{ id: h.key, harness: h, p }] : [])]),
    ...scheduleEntries(finishedRows),
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
  const scheduleRow = ({ item: s, rows, more }: (typeof activeRows)[number]) => {
    const last = s.runs[0];
    const next = nextWords(s, now);
    const all = allShown.has(s.id);
    const done = s.finished != null;
    return (
      <Fragment key={s.id}>
        <Row
          id={scheduleRowId(s.id)}
          label={s.name}
          selected={s.id === selectedId && !selectedSession}
          className={cx('ca-lane', 'is-item', 'ca-schedrow', (s.paused || done) && 'is-paused', !done && !!s.problem && 'is-problem')}
          onSelect={() => onSelect(s.id)}
          onOpen={() => onEdit(s)}
          onMenu={() => onMenu(s)}
        >
          <div className="ca-seg ca-seg-glyph">
            <Icon name={done ? 'check' : s.problem ? 'alert' : s.paused ? 'player-pause' : whenIcon(s.when)} size={14} className="ca-schedrow-glyph" />
          </div>
          <div className="ca-seg ca-seg-main">
            <div className="ca-lane-line1">
              <span className="ca-lane-headline ca-schedrow-name">{s.name}</span>
              <span className="ca-lane-detail">{s.whenWords}</span>
            </div>
            <div className="ca-lane-line2">
              <span className="ca-chips ca-lane-lead">
                <Chip className="ca-schedrow-does" title={s.run.prompt}>
                  {doesWords(s, sessionTitle)}
                </Chip>
                {s.once && !done && (
                  <Chip className="ca-schedrow-once" title="It stops after its first run. Run now doesn’t count.">
                    Once
                  </Chip>
                )}
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
              </span>
              {next && <span className="ca-lane-right ca-schedrow-next">{next}</span>}
            </div>
          </div>
          <div className="ca-seg ca-seg-age" title={last ? `${OUTCOME_WORD[last.outcome]} ${absolute(last.at)}: ${last.why}` : 'Hasn’t run yet'}>
            {last ? ago(last.at, now) : ''}
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
    return (
      <Fragment key={h.key}>
        <Row
          id={scheduleRowId(h.key)}
          label={h.name}
          selected={h.key === selectedId && !selectedSession}
          className={cx('ca-lane', 'is-item', 'ca-schedrow', 'is-harness', h.paused && 'is-paused', !!h.problem && 'is-problem')}
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
            </div>
            <div className="ca-lane-line2">
              <span className="ca-chips ca-lane-lead">
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
              </span>
              {next && <span className="ca-lane-right ca-schedrow-next">{next}</span>}
            </div>
          </div>
          <div className="ca-seg ca-seg-age" />
        </Row>
        {p && sessionRow(h.key, p)}
      </Fragment>
    );
  };
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
      {items.length === 0 && harnessItems.length === 0 ? (
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
          {activeRows.map(scheduleRow)}
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
              {showFinished && <div className="ca-bay-lanes">{finishedRows.map(scheduleRow)}</div>}
            </section>
          )}
        </div>
      )}
    </>
  );
}
