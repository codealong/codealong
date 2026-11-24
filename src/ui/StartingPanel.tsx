// A pending start whose session isn't on the board yet (`Board.starting`, starting.rs): shown as
// its own panel in the main window (MainPanel.tsx) and in floating cards (SessionCard.tsx) when
// you click its row in the Sessions list or under its parent session's title. It carries what the
// parent's header used to crowd inline: the terminal's screen and choices to answer it with, Show
// in its terminal where its tab was found, and Mark as done (⇧⌘E), which closes its tab when found
// and stops waiting for it (`start_stop`).

import { useCallback, useEffect, useState } from 'react';
import { answerStart, openTarget, stopStart } from '../api';
import type { MenuSpec } from '../contextMenu';
import { duration } from '../format';
import { useLatest } from '../latest';
import { screenHeadline, startingState, startingTitle } from '../lineage';
import * as menus from '../menus';
import { chordPressed, held, keysIn } from '../platform';
import { errorText } from '../report';
import { snoozeChordIndex } from '../snooze';
import type { Lane, Session, Starting } from '../types';
import { cardZoom } from '../zoom';
import { dialogUp } from './focusTrap';
import { Button, cx, Icon, PanelBack, SessionGlyph, type PanelBackTo } from './primitives';
import { QueueStrip, type Hotkeys, type Notice, type Queue, type StripAction } from './SessionPanel';

export interface StartingPanelProps {
  starting: Starting;
  parent: { lane: Lane; session: Session };
  now: number;
  variant?: 'panel' | 'card';
  zoom?: number;
  queue?: Queue;
  notice?: Notice | null;
  hotkeys?: Hotkeys;
  back: PanelBackTo;
  /** Called once Mark as done stops/forgets this start. */
  onDone: () => void;
  /** Opens the worktree it was started in, where the host can show it. */
  onShowLane?: () => void;
  /** Closes a card window (peek, kept desktop card). */
  onClose?: () => void;
  closeTitle?: string;
}

/** Tooltip for Mark as done on a pending start: closes its tab when found, otherwise stops waiting. */
export function startingDoneTitle(s: Starting): string {
  return s.pid != null
    ? 'Mark as done: closes its tab, ending it before it starts, and stops waiting for it'
    : 'Mark as done: stops waiting for it. Codealong can’t tell which tab is its: close it yourself if it’s still open.';
}

/** Context menu for a pending start's row in the Sessions list or under its parent's title. */
export function startingMenu(s: Starting, onDone: () => void, onError: (why: string) => void): MenuSpec {
  const terminal = s.terminal ?? 'its terminal';
  return [
    ...(s.pid != null
      ? [
          {
            label: `Show in ${terminal}`,
            run: () => void openTarget(s.lane, `start|${s.at}|focus`).catch((e: unknown) => onError(errorText(e))),
          },
        ]
      : []),
    {
      label: 'Mark as done',
      accelerator: menus.CHORDS.done,
      run: () => void stopStart(s.at).then(onDone, (e: unknown) => onError(errorText(e))),
    },
  ];
}

export function StartingPanel({ starting: s, parent, now, variant = 'panel', zoom, queue, notice, hotkeys, back, onDone, onShowLane, onClose, closeTitle }: StartingPanelProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const terminal = s.terminal ?? 'its terminal';
  const who = `${s.harness} on ${s.account}`;
  const where = s.terminal ? `in ${s.terminal}, in ${s.place}` : `in ${s.place}`;
  const headline = s.screen ? screenHeadline(s.screen) : '';
  const state = startingState(s);
  const doneHelp = startingDoneTitle(s);

  const act = useCallback((work: () => Promise<void>, after?: () => void) => {
    setBusy(true);
    setError(null);
    work()
      .then(() => after?.())
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setBusy(false));
  }, []);

  const runDone = useCallback(() => {
    if (busy) return;
    act(() => stopStart(s.at), onDone);
  }, [act, busy, onDone, s.at]);

  const showTab = s.pid != null ? () => act(() => openTarget(s.lane, `start|${s.at}|focus`)) : undefined;

  const doneNow = useLatest(runDone);
  const panel = variant === 'panel';
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing || dialogUp()) return;
      if (chordPressed(menus.CHORDS.done ?? '', e)) {
        // Taken ahead of the parent session selected behind this panel (mainKeys.ts / SessionCard.tsx).
        e.preventDefault();
        doneNow.current();
        return;
      }
      if (!panel) return;
      // Swallow session chords so they never act on the parent session while this helper is open.
      if (
        chordPressed(menus.CHORDS.mergeDone ?? '', e) ||
        chordPressed(menus.CHORDS.close ?? '', e) ||
        chordPressed(menus.CHORDS.archive ?? '', e) ||
        chordPressed(menus.CHORDS.rename ?? '', e) ||
        chordPressed(menus.CHORDS.pin ?? '', e) ||
        (held(e, 'Ctrl', 'Cmd') && snoozeChordIndex(e.key.toLowerCase()) >= 0)
      ) {
        e.preventDefault();
      }
    };
    window.addEventListener('keydown', onKey, panel);
    return () => window.removeEventListener('keydown', onKey, panel);
  }, [doneNow, panel]);

  const stripButtons: StripAction[] = [
    { label: 'Done', chord: menus.CHORDS.done, title: doneHelp, run: busy ? undefined : runDone },
  ];

  const said = s.stuck
    ? s.screen
      ? `${who} hasn’t made its session ${duration(s.at, now)} after it was started ${where}. Its terminal, which may be behind your windows, is waiting on this prompt before it starts.`
      : `${who} hasn’t made its session ${duration(s.at, now)} after it was started ${where}. Its terminal, which may be behind your windows, may be waiting on a prompt of its own or may have closed before starting.`
    : `${who} is starting ${where}. Its conversation will appear once ${s.harness} creates a session.`;

  const body = (
    <>
      <QueueStrip
        queue={queue}
        notice={notice}
        hotkeys={hotkeys}
        buttons={stripButtons}
        of={s.helper ? 'this helper' : 'this start'}
        back={hotkeys ? back : undefined}
      />
      <div className="ca-sessinsp-head">
        {!hotkeys && <PanelBack back={back} />}
        <button
          type="button"
          className="ca-sessinsp-kin is-parent"
          title={`Back to ${parent.session.title}`}
          onClick={back.onBack}
        >
          <span className="ca-sessinsp-kin-phrase">{s.helper ? 'Helper of' : 'Started by'}</span>
          <SessionGlyph s={parent.session} size={11} />
          <span className="ca-sessinsp-kin-title">{parent.session.title}</span>
        </button>
        <div className="ca-sessinsp-top">
          <SessionGlyph s={{ status: s.stuck ? 'needs' : 'working' }} size={12} />
          <h2 className="ca-sessinsp-title">{startingTitle(s)}</h2>
          <div className="ca-sessinsp-tools">
            {showTab && (
              <Button size="sm" variant="secondary" icon="terminal" disabled={busy} title={`Shows its tab in ${terminal}, to answer it there.`} onClick={showTab}>
                Show in {terminal}
              </Button>
            )}
            {!hotkeys && (
              <Button size="sm" variant="ghost" icon="check" disabled={busy} title={keysIn(`${doneHelp} (⇧⌘E)`)} onClick={runDone}>
                Done
              </Button>
            )}
            {onClose && <Button variant="ghost" size="sm" icon="x" aria-label="Close" title={keysIn(closeTitle ?? 'Close (esc)')} onClick={onClose} />}
          </div>
        </div>
        <div className="ca-sessinsp-meta">
          <span className={cx(s.stuck && 'ca-startpanel-stuck')}>{state}</span>
          <span aria-hidden="true">·</span>
          {onShowLane ? (
            <button type="button" className="ca-sessinsp-lane" title={`Show ${s.place}`} onClick={onShowLane}>
              <Icon name="folder" size={12} />
              <span>{s.place}</span>
            </button>
          ) : (
            <span className="ca-sessinsp-lane">
              <Icon name="folder" size={12} />
              <span>{s.place}</span>
            </span>
          )}
          <span aria-hidden="true">·</span>
          <span>{who}</span>
          <span aria-hidden="true">·</span>
          <span>{duration(s.at, now)}</span>
        </div>
      </div>

      <div className="ca-startpanel-body">
        <p className="ca-startpanel-said">{said}</p>
        {s.screen && <pre className="ca-startpanel-screen">{s.screen}</pre>}
        {s.stuck && s.choices.length > 0 && (
          <div className="ca-startpanel-choices" role="group" aria-label={headline ? `Answer “${headline}”` : `${s.harness} waits in ${terminal}`}>
            {s.choices.map((c) => (
              <Button
                key={c.number}
                size="sm"
                disabled={busy}
                title={`Picks “${c.words}” in its terminal, as you would there.`}
                onClick={() => act(() => answerStart(s.at, headline, c.number, c.words))}
              >
                {c.words}
              </Button>
            ))}
          </div>
        )}
        {s.stuck && s.pid == null && (
          <p className="ca-startpanel-note">
            Codealong can’t tell which tab in {terminal} is its. If it’s still open, answer or close it there — or mark <b>Done</b> ({keysIn('⇧⌘E')}) to stop waiting for it.
          </p>
        )}
        {s.opens && (
          <div className="ca-startpanel-prompt">
            <span className="ca-startpanel-label">Prompt</span>
            <p className="ca-startpanel-opens">{s.opens}</p>
          </div>
        )}
        {error && (
          <p className="ca-startpanel-error" role="alert">
            {error}
          </p>
        )}
      </div>
    </>
  );

  if (variant === 'card') {
    return (
      <div className="ca-reply ca-startpanel" style={cardZoom(zoom)}>
        {body}
      </div>
    );
  }
  return (
    <aside className="ca-insp ca-sessinsp ca-startpanel" aria-label={`${startingTitle(s)} details`}>
      {body}
    </aside>
  );
}
