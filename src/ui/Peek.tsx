// The floating bar's peek (peek.rs has its window): the session whose pill is under the pointer,
// its conversation, and the box that answers it (ReplyView.tsx). A click in it gives it the
// keyboard; ⏎ sends, and once a message or an answer went through the peek closes, so the app
// you were in has the keyboard back; after an answer it stays while the box holds something for
// you to send next. Escape closes it too, and what you'd typed stays with the session (drafts.ts),
// here and in its panels, until you send it. Its header's ✓ marks a finished turn seen and its
// archive button archives the session, either of which takes it off the bar. While it's up, the
// session follows the board, and the peek stays on it as its pill moves along the bar.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { archiveSession, barNote, getBoard, markSeen, inPreview, onBoard, onPeekHidden, onPeekOpen, onPeekShown, openFromBar, peekFit, peekFocus, peekHide, peekReady, showMain } from '../api';
import { readDraft } from '../drafts';
import { attentionRows, ALL_PROJECTS } from '../quick';
import type { Lane, PeekOpen, Session, Theme } from '../types';
import { useKeyPerf } from '../perf';
import { ReplyView } from './ReplyView';
import { applyTheme } from './ThemeSwitch';

/** Room around the card for its shadow: the window (peek.rs) is the card, this on its sides and
 *  the side away from the bar, and `NEAR` on the side toward the bar, where it mustn't cover the
 *  bar's pills. */
const MARGIN = 20;
const NEAR = 8;
/** How long an open waits for the conversation before showing what it has. */
const LOAD_WAIT_MS = 250;
/** Where a preview (scripts/preview.sh) says which session to peek at. */
const PREVIEW_KEY = 'codealong.preview.peek';

function useNow(everyMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), everyMs);
    return () => window.clearInterval(id);
  }, [everyMs]);
  return now;
}

/** A preview has no engine to open the peek: it opens on the session named for it, else the first
 *  one that needs you. */
async function previewOpen(): Promise<PeekOpen | null> {
  const board = await getBoard();
  let key: string | null = null;
  try {
    key = window.localStorage.getItem(PREVIEW_KEY);
  } catch {
    // No storage: the first session that needs you.
  }
  key = key || attentionRows(board, ALL_PROJECTS).rows[0]?.session.key || null;
  const lane = board.lanes.find((l) => l.sessions.some((s) => s.key === key));
  const session = lane?.sessions.find((s) => s.key === key);
  if (!lane || !session) return null;
  return { id: 1, lane, session, terminal: board.apps.terminal, theme: board.theme, room: 608, below: true, focus: false };
}

export function Peek() {
  const [open, setOpen] = useState<PeekOpen | null>(null);
  // The session as the board has it now, while the peek is up.
  const [live, setLive] = useState<{ lane: Lane; session: Session; theme: Theme } | null>(null);
  const [shown, setShown] = useState(false);
  const [engaged, setEngaged] = useState(false);
  const now = useNow(30_000);
  const card = useRef<HTMLDivElement>(null);
  // The open answered with its height, and the height last reported after it.
  const answered = useRef<number | null>(null);
  const reported = useRef(0);
  // The session whose conversation is laid out.
  const loadedView = useRef<string | null>(null);

  useEffect(() => {
    const offOpen = onPeekOpen((o) => {
      setOpen(o);
      setLive(null);
      setShown(true);
      setEngaged(o.focus);
    });
    const offHidden = onPeekHidden(() => {
      setShown(false);
      setEngaged(false);
      loadedView.current = null;
    });
    const offShown = onPeekShown((s) => setEngaged(s.engaged));
    if (inPreview) {
      void previewOpen().then((o) => {
        if (!o) return;
        setOpen(o);
        setShown(true);
      });
    }
    return () => {
      offOpen();
      offHidden();
      offShown();
    };
  }, []);

  const key = open?.session.key ?? null;
  useEffect(() => {
    if (!shown || !key) return;
    return onBoard((b) => {
      const lane = b.lanes.find((l) => l.sessions.some((s) => s.key === key));
      const session = lane?.sessions.find((s) => s.key === key);
      if (lane && session) setLive({ lane, session, theme: b.theme });
    });
  }, [shown, key]);

  const lane = live?.lane ?? open?.lane ?? null;
  const session = live?.session ?? open?.session ?? null;
  const theme = live?.theme ?? open?.theme ?? 'system';
  useEffect(() => applyTheme(theme), [theme]);

  const measure = () => Math.ceil((card.current?.getBoundingClientRect().height ?? 0) + MARGIN + NEAR);

  /** Answers the open with the page's height, once: when the conversation is laid out, or after a
   *  moment without it. */
  const ready = useCallback(() => {
    if (!open || answered.current === open.id) return;
    answered.current = open.id;
    reported.current = measure();
    void peekReady(open.id, reported.current);
  }, [open]);
  useEffect(() => {
    if (!open) return;
    const id = window.setTimeout(ready, LOAD_WAIT_MS);
    return () => window.clearTimeout(id);
  }, [open, ready]);

  // An open for the session already shown (Reply here on the pill it shows) answers at once; a
  // hidden peek shows nothing, so each showing reads the conversation afresh.
  const view = open?.session.key ?? '';
  const loaded = () => {
    loadedView.current = view;
    ready();
  };
  useLayoutEffect(() => {
    if (loadedView.current === view) ready();
  }, [open, view, ready]);

  // Once shown, the window follows the card: new steps, a longer draft, an approval coming up.
  useLayoutEffect(() => {
    const el = card.current;
    if (!el) return;
    const observer = new ResizeObserver(() => {
      if (!open || answered.current !== open.id) return;
      const h = measure();
      if (h === reported.current) return;
      reported.current = h;
      void peekFit(h);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [open, shown]);

  // With the perf log on, how long typed keys take to show.
  const keyed = useKeyPerf(open?.perfLog ?? false);
  useEffect(() => {
    window.addEventListener('keydown', keyed, true);
    return () => window.removeEventListener('keydown', keyed, true);
  }, [keyed]);

  // Escape closes it, and ⌘Q, ⌘H and ⌘W close it rather than going to Codealong: the menu bar is
  // the other app's. Typing with nothing focused goes to the box.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      if (e.key === 'Escape') {
        e.preventDefault();
        void peekHide();
        return;
      }
      if (e.metaKey && !e.ctrlKey && !e.altKey && ['q', 'h', 'w'].includes(e.key.toLowerCase())) {
        e.preventDefault();
        void peekHide();
        return;
      }
      const el = document.activeElement;
      if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && (!el || el === document.body)) {
        document.querySelector<HTMLTextAreaElement>('.ca-peek .ca-composer textarea')?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  if (!open || !lane || !session || !shown) return null;

  const openHere = () => {
    void peekHide();
    void openFromBar(lane.id, session.openTarget).catch((e: unknown) => void barNote({ key: session.key, message: String(e).replace(/^Error: /, ''), tone: 'error' }));
  };

  /** Takes the session off the bar, and puts the peek away. */
  const dismiss = (act: Promise<void>) => {
    void peekHide();
    act.catch((e: unknown) => void barNote({ key: session.key, message: String(e).replace(/^Error: /, ''), tone: 'error' }));
  };

  return (
    <div
      className="ca-peek-root"
      style={{ padding: open.below ? `${NEAR}px ${MARGIN}px ${MARGIN}px` : `${MARGIN}px ${MARGIN}px ${NEAR}px` }}
      onMouseDown={(e) => {
        // The shadow around the card is part of the window; a click there is a click outside.
        if (e.target === e.currentTarget) void peekHide();
      }}
    >
      <div
        ref={card}
        className="ca-peek"
        style={{ maxHeight: open.room - MARGIN - NEAR }}
        onPointerDown={() => {
          // Hovering never gives the peek the keyboard; a click does.
          if (!engaged) void peekFocus();
        }}
      >
        <ReplyView
          key={view}
          lane={lane}
          session={session}
          terminal={open.terminal}
          now={now}
          autoFocus={open.focus}
          onLoaded={loaded}
          onOpen={openHere}
          onSeen={() => dismiss(markSeen([session.key]))}
          onArchive={() => dismiss(archiveSession(session.key, true))}
          onReveal={() => {
            void peekHide();
            void showMain({ kind: 'session', laneId: lane.id, key: session.key });
          }}
          onSent={(sent) => {
            // What you'd want to know goes on the bar: the prompt is on the clipboard, say.
            if (sent.opened) void barNote({ key: session.key, message: sent.message, tone: 'info' });
            void peekHide();
          }}
          onAnswered={() => {
            // What you wrote while it waited is likely what you'd send next.
            if (!readDraft(session.key)) void peekHide();
          }}
        />
      </div>
    </div>
  );
}
