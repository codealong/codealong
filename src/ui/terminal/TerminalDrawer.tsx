// A session's terminals of Codealong's own, in a drawer under its box (terminals.rs): a strip of
// tabs, one a terminal, with + for another, and the one picked under them. Folded, it's the strip
// alone; with no terminals, nothing, until one is opened (the session's New terminal, ⌃`, or Run
// on a command). The drawer is as tall as you left it last, dragged by its top edge, in every
// session. What a box sends goes to the session, never to a terminal: the terminal takes its keys
// itself, so the drawer sits under the box rather than between it and the conversation.

import { useEffect, useRef, useState } from 'react';
import { errorText } from '../../report';
import { terminalClose, terminalPutBack, terminalWindow } from '../../api';
import { useStored, type Codec } from '../../stored';
import { drawerOf, openTerminal, setDrawer, shownOf, terminalName, useDrawer, usePoppedTerminals } from '../../terminals';
import type { TerminalGlance } from '../../types';
import { Button, cx, Icon } from '../primitives';
import { TerminalView } from './TerminalView';

const HEIGHT: Codec<number> = {
  parse: (raw) => {
    const n = Number(raw);
    return Number.isFinite(n) && n > 0 ? n : undefined;
  },
  format: (n) => String(Math.round(n)),
};
const DEFAULT_HEIGHT = 260;
const LEAST_HEIGHT = 96;

export interface TerminalDrawerProps {
  sessionKey: string;
  /** Where a new one starts: the session's folder on this Mac. */
  folder: string;
  terminals: TerminalGlance[];
  /** ⌃` in a terminal: the keyboard goes back to the box. */
  onLeave: () => void;
}

/** What a tab says of how its terminal stands, beside its name. */
function stateOf(t: TerminalGlance): { word: string; tone: 'quiet' | 'busy' | 'failed' | 'ended' } | null {
  if (!t.running) return { word: t.exit === 0 ? 'done' : `exit ${t.exit ?? '?'}`, tone: t.exit === 0 ? 'ended' : 'failed' };
  if (t.program) return { word: t.program, tone: 'busy' };
  return null;
}

export function TerminalDrawer({ sessionKey, folder, terminals, onLeave }: TerminalDrawerProps) {
  const [error, setError] = useState<string | null>(null);
  const onError = (e: unknown) => setError(errorText(e));
  const drawer = useDrawer(sessionKey);
  const [height, setHeight] = useStored('terminal-drawer-height', HEIGHT, DEFAULT_HEIGHT);
  const shown = shownOf(drawer, terminals);
  const popped = usePoppedTerminals();
  const self = useRef<HTMLDivElement>(null);

  // A terminal that closed elsewhere leaves the tab it was for: the drawer shows another.
  useEffect(() => {
    if (drawer.active && !terminals.some((t) => t.id === drawer.active)) setDrawer(sessionKey, { active: terminals[terminals.length - 1]?.id ?? null });
  }, [drawer.active, terminals, sessionKey]);

  // One that opens while the session is shown (Run on a command, an agent's you allowed) shows
  // itself, leaving the keyboard where it was; those it had when it was shown stay as they were.
  const seen = useRef<Set<string> | null>(null);
  useEffect(() => {
    const now = new Set(terminals.map((t) => t.id));
    if (seen.current) {
      const fresh = terminals.filter((t) => !seen.current?.has(t.id) && Date.now() - t.startedAt < 15_000);
      const last = fresh[fresh.length - 1];
      if (last && drawerOf(sessionKey).active !== last.id) setDrawer(sessionKey, { open: true, active: last.id });
    }
    seen.current = now;
  }, [terminals, sessionKey]);

  if (!terminals.length) return null;
  const open = drawer.open && !!shown;
  // Popped out into a window of its own, it's drawn there, not here too.
  const away = !!shown && popped.includes(shown.id);
  const another = () => void openTerminal(sessionKey, shown?.folder || folder).catch(onError);
  const close = (t: TerminalGlance) => void terminalClose(t.id).catch(onError);
  const drag = (e: React.PointerEvent<HTMLDivElement>) => {
    const start = e.clientY;
    const from = height;
    const panel = self.current?.parentElement?.getBoundingClientRect().height ?? 800;
    const handle = e.currentTarget;
    handle.setPointerCapture(e.pointerId);
    const move = (m: PointerEvent) => setHeight(Math.min(panel * 0.8, Math.max(LEAST_HEIGHT, from + start - m.clientY)));
    const up = () => {
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('pointerup', up);
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', up);
  };
  return (
    <section ref={self} className={cx('ca-termdrawer', open && 'is-open')} aria-label="Terminals" style={open ? { height } : undefined}>
      {open && <div className="ca-termdrawer-grip" role="separator" aria-orientation="horizontal" aria-label="Drag to resize the terminals" onPointerDown={drag} onDoubleClick={() => setHeight(DEFAULT_HEIGHT)} />}
      <div className="ca-termdrawer-tabs" role="tablist" aria-label="Terminals">
        {terminals.map((t) => {
          const state = stateOf(t);
          const active = open && t.id === shown?.id;
          return (
            <div key={t.id} className={cx('ca-termtab', active && 'is-active', !t.running && 'is-ended')}>
              <button
                type="button"
                role="tab"
                aria-selected={active}
                className="ca-termtab-name"
                title={`${terminalName(t)} · ${t.folder}`}
                onClick={() => setDrawer(sessionKey, active ? { open: false } : { open: true, active: t.id, focusAt: Date.now() })}
              >
                <Icon name="terminal" size={12} />
                <span className="ca-termtab-label">{terminalName(t)}</span>
                {state && <span className={cx('ca-termtab-state', `is-${state.tone}`)}>{state.word}</span>}
              </button>
              <button type="button" className="ca-termtab-close" aria-label={`Close ${terminalName(t)}`} title={t.running && t.program ? `Close it, ending ${t.program}` : 'Close it'} onClick={() => close(t)}>
                <Icon name="x" size={12} />
              </button>
            </div>
          );
        })}
        <button type="button" className="ca-termdrawer-act" aria-label="New terminal" title="New terminal in this session’s folder" onClick={another}>
          <Icon name="plus" size={14} />
        </button>
        <span className="ca-termdrawer-gap" />
        {open && shown && !away && (
          <button type="button" className="ca-termdrawer-act" aria-label="Pop it out" title="Pop it out into a window of its own" onClick={() => void terminalWindow(sessionKey, shown.id, true).catch(onError)}>
            <Icon name="picture-in-picture-on" size={14} />
          </button>
        )}
        <button type="button" className="ca-termdrawer-act" aria-label={open ? 'Fold the terminals' : 'Show the terminals'} title={open ? 'Fold the terminals' : 'Show the terminals'} onClick={() => setDrawer(sessionKey, { open: !open, focusAt: open ? drawer.focusAt : Date.now() })}>
          <Icon name={open ? 'chevron-down' : 'chevron-up'} size={14} />
        </button>
      </div>
      {error && (
        <p className="ca-term-failed" role="status">
          {error}
        </p>
      )}
      {open && shown && away && (
        <div className="ca-term-away">
          <span>It’s in a window of its own.</span>
          <Button size="sm" variant="ghost" onClick={() => void terminalWindow(sessionKey, shown.id, true).catch(onError)}>
            Show its window
          </Button>
          <Button size="sm" onClick={() => void terminalPutBack(shown.id).catch(onError)}>
            Put it back here
          </Button>
        </div>
      )}
      {open && shown && !away && <TerminalView key={shown.id} id={shown.id} focusAt={drawer.focusAt} onLeave={onLeave} />}
    </section>
  );
}
