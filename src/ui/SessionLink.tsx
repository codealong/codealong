// A link to a session or a todo in a conversation (Markdown.tsx), as an agent writes one when it
// names another session or a todo to you (codealongLinks.ts): `[its title](codealong://session/…)`.
// A session's wears its status and emblem ahead of its words, both as they are now, not as they
// were when it was written; a bare one reads as the session's title. Hovering it brings up a
// card: the session as a PR's or a todo's panel lists one (PanelSession.tsx), with who runs it
// and what it's doing, and its worktree. A click shows the session as a click on it there does,
// in this window or the main one, ⌘-click opens it where it runs, and a right-click has its menu
// (menus.ts). A session's key written as code is a link the same way, as written. A todo's link
// wears its box and shows the todo on a click.
//
// A link only shows what it names, whatever is written in it: a conversation's text is the
// agent's, and what a tool read to it. One that names no session or todo the window knows (an
// old one, a typo, a board not read yet) is its words, as any link Codealong can't follow is.
//
// The window says what it lists and what it does with a session through the PR links' host
// (PrLink.tsx `usePrLinks`, `linkHost`): a link watches only its own session there.

import { useContext, useSyncExternalStore, type ReactNode } from 'react';
import { showMain } from '../api';
import { sessionLink, sessionNamed, todoLink, todoNamed } from '../codealongLinks';
import { cmd } from '../platform';
import { STATUS_WORD } from '../todos';
import type { Lane, Session } from '../types';
import { ImagesFrom } from './DescriptionEditor';
import { HoverCard, useHoverCard, type HoverCardState } from './HoverCard';
import { PanelSession } from './PanelSession';
import { linkHost, type PrLinkActions } from './PrLink';
import { SessionEmblem, SessionGlyph, WorktreeChip } from './primitives';
import { TodoGlyph } from './TodoBits';

interface LinkProps {
  /** Its words: left out, a bare link's are the session's title or the todo's. */
  children?: ReactNode;
  /** What shows where it names nothing the window knows: its words, or what was written. */
  unknown: ReactNode;
  /** As written, with nothing put ahead of it: a key or an id written as code. */
  plain?: boolean;
}

/** What the window has of what a link names, read again when it hears of a new board. Drawn
 *  outside a page (a test), it's what the window has then. */
function useLinked<T>(read: () => T): T {
  return useSyncExternalStore(linkHost.subscribe, read, read);
}

/** `name`: what the link names the session by, its key or id or the start of one. */
export function SessionLink({ name, children, unknown, plain }: LinkProps & { name: string }) {
  // The session and its worktree as the board has them now: the objects the board holds, so a
  // push re-renders a link only when its own session changed.
  // In a conversation on another of your Macs, the keys written are that Mac's.
  const away = useContext(ImagesFrom);
  const session = useLinked(() => sessionNamed(linkHost.sessions(), name, away)?.session ?? null);
  const lane = useLinked(() => sessionNamed(linkHost.sessions(), name, away)?.lane ?? null);
  const c = useHoverCard();
  // A page that never said what it does with a session (PrLink.tsx `usePrLinks`) has no links.
  if (!session || !lane || !linkHost.actions()) return <>{unknown}</>;
  const opens = !!session.openTarget;
  // What the window does with it now, asked as it's done: the window says so again each time it
  // draws, and a link draws only when its own session changes.
  const act = (f: (a: PrLinkActions) => void) => {
    c.close();
    const actions = linkHost.actions();
    if (actions) f(actions);
  };
  return (
    <>
      <a
        ref={(el) => void (c.anchor.current = el)}
        className={plain ? 'ca-sesslink is-plain' : 'ca-md-a ca-sesslink'}
        href={sessionLink(session.key)}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          const there = cmd(e) && opens;
          act((a) => (there ? a.openSession(lane, session) : a.showSession(lane, session)));
        }}
        onMouseEnter={c.show}
        onMouseLeave={c.hide}
        onContextMenu={(e) => {
          e.preventDefault();
          e.stopPropagation();
          act((a) => a.sessionMenu(lane, session));
        }}
      >
        {!plain && (
          <span className="ca-sesslink-marks">
            <SessionGlyph s={session} size={10} />
            <SessionEmblem emblem={session.emblem} of={session.key} size={14} />
          </span>
        )}
        {children ?? session.title}
      </a>
      {c.open && c.anchor.current && <SessionLinkCard card={c} lane={lane} session={session} />}
    </>
  );
}

/** The card of a link to a session: the session as a panel lists one, and where it works. Its
 *  row shows it on a click, opens it where it runs from its arrow, and has its menu. */
function SessionLinkCard({ card, lane, session }: { card: HoverCardState; lane: Lane; session: Session }) {
  // Doing something from the card closes it first.
  const act = (f: () => void) => {
    card.close();
    f();
  };
  return (
    <HoverCard card={card} label={session.title} className="ca-sesshover">
      <div className="ca-tree" role="tree" aria-label="Session">
        <PanelSession
          lane={lane}
          session={session}
          now={Date.now()}
          onShow={(l, s) => act(() => linkHost.actions()?.showSession(l, s))}
          onOpen={(l, s) => act(() => linkHost.actions()?.openSession(l, s))}
          onMenu={(l, s) => linkHost.actions()?.sessionMenu(l, s)}
        />
      </div>
      <div className="ca-chips ca-sesshover-where">
        <WorktreeChip lane={lane} />
      </div>
    </HoverCard>
  );
}

/** `id`: the todo's id, or the start of only one's. */
export function TodoLink({ id, children, unknown, plain }: LinkProps & { id: string }) {
  const todo = useLinked(() => todoNamed(linkHost.todos(), id));
  if (!todo) return <>{unknown}</>;
  const title = todo.title || 'Untitled todo';
  return (
    <a
      className={plain ? 'ca-todolink is-plain' : 'ca-md-a ca-todolink'}
      href={todoLink(todo.id)}
      title={`Todo · ${STATUS_WORD[todo.status]}: ${title}\nClick to show it in Todo`}
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        const show = linkHost.actions()?.showTodo;
        if (show) show(todo);
        else void showMain({ kind: 'todo', id: todo.id });
      }}
    >
      {!plain && <TodoGlyph status={todo.status} size={10} />}
      {children ?? title}
    </a>
  );
}
