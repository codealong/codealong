// A schedule's panel, beside the Schedules view's list: when it runs and when next, what it does
// and what it's told, which of your machines runs it next, how its watch's last look went, the
// session it keeps going, and its latest runs, each with why it ran, how it went and on which
// machine, its session a click away. Run now, Pause and Edit
// are its buttons; its menu (menus.ts `scheduleMenu`) has those and the rest.

import { absolute, ago, agoPhrase } from '../format';
import { nextWords, OUTCOME_ICON, OUTCOME_WORD, whenIcon } from '../schedules';
import type { Lane, ScheduleItem, Session } from '../types';
import { Button, Icon, SessionGlyph, cx, headerMenu } from './primitives';

export interface SchedulePanelProps {
  item: ScheduleItem | null;
  now: number;
  /** A session and its worktree by key, where the board has it. */
  sessionOf: (key: string) => { lane: Lane; session: Session } | null;
  onShowSession: (lane: Lane, session: Session) => void;
  onRun: () => void;
  onPause: (paused: boolean) => void;
  onEdit: () => void;
  onMenu: (at?: { x: number; y: number }) => void;
}

/** What a schedule does besides start or send, in a sentence each. */
function settingsWords(s: ScheduleItem): string[] {
  const out: string[] = [];
  const r = s.run;
  if (r.to !== 'session') {
    const on = [r.account.trim() || 'Auto', r.model, r.effort].filter(Boolean).join(', ');
    out.push(`On ${on}, ${r.place ? `in ${r.place.replace('-', ' ')}` : 'out of sight'}${r.worktree ? ', in a new worktree each time' : ''}.`);
  }
  if (r.to === 'thread' && r.freshAfter.trim()) out.push(`A new session once its session is ${r.freshAfter.trim()} old.`);
  if (s.quiet) out.push('Its sessions are quiet: they wait on you only when they need you.');
  if (s.notes) out.push('Each run reads and adds to its notes.');
  if (r.to === 'new') out.push(s.overlap === 'allow' ? 'Starts another while the last run still works.' : 'Skips a run while the last one still works or waits on you.');
  out.push(s.catchUp === 'once' ? 'A time missed while the Mac slept or Codealong was closed runs once when it’s back.' : 'A time missed while the Mac slept or Codealong was closed is skipped.');
  if (s.perDay) out.push(`At most ${s.perDay} run${s.perDay === 1 ? '' : 's'} a day.`);
  return out;
}

export function SchedulePanel({ item, now, sessionOf, onShowSession, onRun, onPause, onEdit, onMenu }: SchedulePanelProps) {
  if (!item) {
    return (
      <aside className="ca-insp ca-insp--empty" aria-label="Schedule details">
        Select a schedule to see when it runs and how its runs went.
      </aside>
    );
  }
  const s = item;
  const maker = s.createdBy ? sessionOf(s.createdBy) : null;
  const thread = s.thread ? sessionOf(s.thread) : null;
  const target = s.run.to === 'session' ? sessionOf(s.run.session) : null;
  const next = nextWords(s, now);
  const sessionButton = (found: { lane: Lane; session: Session }, phrase: string) => (
    <button type="button" className="ca-sessinsp-kin" title={`Show “${found.session.title}”`} onClick={() => onShowSession(found.lane, found.session)}>
      <span className="ca-sessinsp-kin-phrase">{phrase}</span>
      <SessionGlyph s={found.session} size={11} />
      <span className="ca-sessinsp-kin-title">{found.session.title}</span>
    </button>
  );
  return (
    <aside className="ca-insp ca-schedinsp" aria-label="Schedule details">
      <header onContextMenu={headerMenu(() => onMenu())}>
        <div className="ca-insp-title">
          <Icon name={whenIcon(s.when)} size={14} />
          <span className="ca-tag">{s.problem ? 'Can’t run' : s.paused ? 'Paused' : 'Active'}</span>
          <Button variant="ghost" size="sm" icon="dots" aria-label="More" title="Run, pause, change or delete it" onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            onMenu({ x: r.left, y: r.bottom });
          }} />
        </div>
        <h3 className="ca-schedinsp-name">{s.name}</h3>
        <div className="ca-schedinsp-meta">
          <span title={absolute(s.createdAt)}>Added {agoPhrase(s.createdAt, now)}</span>
          {maker ? <span>by an agent, “{maker.session.title}”</span> : s.createdBy ? <span>by an agent</span> : null}
        </div>
      </header>
      <div className="ca-insp-actions">
        <Button icon="player-play" title="Run it now, as its time coming would" onClick={onRun}>
          Run now
        </Button>
        <Button icon={s.paused ? 'player-play' : 'player-pause'} title={s.paused ? 'Let it run again from its next time' : 'Nothing runs it until you resume it'} onClick={() => onPause(!s.paused)}>
          {s.paused ? 'Resume' : 'Pause'}
        </Button>
        <Button icon="pencil" title="Change when it runs and what it does" onClick={onEdit}>
          Edit
        </Button>
      </div>
      {s.problem && (
        <div className="ca-schedproblem" role="alert">
          <Icon name="alert" size={14} />
          <span>{s.problem}</span>
        </div>
      )}
      {s.createdBy && <p className="ca-schedinsp-note">An agent made it, so what it sends goes as Codealong’s, not as yours, and the sessions it starts are that agent’s.</p>}

      <section className="ca-insp-sec">
        <h4 className="ca-label">When</h4>
        <p className="ca-schedinsp-line">
          {s.whenWords}
          {next && <span className="ca-schedinsp-next"> · {next}</span>}
        </p>
        {s.when.kind === 'watch' && <code className="ca-schedinsp-code">{s.when.command}</code>}
        {s.when.kind === 'watch' && s.check && (
          <p className={cx('ca-schedinsp-check', !s.check.ok && 'is-failing')} title={absolute(s.check.at)}>
            {s.check.ok ? 'Last look' : s.check.failures > 1 ? `Its last ${s.check.failures} looks failed` : 'Its last look failed'} {agoPhrase(s.check.at, now)}: {s.check.said}
          </p>
        )}
        {s.when.kind === 'fired' && (
          <p className="ca-schedinsp-note">
            Run it from a script with <code>codealong schedule fire</code>; <code>codealong schedule fire --help</code> says how.
          </p>
        )}
        {s.onlyIf.trim() && (
          <>
            <p className="ca-schedinsp-note">Only when this succeeds and prints something:</p>
            <code className="ca-schedinsp-code">{s.onlyIf}</code>
          </>
        )}
      </section>

      {s.runsOnWords && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">Runs on</h4>
          <p className="ca-schedinsp-line">
            {s.runsOn?.kind === 'every' ? 'Every one of your machines, each on its own' : 'One of your machines each time'}
            {s.runsOn?.kind === 'one' && <span className="ca-schedinsp-next"> · {s.runsOnWords}</span>}
          </p>
          {s.here && <p className="ca-schedinsp-note">Not on this machine: {s.here}</p>}
        </section>
      )}

      <section className="ca-insp-sec">
        <h4 className="ca-label">Does</h4>
        {s.run.to === 'session' ? (
          target ? sessionButton(target, 'Sends to') : <p className="ca-schedinsp-line">Sends to a session that’s no longer on the board.</p>
        ) : (
          <p className="ca-schedinsp-line">{s.run.to === 'thread' ? 'Keeps one session going: the first run starts it, and the rest are sent to it.' : 'Starts a new session each time.'}</p>
        )}
        {thread && sessionButton(thread, 'Its session')}
        <ul className="ca-schedinsp-settings">
          {settingsWords(s).map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
        <div className="ca-schedinsp-prompt">{s.run.prompt}</div>
      </section>

      {s.queued && (
        <section className="ca-insp-sec">
          <h4 className="ca-label">Waiting</h4>
          <p className="ca-schedinsp-line">
            Since {ago(s.queued.at, now)} ago, for its session’s turn to end: {s.queued.why.join('; ')}.
          </p>
        </section>
      )}

      <section className="ca-insp-sec">
        <h4 className="ca-label">Runs</h4>
        {s.runs.length === 0 && <p className="ca-schedinsp-note">{s.asked > 0 ? 'Asked to run: it runs in a few seconds.' : 'It hasn’t run yet.'}</p>}
        <div className="ca-schedruns">
          {s.runs.map((r) => {
            const found = r.session ? sessionOf(r.session) : null;
            return (
              <div key={`${r.at}-${r.why}`} className={cx('ca-schedrun', `is-${r.outcome}`)}>
                <Icon name={OUTCOME_ICON[r.outcome]} size={12} className="ca-schedrun-icon" />
                <div className="ca-schedrun-main">
                  <div className="ca-schedrun-line">
                    <b>{OUTCOME_WORD[r.outcome]}</b>
                    <span className="ca-schedrun-why">{r.why}</span>
                    {r.machine && <span className="ca-schedrun-on">on {r.machine}</span>}
                    <span className="ca-schedrun-at" title={absolute(r.at)}>
                      {agoPhrase(r.at, now)}
                    </span>
                  </div>
                  {r.pending && <span className="ca-schedrun-said">Its session hasn’t shown yet.</span>}
                  {r.said && <span className="ca-schedrun-said">{r.said}</span>}
                  {found && sessionButton(found, 'Session')}
                </div>
              </div>
            );
          })}
        </div>
      </section>
    </aside>
  );
}
