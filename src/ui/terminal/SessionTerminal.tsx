// The terminal a session runs in, for a session started or resumed in Codealong's own terminal
// (`[apps] terminal = "codealong"`, launch/codealong.rs): a switch under its header between its
// conversation and its terminal, and the terminal itself in the conversation's place, where you
// type to the agent directly. Popped out, the panel says where it is, as the drawer does.

import { useState } from 'react';
import { terminalPutBack, terminalWindow } from '../../api';
import { errorText } from '../../report';
import { setDrawer, setSessionView, useDrawer, usePoppedTerminals, type SessionView } from '../../terminals';
import type { TerminalGlance } from '../../types';
import { Button, Icon, Seg } from '../primitives';
import { TerminalView } from './TerminalView';

const VIEWS = [
  { id: 'conversation' as const, label: 'Conversation', icon: 'message' },
  { id: 'terminal' as const, label: 'Terminal', icon: 'terminal' },
];

/** Shows the session's conversation or its terminal: the terminal takes the keyboard. */
export function showView(sessionKey: string, view: SessionView): void {
  setSessionView(sessionKey, view);
  if (view === 'terminal') setDrawer(sessionKey, { homeAt: Date.now() });
}

export function SessionViewSwitch({ sessionKey, home, view }: { sessionKey: string; home: TerminalGlance; view: SessionView }) {
  const popped = usePoppedTerminals();
  const [error, setError] = useState<string | null>(null);
  const away = popped.includes(home.id);
  return (
    <div className="ca-sessview">
      <Seg className="ca-sessview-seg" label="What the panel shows" options={VIEWS} value={view} onPick={(v) => showView(sessionKey, v)} />
      <span className="ca-termdrawer-gap" />
      {error && (
        <span className="ca-term-failed" role="status">
          {error}
        </span>
      )}
      {view === 'terminal' && !away && (
        <button type="button" className="ca-termdrawer-act" aria-label="Pop it out" title="Pop its terminal out into a window of its own" onClick={() => void terminalWindow(sessionKey, home.id, true).catch((e: unknown) => setError(errorText(e)))}>
          <Icon name="picture-in-picture-on" size={14} />
        </button>
      )}
    </div>
  );
}

/** The terminal a session runs in, in its panel. ⌃` there goes back to the conversation and its
 *  box, as it hides a terminal under the box. */
export function SessionTerminal({ sessionKey, home, onLeave }: { sessionKey: string; home: TerminalGlance; onLeave: () => void }) {
  const drawer = useDrawer(sessionKey);
  const popped = usePoppedTerminals();
  const [error, setError] = useState<string | null>(null);
  const onError = (e: unknown) => setError(errorText(e));
  if (popped.includes(home.id)) {
    return (
      <section className="ca-sessterm" aria-label="Its terminal">
        <div className="ca-term-away">
          <span>Its terminal is in a window of its own.</span>
          <Button size="sm" variant="ghost" onClick={() => void terminalWindow(sessionKey, home.id, true).catch(onError)}>
            Show its window
          </Button>
          <Button size="sm" onClick={() => void terminalPutBack(home.id).catch(onError)}>
            Put it back here
          </Button>
        </div>
        {error && (
          <p className="ca-term-failed" role="status">
            {error}
          </p>
        )}
      </section>
    );
  }
  return (
    <section className="ca-sessterm" aria-label="Its terminal">
      <TerminalView key={home.id} id={home.id} focusAt={drawer.homeAt} onLeave={onLeave} />
    </section>
  );
}
