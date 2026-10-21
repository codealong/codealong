// The Schedules view's list (App.tsx): a row for each schedule, laid out as todo and PR rows are
// (rows.css .is-item): what watches for its time, its name and when it runs, what it does, when it
// runs next, and how long ago it last ran. The box at the top makes one (ScheduleDialog.tsx); a
// row's menu is menus.ts `scheduleMenu`, and its panel (SchedulePanel.tsx) has the rest.

import { useRef, type KeyboardEvent } from 'react';
import { ago, absolute } from '../format';
import { doesWords, nextWords, OUTCOME_WORD, whenIcon } from '../schedules';
import type { ScheduleItem } from '../types';
import { Chip, cx, Icon, Row } from './primitives';

export interface SchedulesViewProps {
  items: ScheduleItem[];
  /** Why the schedules' files don't read, while they don't. */
  problem?: string;
  now: number;
  selectedId: string | null;
  /** A session's title by its key, for what a schedule sends to. */
  sessionTitle: (key: string) => string | null;
  onSelect: (id: string) => void;
  onEdit: (item: ScheduleItem) => void;
  onMenu: (item: ScheduleItem) => void;
  onNew: () => void;
}

/** A row's element id, from its schedule's. */
export const scheduleRowId = (id: string) => `schedule-${id}`;

export function SchedulesView({ items, problem, now, selectedId, sessionTitle, onSelect, onEdit, onMenu, onNew }: SchedulesViewProps) {
  const list = useRef<HTMLDivElement>(null);
  // ↑ ↓ through the schedules, Return to change the one selected.
  const keys = (e: KeyboardEvent<HTMLDivElement>) => {
    const i = items.findIndex((s) => s.id === selectedId);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const next = items[Math.min(items.length - 1, Math.max(0, i + (e.key === 'ArrowDown' ? 1 : -1)))];
      if (next) onSelect(next.id);
    } else if (e.key === 'Enter' && i >= 0) {
      e.preventDefault();
      onEdit(items[i]);
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
        <div role="listbox" aria-label="Schedules" tabIndex={0} ref={list} className="ca-board-list ca-schedlist" aria-activedescendant={selectedId ? scheduleRowId(selectedId) : undefined} onKeyDown={keys}>
          {items.map((s) => {
            const last = s.runs[0];
            const next = nextWords(s, now);
            return (
              <Row
                key={s.id}
                id={scheduleRowId(s.id)}
                label={s.name}
                selected={s.id === selectedId}
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
            );
          })}
        </div>
      )}
    </>
  );
}
