// The Schedules view's list (App.tsx): a row for each schedule, laid out as todo and PR rows are
// (rows.css .is-item): what watches for its time, its name and when it runs, what it does, when it
// runs next, and how long ago it last ran. Under it, set in as a session's helpers are in
// Sessions, the sessions its runs started or sent to (boardLists.ts `scheduleSessions`): the
// latest run's and the ones at work, the rest a click away. The box at the top makes one
// (ScheduleDialog.tsx); a row's menu is menus.ts `scheduleMenu`, and its panel (SchedulePanel.tsx)
// has the rest; a session's is its own.

import { Fragment, useRef, type KeyboardEvent } from 'react';
import { ago, absolute } from '../format';
import type { Placed } from '../lanes';
import { doesWords, nextWords, OUTCOME_WORD, whenIcon } from '../schedules';
import type { ScheduleItem } from '../types';
import { AgentRow } from './LaneRow';
import { Chip, cx, Icon, Row } from './primitives';

export interface SchedulesViewProps {
  items: ScheduleItem[];
  /** Why the schedules' files don't read, while they don't. */
  problem?: string;
  now: number;
  selectedId: string | null;
  /** The session selected under its schedule, by key; the schedule's row isn't, meanwhile. */
  selectedSession: string | null;
  /** A session's title by its key, for what a schedule sends to. */
  sessionTitle: (key: string) => string | null;
  /** The sessions listed under a schedule (boardLists.ts `scheduleSessions`), and how many more
   *  its fold row lists, or hides while they're listed (`allShown`). */
  sessionsOf: (item: ScheduleItem) => { rows: Placed[]; more: number };
  allShown: ReadonlySet<string>;
  onToggleAll: (id: string) => void;
  onSelect: (id: string) => void;
  onEdit: (item: ScheduleItem) => void;
  onMenu: (item: ScheduleItem) => void;
  onSelectSession: (item: ScheduleItem, p: Placed) => void;
  onOpenSession: (p: Placed) => void;
  onSessionMenu: (p: Placed) => void;
  onNew: () => void;
}

/** A row's element id, from its schedule's. */
export const scheduleRowId = (id: string) => `schedule-${id}`;

/** A session's row's element id, under schedule `id`. */
const sessionRowId = (id: string, key: string) => `schedule-${id}-${key}`;

/** One of the list's rows, in order: a schedule, or a session under it. */
type Entry = { item: ScheduleItem; p: Placed | null };

export function SchedulesView({ items, problem, now, selectedId, selectedSession, sessionTitle, sessionsOf, allShown, onToggleAll, onSelect, onEdit, onMenu, onSelectSession, onOpenSession, onSessionMenu, onNew }: SchedulesViewProps) {
  const list = useRef<HTMLDivElement>(null);
  const listed = items.map((item) => ({ item, ...sessionsOf(item) }));
  const entries: Entry[] = listed.flatMap(({ item, rows }) => [{ item, p: null }, ...rows.map((p) => ({ item, p }))]);
  const isSelected = (x: Entry) => (x.p ? x.item.id === selectedId && x.p.session.key === selectedSession : x.item.id === selectedId && !selectedSession);
  const select = (x: Entry) => (x.p ? onSelectSession(x.item, x.p) : onSelect(x.item.id));
  const selectedEntry = entries.find(isSelected);
  // ↑ ↓ through the schedules and the sessions under them; Return changes a schedule. A session's
  // Return, and its menu's keys, are the window's (mainKeys.ts), as in any list, and a button (List
  // N more) takes its own.
  const keys = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.target instanceof HTMLButtonElement) return;
    const i = entries.findIndex(isSelected);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const next = entries[Math.min(entries.length - 1, Math.max(0, i + (e.key === 'ArrowDown' ? 1 : -1)))];
      if (next) select(next);
    } else if (e.key === 'Enter' && i >= 0 && !entries[i].p) {
      e.preventDefault();
      onEdit(entries[i].item);
    }
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
      {items.length === 0 ? (
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
          aria-activedescendant={selectedEntry ? (selectedEntry.p ? sessionRowId(selectedEntry.item.id, selectedEntry.p.session.key) : scheduleRowId(selectedEntry.item.id)) : undefined}
          onKeyDown={keys}
        >
          {listed.map(({ item: s, rows, more }) => {
            const last = s.runs[0];
            const next = nextWords(s, now);
            const all = allShown.has(s.id);
            return (
              <Fragment key={s.id}>
                <Row
                  id={scheduleRowId(s.id)}
                  label={s.name}
                  selected={isSelected({ item: s, p: null })}
                  className={cx('ca-lane', 'is-item', 'ca-schedrow', s.paused && 'is-paused', !!s.problem && 'is-problem')}
                  onSelect={() => onSelect(s.id)}
                  onOpen={() => onEdit(s)}
                  onMenu={() => onMenu(s)}
                >
                  <div className="ca-seg ca-seg-glyph">
                    <Icon name={s.problem ? 'alert' : s.paused ? 'player-pause' : whenIcon(s.when)} size={14} className="ca-schedrow-glyph" />
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
                        {s.problem && (
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
                {rows.map((p) => (
                  <AgentRow
                    key={p.session.key}
                    lane={p.lane}
                    session={p.session}
                    domId={sessionRowId(s.id, p.session.key)}
                    now={now}
                    selected={isSelected({ item: s, p })}
                    onSelect={() => onSelectSession(s, p)}
                    onOpen={() => onOpenSession(p)}
                    onContextMenu={() => onSessionMenu(p)}
                    nested
                    // Its schedule's sessions can be in any worktree: each row says its own.
                    elsewhere
                  />
                ))}
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
          })}
        </div>
      )}
    </>
  );
}
