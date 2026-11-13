// What a session's agent left running in the background (Session.tasks), in its header: a chip
// among its links that says how many run, and the list it opens under them, each command with
// where it stands, the last of what it printed, and Stop. Its harness's own list, as Claude Code's
// /tasks and Codex's /ps show it; the words are background.ts's.

import { useEffect, useState } from 'react';
import { sessionTaskOutput, sessionTaskStop, openUrl } from '../api';
import { lastLines, localAddress, taskChipTitle, taskChipWords, taskName, taskState } from '../background';
import { errorText } from '../report';
import type { BackgroundTask, Session } from '../types';
import { Button, Chip, cx, Icon } from './primitives';

/** How many lines of a command's output its row shows. */
const OUTPUT_LINES = 40;

/** How often a running command's output is read again while it's shown. */
const OUTPUT_EVERY_MS = 4000;

/** The chip among a session's links: how many of its background commands run, else how many
 *  ended a little while ago. A click shows the list under the links (`BackgroundList`). */
export function BackgroundChip({ session: s, now, open, onToggle }: { session: Session; now: number; open: boolean; onToggle: () => void }) {
  const words = taskChipWords(s);
  if (!words) return null;
  const running = (s.tasks ?? []).some((t) => t.endedAt == null);
  return (
    <Chip className={cx('ca-taskchip', running ? 'is-running' : 'is-ended')} title={taskChipTitle(s, now)} does={open ? 'hide the list' : 'list them'} act={onToggle} expanded={open}>
      <Icon name="terminal" size={12} />
      <span>{words}</span>
    </Chip>
  );
}

/** The commands under the links, a row each: running ones first, as the engine orders them. */
export function BackgroundList({ session: s, now, remote }: { session: Session; now: number; remote: boolean }) {
  const tasks = s.tasks ?? [];
  if (tasks.length === 0) return null;
  return (
    <div className="ca-tasks" role="list" aria-label="Running in the background">
      {tasks.map((t) => (
        <TaskRow key={t.id} sessionKey={s.key} task={t} now={now} remote={remote} />
      ))}
    </div>
  );
}

/** One command: its name and state, what it's for, and on demand the last of what it printed. A
 *  running one can be stopped, which its harness then tells the agent of. */
function TaskRow({ sessionKey, task: t, now, remote }: { sessionKey: string; task: BackgroundTask; now: number; remote: boolean }) {
  const [showing, setShowing] = useState(false);
  const [output, setOutput] = useState<{ text: string } | { error: string } | null>(null);
  const [stop, setStop] = useState<{ busy: boolean; said?: string }>({ busy: false });
  const running = t.endedAt == null;

  // Read while shown, and again every few seconds while it runs: a server's lines come as it goes.
  useEffect(() => {
    if (!showing) return;
    let live = true;
    const read = () =>
      sessionTaskOutput(sessionKey, t.id).then(
        (text) => live && setOutput({ text }),
        (e) => live && setOutput({ error: errorText(e) }),
      );
    void read();
    const timer = running ? window.setInterval(read, OUTPUT_EVERY_MS) : undefined;
    return () => {
      live = false;
      window.clearInterval(timer);
    };
  }, [showing, running, sessionKey, t.id]);

  const stopIt = () => {
    setStop({ busy: true });
    sessionTaskStop(sessionKey, t.id).then(
      (said) => setStop({ busy: false, said }),
      (e) => setStop({ busy: false, said: errorText(e) }),
    );
  };

  const text = output && 'text' in output ? output.text : '';
  // A server's address opens here only for one that runs on this machine.
  const address = remote ? null : localAddress(text);
  return (
    <div className={cx('ca-task', running ? 'is-running' : 'is-ended')} role="listitem">
      <div className="ca-task-head">
        <Icon name={t.monitor ? 'eye' : 'terminal'} size={12} />
        <code className="ca-task-command" title={t.command || undefined}>
          {taskName(t)}
        </code>
        <span className="ca-task-state">{taskState(t, now)}</span>
        <button type="button" className="ca-linkbtn ca-task-toggle" aria-expanded={showing} title={showing ? 'Hide what it printed' : 'Show the last of what it printed'} onClick={() => setShowing(!showing)}>
          {showing ? 'Hide output' : 'Output'}
        </button>
        {running && (
          <Button size="sm" disabled={stop.busy} title="End its process and what it started; the agent is told it ended" onClick={stopIt}>
            {stop.busy ? 'Stopping…' : 'Stop'}
          </Button>
        )}
      </div>
      {(t.about || t.monitor) && <div className="ca-task-about">{[t.monitor ? 'Monitor: each line it prints goes to the agent' : '', t.about ?? ''].filter(Boolean).join(' · ')}</div>}
      {stop.said && <div className="ca-task-said">{stop.said}</div>}
      {showing && (
        <div className="ca-task-output">
          {output == null ? (
            <span className="ca-task-note">Reading…</span>
          ) : 'error' in output ? (
            <span className="ca-task-note">{output.error}</span>
          ) : text.trim() ? (
            <pre>{lastLines(text, OUTPUT_LINES)}</pre>
          ) : (
            <span className="ca-task-note">It hasn’t printed anything yet.</span>
          )}
          {address && (
            <button type="button" className="ca-linkbtn ca-task-address" title={`Open ${address} in the browser`} onClick={() => void openUrl(address).catch(() => {})}>
              <Icon name="world" size={12} />
              {address.replace(/^https?:\/\//, '')}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
