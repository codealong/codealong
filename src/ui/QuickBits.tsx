// Pieces of the quick panel's page around its list (Quick.tsx has the panel, QuickRows.tsx the
// list's rows): what it asks when it's set to listen, the row that starts (Listen now's, or the
// new session's), the todos the session is for, and the notes under them.

import type { ReactNode } from 'react';
import { CODEALONG_PROJECT, type improveWords } from '../improve';
import { deviceLabel } from '../devices';
import { CALMER_CHORD, loadSentence, type Calmer } from '../machineLoad';
import { meetingTime, meetingWith } from '../meetings';
import { chordKeys } from '../platform';
import { COMPOSE } from '../quickRows';
import { sureness } from '../route';
import type { Lane, ListenAsk, Load, MeetingAhead, Session, Todo } from '../types';
import { ListenChoices } from './Listening';
import { Button, CheckBox, cx, Icon, Kbd } from './primitives';
import { TodoGlyph } from './TodoBits';

/** What the panel asks in the list's place, set to listen: whether it's the meeting in your
 *  calendar (`its`, until you say it isn't), and what a session's Listen… asks (Listening.tsx). */
export function ListenAsked({ meeting, its, setNotMeeting, now, ask, set }: { meeting: MeetingAhead | null; its: boolean; setNotMeeting: (id: string | null) => void; now: number; ask: ListenAsk; set: (next: Partial<ListenAsk>) => void }) {
  // A press here leaves the keyboard in the box, as a press on a row does.
  return (
    <div className="ca-quick-listen" onMouseDown={(e) => e.preventDefault()}>
      {meeting && (
        <label className="ca-listendlg-check">
          <CheckBox checked={its} onChange={(on) => setNotMeeting(on ? null : meeting.id)} />
          <span>It’s {meeting.title.trim() ? `“${meeting.title.trim()}”` : 'the meeting in your calendar'}</span>
          <small>{[meetingTime(meeting, now), meetingWith(meeting)].filter(Boolean).join(', ')}. Its notes are kept under its name, with who’s in it.</small>
        </label>
      )}
      <ListenChoices ask={ask} set={set} playing={[]} />
    </div>
  );
}

/** Listen now's row, under the list: what ⏎ starts, or why it can't (`problem`). */
export function ListenRow({ problem, title, why, onClick }: { problem: string | null; title: string; why: string; onClick: () => void }) {
  return (
    <div role="option" aria-selected className={cx('ca-quick-row', 'ca-quick-compose', 'is-active', problem && 'is-disabled')} onMouseDown={(e) => e.preventDefault()} onClick={onClick}>
      <span className="ca-quick-plus">
        <Icon name="microphone" size={14} stroke={2} />
      </span>
      <span className="ca-quick-row-main">
        <span className="ca-quick-row-title">{title}</span>
        <span className="ca-quick-row-why">{why}</span>
      </span>
      <Kbd keys={['⏎']} />
    </div>
  );
}

/** The new session's row, under the list: where it starts (`title`, `where`), what starting does
 *  (its children, on the line under the title), and how likely Jev thinks this start is when it
 *  suggested just this one (`suggested`, by the model `from`). */
export function StartRow({ active, ready, title, where, suggested, from, onClick, children }: { active: boolean; ready: boolean; title: string; where: string | null; suggested: { p: number } | null; from: string; onClick: (e: React.MouseEvent) => void; children: ReactNode }) {
  return (
    <div
      role="option"
      aria-selected={active}
      className={cx('ca-quick-row', 'ca-quick-compose', active && 'is-active', !ready && 'is-disabled')}
      data-row={COMPOSE}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
    >
      <span className="ca-quick-plus">
        <Icon name="plus" size={14} stroke={2} />
      </span>
      <span className="ca-quick-row-main">
        <span className="ca-quick-row-title">{title}</span>
        <span className="ca-quick-row-why">{children}</span>
      </span>
      {where !== null && <span className="ca-quick-row-where">{where}</span>}
      {/* Where a suggestion says how likely it is, so the columns line up with theirs: Jev's,
          when it suggested just this start. */}
      {suggested ? (
        <span className={`ca-quick-sure is-${sureness(suggested.p)}`} title={`Jev suggests this start: ${Math.round(suggested.p * 100)}% likely, from ${from}`}>
          {Math.round(suggested.p * 100)}%
        </span>
      ) : (
        <span className="ca-quick-sure" aria-hidden="true" />
      )}
      <Kbd keys={['⌘', '⏎']} />
    </div>
  );
}

/** The todos a new session is for, under the list: what they wait on (`blocked`, `waitsOn`), the
 *  ones whose primary session is running (`held`), the ones marked as waiting (`marked`), and
 *  the way to start without them. */
export function TodoStrip({ todo, forTodos, blocked, waitsOn, held, marked, showSession, unlink }: { todo: Todo; forTodos: Todo[]; blocked: { t: Todo; on: Todo[] }[]; waitsOn: Todo[]; held: { todo: Todo; at: { lane: Lane; session: Session } }[]; marked: Todo[]; showSession: (lane: Lane, session: Session) => void; unlink: () => void }) {
  return (
    <div className="ca-quick-todo">
      {forTodos.length > 1 ? <Icon name="list-check" size={14} /> : <TodoGlyph status={todo.status} />}
      <span className="ca-quick-todo-title" title={forTodos.length > 1 ? forTodos.map((t, i) => `${i + 1}. ${t.title || 'Untitled'}`).join('\n') : undefined}>
        {forTodos.length > 1 ? `For ${forTodos.length} todos: ${forTodos.map((t) => `“${t.title || 'Untitled'}”`).join(', ')}` : `For “${todo.title || 'Untitled'}”`}
      </span>
      {waitsOn.length > 0 && (
        <span className="ca-quick-todo-wait" title={blocked.flatMap((b) => b.on.map((w) => `${forTodos.length > 1 ? `“${b.t.title || 'Untitled'}” waits` : 'Waits'} on “${w.title || 'Untitled'}”`)).join('\n')}>
          <Icon name="lock" size={12} />
          {forTodos.length > 1 && (blocked.length === 1 ? 'one ' : `${blocked.length} `)}
          {forTodos.length > 1 && blocked.length > 1 ? 'wait' : 'waits'} on {waitsOn.length === 1 ? 'a todo that isn’t done' : `${waitsOn.length} todos that aren’t done`}
        </span>
      )}
      {held.length > 0 && (
        <button
          type="button"
          className="ca-quick-todo-wait ca-quick-todo-primary"
          title={held.map(({ todo: t, at }) => `“${at.session.title}” (${at.session.account} · ${at.lane.callsign}) is ${forTodos.length > 1 ? `“${t.title || 'Untitled'}”’s` : 'its'} primary session. Click to show it.`).join('\n')}
          onClick={() => showSession(held[0].at.lane, held[0].at.session)}
        >
          <Icon name="user" size={12} />
          <span className="ca-quick-todo-primary-text">{held.length === 1 ? `“${held[0].at.session.title}” is on it` : `${held.length} already have a session`}</span>
        </button>
      )}
      {marked.length > 0 && (
        <span className="ca-quick-todo-wait" title={`Starting a session on ${forTodos.length > 1 ? 'them' : 'it'} ends the wait.`}>
          <Icon name="hourglass" size={12} />
          {forTodos.length > 1 ? `${marked.length === 1 ? 'one' : marked.length} marked as waiting` : todo.waiting?.note ? `waiting for ${todo.waiting.note}` : 'marked as waiting'}
        </span>
      )}
      <button type="button" className="ca-linkbtn" title={forTodos.length > 1 ? "Start a session that isn't linked to these todos" : "Start a session that isn't linked to this todo"} onClick={unlink}>
        Don’t link
      </button>
    </div>
  );
}

/** What the panel has to say, under the list, the first of: what went wrong, what a menu did,
 *  where Improve Codealong's source is, and a calm machine while the start's is strained. */
export function QuickNotes({ error, info, improveNote, addCodealongProject, calmer, startsOnLoad, startsOnName, takeCalmer }: { error: string | null; info: string | null; improveNote: ReturnType<typeof improveWords>; addCodealongProject: () => void; calmer: Calmer | null; startsOnLoad: Load | null | undefined; startsOnName: string; takeCalmer: (c: Calmer) => void }) {
  return (
    <>
      {error && (
        <p className="ca-quick-error" role="alert">
          <Icon name="alert" size={14} />
          <span>{error}</span>
        </p>
      )}
      {info && !error && (
        <p className="ca-quick-error is-info" role="status">
          <Icon name="check" size={14} />
          <span>{info}</span>
        </p>
      )}
      {improveNote && !error && !info && (
        <p className="ca-quick-error is-info ca-quick-improve" role="status">
          <Icon name="bulb" size={14} />
          <span>{improveNote.text}</span>
          {improveNote.add && (
            <Button size="sm" onMouseDown={(e) => e.preventDefault()} onClick={addCodealongProject}>
              Add a {CODEALONG_PROJECT} project
            </Button>
          )}
        </p>
      )}
      {calmer && startsOnLoad && !error && !info && !improveNote && (
        <p className="ca-quick-error is-info ca-quick-calmer" role="status">
          <Icon name="gauge" size={14} />
          <span>
            {loadSentence(startsOnLoad, startsOnName)} {calmer.name} is calm{calmer.lane.startsChats ? '' : ` and has ${calmer.lane.repo}`}.
          </span>
          <Button size="sm" title={loadSentence(calmer.load, calmer.name)} onMouseDown={(e) => e.preventDefault()} onClick={() => takeCalmer(calmer)}>
            Use {deviceLabel(calmer.name)} <Kbd keys={chordKeys(CALMER_CHORD)} />
          </Button>
        </p>
      )}
    </>
  );
}
